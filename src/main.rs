#![windows_subsystem = "windows"]

use eframe::egui;
use fasttail::config::FastTailConfig;
use fasttail::ui::FastTailApp;

fn main() -> eframe::Result<()> {
    fasttail::crash_handler::install_crash_handler();

    let config = FastTailConfig::load();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FastTail by Matteo Baccan")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_decorations(!config.borderless)
            .with_drag_and_drop(true),
        ..Default::default()
    };

    eframe::run_native(
        "FastTail by Matteo Baccan",
        native_options,
        Box::new(|cc| Ok(Box::new(FastTailApp::new(cc)))),
    )
}
