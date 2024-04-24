use images_to_video;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use tree_migration;

fn build_video_config(
    image_config: &tree_migration::Config,
    ffmpeg_path: &PathBuf,
    codec: images_to_video::Codec,
    frame_rate: u32,
    video_output_path: Option<PathBuf>,
) -> Result<images_to_video::Config, images_to_video::utils::Error> {
    let output_file_name = image_config.location.clone()
        + "-"
        + image_config.camera.as_str()
        + "-"
        + image_config.start_date.to_string().as_str()
        + "-"
        + image_config.end_date.to_string().as_str()
        + ".mov";

    images_to_video::build_config(
        ffmpeg_path.display().to_string().as_str(),
        image_config.output_path.display().to_string().as_str(),
        video_output_path,
        output_file_name.as_str(),
        frame_rate,
        codec,
    )
}
pub enum Signal {
    Success(PathBuf),
    Error((PathBuf, tree_migration::Error)),
}

#[derive(PartialEq)]
pub enum AppState {
    Init,
    InvalidConfigs,
    ValidConfigs,
    Processing,
    ProcessingDone,
    ProcessingErrors,
}

#[derive(PartialEq)]
pub enum ItemState {
    InvalidConfig,
    ValidConfig,
    Processing,
    ProcessingDone,
    ProcessingError,
    Unkown,
}

