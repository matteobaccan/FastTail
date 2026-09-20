#![windows_subsystem = "windows"]

use eframe::egui;
use fasttail::cli::{CliArgs, USAGE};
use fasttail::config::FastTailConfig;
use fasttail::renderer::{self, RendererChoice, RendererKind};
use fasttail::ui::FastTailApp;

/// GUI-subsystem executables have no console; attach the parent's so `--help` and
/// `--version` are visible when launched from a terminal.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    // SAFETY: plain Win32 call; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

/// Parses the command line; prints help/version or a usage error and exits when asked to.
fn parse_command_line() -> CliArgs {
    let cli = match CliArgs::from_env() {
        Ok(cli) => cli,
        Err(err) => {
            attach_parent_console();
            eprintln!("fasttail: {err}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if cli.show_help || cli.show_version {
        attach_parent_console();
        if cli.show_version {
            println!("fasttail {}", env!("CARGO_PKG_VERSION"));
        }
        if cli.show_help {
            print!("{USAGE}");
        }
        std::process::exit(0);
    }
    if let Some(cfg) = &cli.config {
        // The config loader reads FASTTAIL_CONFIG first; set it before anything loads it.
        std::env::set_var("FASTTAIL_CONFIG", cfg);
    }
    cli
}

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

    let cli = parse_command_line();
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

    let make_options = |renderer: eframe::Renderer| eframe::NativeOptions {
        viewport: viewport.clone(),
        renderer,
        // Drivers (including NVIDIA on Windows among them) busy-wait inside presentation
        // while waiting for the vertical blank, which costs a whole core whenever egui
        // repaints continuously (e.g. during mouse moves). egui only repaints on demand,
        // so disabling vsync on both the OpenGL and wgpu paths trades tearing on a UI that
        // hardly animates for a drastically lower CPU cost.
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: false,
            ..Default::default()
        },
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            surface: eframe::egui_wgpu::SurfaceConfig {
                present_mode: eframe::egui_wgpu::wgpu::PresentMode::AutoNoVsync,
                desired_maximum_frame_latency: Some(1),
            },
            ..Default::default()
        },
        ..Default::default()
    };

    // wgpu first, OpenGL on failure (or whichever backend was forced).
    let choice = cli
        .renderer
        .unwrap_or_else(|| RendererChoice::from_env(config.renderer));
    match choice {
        RendererChoice::Wgpu => {
            renderer::mark_starting(RendererKind::Wgpu, false);
            eframe::run_native(&app_title, make_options(eframe::Renderer::Wgpu), {
                let cli = cli.clone();
                Box::new(move |cc| Ok(Box::new(FastTailApp::new(cc, cli))))
            })
        }
        RendererChoice::Glow => {
            renderer::mark_starting(RendererKind::Glow, false);
            eframe::run_native(&app_title, make_options(eframe::Renderer::Glow), {
                let cli = cli.clone();
                Box::new(move |cc| Ok(Box::new(FastTailApp::new(cc, cli))))
            })
        }
        RendererChoice::Auto => {
            renderer::mark_starting(RendererKind::Wgpu, false);
            let first = eframe::run_native(&app_title, make_options(eframe::Renderer::Wgpu), {
                let cli = cli.clone();
                Box::new(move |cc| Ok(Box::new(FastTailApp::new(cc, cli))))
            });
            match first {
                Err(err) if !renderer::app_created() => {
                    eprintln!("renderer: wgpu backend failed to start: {err}");
                    eprintln!("renderer: falling back to OpenGL");
                    renderer::mark_starting(RendererKind::Glow, true);
                    eframe::run_native(&app_title, make_options(eframe::Renderer::Glow), {
                        let cli = cli.clone();
                        Box::new(move |cc| Ok(Box::new(FastTailApp::new(cc, cli))))
                    })
                }
                other => other,
            }
        }
    }
}
