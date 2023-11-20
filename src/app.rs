use images_to_video;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use tree_migration;

pub enum Signal {
    Success(PathBuf),
    Error((PathBuf, tree_migration::Error)),
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MigrationApp {
    pub is_video_enabled: bool,
    pub video_codec: images_to_video::Codec,
    pub ffmpeg_path: Option<PathBuf>,
    pub frame_rate: u32,
    #[serde(skip)]
    pub is_processing: bool,
    #[serde(skip)]
    pub channel: (mpsc::Sender<Signal>, mpsc::Receiver<Signal>),
    #[serde(skip)]
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
            is_video_enabled: false,
            video_codec: images_to_video::Codec::None,
            ffmpeg_path: None,
            frame_rate: 4,
            is_processing: false,
            channel: mpsc::channel::<Signal>(),
            dropped_files: HashMap::new(),
        }
    }
}

impl MigrationApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            let mut app: MigrationApp =
                eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
            if let Some(path) = &app.ffmpeg_path {
                if !path.exists() {
                    app.ffmpeg_path = None;
                }
            }
            return app;
        }

        Default::default()
    }

    pub fn build_settings_view(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(10.0);

            ui.checkbox(&mut self.is_video_enabled, "Video processing")
                .on_hover_text("Check to enable video processing");

            ui.add_space(10.0);

            if self.is_video_enabled {
                ui.horizontal(|ui| {
                    if ui.button("Select ffmpeg binary").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            if let Ok(ffmpeg_path) = images_to_video::utils::ffmpeg_path(
                                path.display().to_string().as_str(),
                            ) {
                                self.ffmpeg_path = Some(ffmpeg_path);
                            } else {
                                self.ffmpeg_path = None;
                            }
                        }
                    }

                    if let Some(path) = &self.ffmpeg_path {
                        ui.monospace(path.display().to_string());
                    } else {
                        ui.label(egui::RichText::new("Not Set").color(egui::Color32::RED));
                    }
                });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Video Codec")
                        .selected_text(match self.video_codec {
                            images_to_video::Codec::H264 => "h.264",
                            images_to_video::Codec::ProRes => "Prores",
                            images_to_video::Codec::None => "None",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.video_codec,
                                images_to_video::Codec::H264,
                                "h.264",
                            );
                            ui.selectable_value(
                                &mut self.video_codec,
                                images_to_video::Codec::ProRes,
                                "Prores",
                            );
                        });
                });

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.frame_rate, 1..=25));
                    ui.label("Frame Rate".to_owned());
                });
            }

            ui.add_space(10.0);
        });
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
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

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

        self.build_settings_view(ctx);

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
