#![windows_subsystem = "windows"]

use eframe::egui;
use fasttail::config::FastTailConfig;
use fasttail::ui::FastTailApp;

fn main() -> eframe::Result<()> {
    fasttail::crash_handler::install_crash_handler();

    let config = FastTailConfig::load();
    let app_title = format!("FastTail v{} by Matteo Baccan", env!("CARGO_PKG_VERSION"));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app_title.clone())
        .with_min_inner_size([800.0, 500.0])
        .with_decorations(!config.borderless)
        .with_drag_and_drop(true);

    if let (Some(w), Some(h)) = (config.window_width, config.window_height) {
        viewport = viewport.with_inner_size([w, h]);
    } else {
        viewport = viewport.with_inner_size([1280.0, 800.0]);
    }

    if let (Some(x), Some(y)) = (config.window_x, config.window_y) {
        viewport = viewport.with_position([x, y]);
    }

    if config.window_maximized {
        viewport = viewport.with_maximized(true);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        &app_title,
        native_options,
        Box::new(|cc| Ok(Box::new(FastTailApp::new(cc)))),
    )
}
