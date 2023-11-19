extern crate images_to_video;
extern crate tree_migration;

mod app;

use app::MigrationApp;

fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let native_options = eframe::NativeOptions {
        initial_window_size: Some([700.0, 500.0].into()),
        min_window_size: Some([300.0, 220.0].into()),
        ..Default::default()
    };
    eframe::run_native(
        "Tree Migration",
        native_options,
        Box::new(|cc| Box::new(MigrationApp::new(cc))),
    )
}