fn item_state(
    app_state: &AppState,
    config: &Result<tree_migration::Config, tree_migration::Error>,
    done: &Option<Result<(), tree_migration::Error>>,
) -> ItemState {
    if done.as_ref().is_some_and(|d| d.is_ok()) {
        return ItemState::ProcessingDone;
    } else if done.as_ref().is_some_and(|d| d.is_err()) {
        return ItemState::ProcessingError;
    } else if config.is_ok() && done.is_none() && app_state == &AppState::Processing {
        return ItemState::Processing;
    } else if config.is_ok() {
        return ItemState::ValidConfig;
    } else if config.is_err() {
        return ItemState::InvalidConfig;
    }
    ItemState::Unkown
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MigrationApp {
    pub is_forest_green_enabled: bool,
    pub is_video_enabled: bool,
    pub video_codec: images_to_video::Codec,
    pub ffmpeg_path: Option<PathBuf>,
    pub video_output_path: Option<PathBuf>,
    pub frame_rate: u32,
    #[serde(skip)]
    pub state: AppState,
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
            is_forest_green_enabled: false,
            is_video_enabled: false,
            video_codec: images_to_video::Codec::None,
            ffmpeg_path: None,
            video_output_path: None,
            frame_rate: 4,
            state: AppState::Init,
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

            ui.checkbox(&mut self.is_forest_green_enabled, "Forest Green")
                .on_hover_text("Check to enable forest green");

            ui.add_space(10.0);

            ui.checkbox(&mut self.is_video_enabled, "Video processing")
                .on_hover_text("Check to enable video processing");

            ui.add_space(10.0);

            if self.is_video_enabled {
                if self.state == AppState::Processing {
                    ui.label(
                        "Settings cannot be changed while files are being processed".to_owned(),
                    );
                } else {
                    ui.horizontal(|ui| {
                        if ui.button("Select output folder").clicked() {
                            self.video_output_path = rfd::FileDialog::new().pick_folder();
                        }

                        if let Some(path) = &self.video_output_path {
                            ui.monospace(path.display().to_string());
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Video ouput path not set.".to_owned());
                            });
                        }
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("Select ffmpeg binary").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_file() {
                                self.ffmpeg_path = images_to_video::utils::ffmpeg_path(
                                    path.display().to_string().as_str(),
                                )
                                .ok();
                            }
                        }

                        if let Some(path) = &self.ffmpeg_path {
                            ui.monospace(path.display().to_string());
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Not set. You can download ffmpeg".to_owned());
                                ui.hyperlink_to(
                                    "here".to_owned(),
                                    "https://ffmpeg.org/download.html",
                                );
                            });
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
            }

            ui.add_space(10.0);
        });
    }

    pub fn build_drag_and_drop_view(&mut self, ctx: &egui::Context) {
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

    pub fn build_processing_view(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::TOP),
                    |ui| match self.state {
                        AppState::Processing => {
                            ui.spinner();
                        }
                        AppState::Init => {
                            ui.label("Nothing to process: No Config Files");
                        }
                        AppState::InvalidConfigs => {
                            ui.label("Cannot process: No or invalid Config Files");
                        }
                        AppState::ValidConfigs | AppState::ProcessingDone => {
                            if ui
                                .button(egui::RichText::new("Process").heading())
                                .clicked()
                            {
                                self.state = AppState::Processing;
                                self.process();
                            }
                        }
                        AppState::ProcessingErrors => {
                            ui.label(
                                egui::RichText::new("Processing error.".to_owned())
                                    .color(egui::Color32::RED),
                            );
                        }
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui.button(egui::RichText::new("Clear").heading()).clicked() {
                        self.dropped_files.clear();
                    }
                });
            });
            ui.add_space(10.0);
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
            if let Ok(image_config) = config {
                configs.push((path.clone(), image_config.clone()));
            }
        }

        for (path, image_config) in configs {
            let sender = self.channel.0.clone();
            let is_forest_green_enabled = self.is_forest_green_enabled;
            let is_video_enabled = self.is_video_enabled;
            let video_codec = self.video_codec.clone();
            let ffmpeg_path = self.ffmpeg_path.clone();
            let video_output_path = self.video_output_path.clone();
            let frame_rate = self.frame_rate;
            async_std::task::spawn(async move {
                match tree_migration::run(image_config.clone(), is_forest_green_enabled).await {
                    Ok(_) => {
                        if is_video_enabled
                            && video_codec != images_to_video::Codec::None
                            && ffmpeg_path.is_some()
                        {
                            let video_config_opt = match build_video_config(
                                &image_config,
                                &ffmpeg_path.as_ref().unwrap(),
                                video_codec.clone(),
                                frame_rate,
                                video_output_path,
                            ) {
                                Err(e) => {
                                    println!("Error Config {}", e);
                                    None
                                }
                                Ok(config) => Some(config),
                            };

                            if let Some(video_config) = video_config_opt {
                                if let Err(e) = images_to_video::run(video_config).await {
                                    println!("Eorrro {}", e);
                                }
                            }
                        }
                        let _ = sender.send(Signal::Success(path));
                    }
                    Err(e) => {
                        let _ = sender.send(Signal::Error((path, e)));
                    }
                }
            });
        }
    }

    fn update_state(&mut self) {
        if self.dropped_files.is_empty() {
            self.state = AppState::Init;
        } else {
            if self.state == AppState::Processing {
                if self
                    .dropped_files
                    .iter()
                    .find(|(_, (config, done))| {
                        item_state(&self.state, &config, &done) == ItemState::Processing
                    })
                    .is_none()
                {
                    self.state = AppState::ProcessingDone;
                } else if self
                    .dropped_files
                    .iter()
                    .find(|(_, (config, done))| {
                        item_state(&self.state, &config, &done) == ItemState::ProcessingError
                    })
                    .is_some()
                {
                    self.state = AppState::ProcessingErrors;
                }
            } else {
                if self
                    .dropped_files
                    .iter()
                    .find(|(_, (config, done))| {
                        item_state(&self.state, &config, &done) == ItemState::InvalidConfig
                    })
                    .is_none()
                {
                    self.state = AppState::ValidConfigs;
                } else {
                    self.state = AppState::InvalidConfigs;
                }
            }
        }
    }

    fn table_ui(&self, ui: &mut egui::Ui) {
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
                    let item_state = item_state(&self.state, &config, &done);
                    let status = match item_state {
                        ItemState::ProcessingDone => String::from("Done"),
                        ItemState::ProcessingError => String::from("Error"),
                        ItemState::ValidConfig => String::from("Valid Config"),
                        ItemState::InvalidConfig => String::from("Invalid Config"),
                        _ => String::from("Unkown"),
                    };
                    body.row(row_height, |mut row| {
                        row.col(|ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.vertical(|ui| {
                                if item_state == ItemState::Processing {
                                    ui.spinner();
                                } else {
                                    ui.label(status.clone());
                                }
                                if item_state == ItemState::ProcessingError {
                                    ui.label("");
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.vertical(|ui| {
                                ui.label(path.to_string_lossy());
                                if item_state == ItemState::InvalidConfig {
                                    ui.label(
                                        RichText::new(format!("{}", status)).color(Color32::RED),
                                    );
                                }
                                if item_state == ItemState::ProcessingError {
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

        self.update_state();

        self.build_settings_view(ctx);

        self.build_drag_and_drop_view(ctx);

        self.build_processing_view(ctx);
    }
}
