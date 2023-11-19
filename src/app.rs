use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use tree_migration;

pub enum Signal {
    Success(PathBuf),
    Error((PathBuf, tree_migration::Error)),
}

pub struct MigrationApp {
    // #[serde(skip)] // This how you opt-out of serialization of a field
    pub is_processing: bool,
    pub channel: (mpsc::Sender<Signal>, mpsc::Receiver<Signal>),
    pub dropped_files: HashMap<
        PathBuf,
        (
            Result<tree_migration::Config, tree_migration::Error>,
            Option<Result<(), tree_migration::Error>>,
        ),
    >,
}

impl Default for MigrationApp {
    fn default() -> Self {
        Self {
            is_processing: false,
            channel: mpsc::channel::<Signal>(),
            dropped_files: HashMap::new(),
        }
    }
}

impl MigrationApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }

    pub fn drag_and_drop(&mut self, ctx: &egui::Context) {
        use egui::*;
        CentralPanel::default().show(ctx, |ui| {
            // Collect dropped files:
            if !ctx.input(|input| input.raw.dropped_files.is_empty()) {
                let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
                for file in dropped_files {
                    let config = tree_migration::Config::from(&file.path.as_ref().unwrap());
                    self.dropped_files
                        .insert(file.path.unwrap(), (config, None));
                }
            }
            use egui_extras::{Size, StripBuilder};
            StripBuilder::new(ui)
                .size(Size::remainder().at_least(100.0)) // for the table
                .size(Size::exact(10.5)) // for the source code link
                .vertical(|mut strip| {
                    strip.cell(|ui| {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            self.table_ui(ui);
                        });
                    });
                });
        });
    }

    pub fn poll(&mut self) {
        while let Ok(signal) = self.channel.1.try_recv() {
            match signal {
                Signal::Success(path) => {
                    if self.dropped_files.contains_key(&path) {
                        self.dropped_files
                            .entry(path)
                            .and_modify(|value| value.1 = Some(Ok(())));
                    }
                }
                Signal::Error((path, error)) => {
                    if self.dropped_files.contains_key(&path) {
                        self.dropped_files
                            .entry(path)
                            .and_modify(|value| value.1 = Some(Err(error)));
                    }
                }
            }
        }
    }

    pub fn process(&self) {
        let mut configs: Vec<(PathBuf, tree_migration::Config)> = Vec::new();
        for (path, (config, _)) in &self.dropped_files {
            if let Ok(c) = config {
                configs.push((path.clone(), c.clone()));
            }
        }

        for (path, config) in configs {
            let sender = self.channel.0.clone();
            async_std::task::spawn(async move {
                match tree_migration::run(config.clone()).await {
                    Ok(_) => {
                        let _ = sender.send(Signal::Success(path));
                    }
                    Err(e) => {
                        let _ = sender.send(Signal::Error((path, e)));
                    }
                }
            });
        }
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        use egui::*;
        use egui_extras::{Column, TableBuilder};

        let table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(100.0).range(40.0..=300.0))
            .column(Column::remainder())
            .min_scrolled_height(0.0);

        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Status");
                });
                header.col(|ui| {
                    ui.strong("Path");
                });
            })
            .body(|mut body| {
                for (path, (config, done)) in &self.dropped_files {
                    let row_height = 18.0;
                    let status = if done.as_ref().is_some_and(|d| d.is_ok()) {
                        String::from("Done")
                    } else if done.as_ref().is_some_and(|d| d.is_err()) {
                        String::from("Error")
                    } else if config.is_ok() {
                        String::from("Valid Config")
                    } else if config.is_err() {
                        String::from("Invalid Config")
                    } else {
                        String::from("Unkown")
                    };
                    body.row(row_height, |mut row| {
                        row.col(|ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.vertical(|ui| {
                                ui.label(status);
                                if done.as_ref().is_some_and(|d| d.is_err()) {
                                    ui.label("");
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.vertical(|ui| {
                                ui.label(path.to_string_lossy());
                                if done.as_ref().is_some_and(|d| d.is_err()) {
                                    if let Err(message) = done.as_ref().unwrap() {
                                        ui.label(
                                            RichText::new(format!("{}", message))
                                                .color(Color32::RED),
                                        );
                                    }
                                }
                            });
                        });
                    });
                }
            });
    }
}

impl eframe::App for MigrationApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        let mut is_processing = false;
        for (_, (config, done)) in &self.dropped_files {
            if config.is_ok() && done.is_none() {
                is_processing = true;
            }
        }
        if !is_processing {
            self.is_processing = false;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.heading("Drag Configuration Files onto Window");
            ui.add_space(10.0);
        });

        self.drag_and_drop(ctx);

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    if self.is_processing {
                        ui.spinner();
                    } else {
                        if ui
                            .button(egui::RichText::new("Process").heading())
                            .clicked()
                        {
                            self.is_processing = true;
                            self.process();
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui.button(egui::RichText::new("Clear").heading()).clicked() {
                        self.dropped_files.clear();
                    }
                });
            });
            ui.add_space(10.0);
        });
    }
}
