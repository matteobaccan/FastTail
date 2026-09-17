#![windows_subsystem = "windows"]

use eframe::egui;
use fasttail::config::FastTailConfig;
use fasttail::ui::FastTailApp;

/// True when the point just inside the top-left corner of a window placed at (`x`, `y`)
/// logical points falls on an attached monitor.
#[cfg(windows)]
fn saved_position_is_visible(x: f32, y: f32) -> bool {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn MonitorFromPoint(pt: Point, flags: u32) -> *mut std::ffi::c_void;
        fn GetDpiForSystem() -> u32;
    }
    const MONITOR_DEFAULTTONULL: u32 = 0;

    if !x.is_finite() || !y.is_finite() {
        return false;
    }
    // Probe a point inside the title bar so a window hugging a monitor edge still counts
    let scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let probe = Point {
        x: ((x + 40.0) * scale) as i32,
        y: ((y + 20.0) * scale) as i32,
    };
    !unsafe { MonitorFromPoint(probe, MONITOR_DEFAULTTONULL) }.is_null()
}

#[cfg(not(windows))]
fn saved_position_is_visible(x: f32, y: f32) -> bool {
    x.is_finite() && y.is_finite()
}

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
        // A position saved on a monitor that is no longer attached would leave the window
        // invisible; in that case let the OS pick a default position instead.
        if saved_position_is_visible(x, y) {
            viewport = viewport.with_position([x, y]);
        }
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
