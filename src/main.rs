#![windows_subsystem = "windows"]

use eframe::egui;
use fasttail::cli::{CliArgs, USAGE};
use fasttail::config::FastTailConfig;
use fasttail::renderer::{self, RendererChoice, RendererKind};
use fasttail::ui::FastTailApp;

#[cfg(windows)]
mod legacy_compat {
    #[repr(C)]
    pub struct FileTime {
        pub dw_low_date_time: u32,
        pub dw_high_date_time: u32,
    }

    type PreciseFn = unsafe extern "system" fn(*mut FileTime);
    type ProcessPrngFn = unsafe extern "system" fn(*mut u8, usize) -> i32;
    type WaitOnAddressFn = unsafe extern "system" fn(
        *const std::ffi::c_void,
        *const std::ffi::c_void,
        usize,
        u32,
    ) -> i32;
    type WakeByAddressFn = unsafe extern "system" fn(*const std::ffi::c_void);

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimeAsFileTime(lp_system_time_as_file_time: *mut FileTime);
        pub fn GetModuleHandleA(lp_module_name: *const u8) -> *mut std::ffi::c_void;
        pub fn LoadLibraryA(lp_lib_name: *const u8) -> *mut std::ffi::c_void;
        pub fn GetProcAddress(
            h_module: *mut std::ffi::c_void,
            lp_proc_name: *const u8,
        ) -> *mut std::ffi::c_void;
        fn Sleep(ms: u32);
    }

    // Dynamic GetDpiForSystem (Win10 1607+) or fallback to 96 (Win7/2008 R2)
    pub unsafe fn get_system_dpi() -> u32 {
        let mut user32 = GetModuleHandleA(b"user32.dll\0".as_ptr());
        if user32.is_null() {
            user32 = LoadLibraryA(b"user32.dll\0".as_ptr());
        }
        if !user32.is_null() {
            let proc = GetProcAddress(user32, b"GetDpiForSystem\0".as_ptr());
            if !proc.is_null() {
                let get_dpi: unsafe extern "system" fn() -> u32 = std::mem::transmute(proc);
                return get_dpi();
            }
        }
        96
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_GetDpiForSystem() -> u32 {
        get_system_dpi()
    }

    #[no_mangle]
    pub static mut __imp_GetDpiForSystem: unsafe extern "system" fn() -> u32 = hook_GetDpiForSystem;

    // 1. Hook GetSystemTimePreciseAsFileTime -> fallback to GetSystemTimeAsFileTime on Win7/2008R2
    static RESOLVED_TIME: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_TIME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    pub unsafe extern "system" fn hook_GetSystemTimePreciseAsFileTime(ft: *mut FileTime) {
        if !CHECKED_TIME.load(std::sync::atomic::Ordering::Acquire) {
            let kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
            if !kernel32.is_null() {
                let proc = GetProcAddress(kernel32, b"GetSystemTimePreciseAsFileTime\0".as_ptr());
                RESOLVED_TIME.store(proc, std::sync::atomic::Ordering::Release);
            }
            CHECKED_TIME.store(true, std::sync::atomic::Ordering::Release);
        }

        let ptr = RESOLVED_TIME.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: PreciseFn = std::mem::transmute(ptr);
            f(ft);
        } else {
            GetSystemTimeAsFileTime(ft);
        }
    }

    #[no_mangle]
    pub static mut __imp_GetSystemTimePreciseAsFileTime: PreciseFn =
        hook_GetSystemTimePreciseAsFileTime;

    // 2. Hook ProcessPrng -> fallback to SystemFunction036 (RtlGenRandom) in advapi32.dll on Win7/2008R2
    static RESOLVED_PRNG: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_PRNG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    pub unsafe extern "system" fn hook_ProcessPrng(buffer: *mut u8, size: usize) -> i32 {
        if !CHECKED_PRNG.load(std::sync::atomic::Ordering::Acquire) {
            let bcrypt = GetModuleHandleA(b"bcryptprimitives.dll\0".as_ptr());
            if !bcrypt.is_null() {
                let proc = GetProcAddress(bcrypt, b"ProcessPrng\0".as_ptr());
                RESOLVED_PRNG.store(proc, std::sync::atomic::Ordering::Release);
            }
            CHECKED_PRNG.store(true, std::sync::atomic::Ordering::Release);
        }

        let ptr = RESOLVED_PRNG.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: ProcessPrngFn = std::mem::transmute(ptr);
            f(buffer, size)
        } else {
            let advapi = GetModuleHandleA(b"advapi32.dll\0".as_ptr());
            if !advapi.is_null() {
                let proc = GetProcAddress(advapi, b"SystemFunction036\0".as_ptr());
                if !proc.is_null() {
                    let rtl_gen_random: unsafe extern "system" fn(*mut u8, u32) -> u8 =
                        std::mem::transmute(proc);
                    if rtl_gen_random(buffer, size as u32) != 0 {
                        return 1;
                    }
                }
            }
            0
        }
    }

    #[no_mangle]
    pub static mut __imp_ProcessPrng: ProcessPrngFn = hook_ProcessPrng;

    // 3. Hook WaitOnAddress, WakeByAddressSingle, WakeByAddressAll -> fallback to polling Sleep on Win7/2008R2
    static RESOLVED_WAIT: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_WAIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    pub unsafe extern "system" fn hook_WaitOnAddress(
        address: *const std::ffi::c_void,
        compare_address: *const std::ffi::c_void,
        address_size: usize,
        dw_milliseconds: u32,
    ) -> i32 {
        if !CHECKED_WAIT.load(std::sync::atomic::Ordering::Acquire) {
            let synch = GetModuleHandleA(b"api-ms-win-core-synch-l1-2-0.dll\0".as_ptr());
            let kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
            let proc = if !synch.is_null() {
                GetProcAddress(synch, b"WaitOnAddress\0".as_ptr())
            } else if !kernel32.is_null() {
                GetProcAddress(kernel32, b"WaitOnAddress\0".as_ptr())
            } else {
                std::ptr::null_mut()
            };
            RESOLVED_WAIT.store(proc, std::sync::atomic::Ordering::Release);
            CHECKED_WAIT.store(true, std::sync::atomic::Ordering::Release);
        }

        let ptr = RESOLVED_WAIT.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WaitOnAddressFn = std::mem::transmute(ptr);
            f(address, compare_address, address_size, dw_milliseconds)
        } else {
            let start = std::time::Instant::now();
            let timeout = if dw_milliseconds == u32::MAX {
                std::time::Duration::from_secs(u64::MAX)
            } else {
                std::time::Duration::from_millis(dw_milliseconds as u64)
            };
            loop {
                let equal = match address_size {
                    1 => *(address as *const u8) == *(compare_address as *const u8),
                    2 => *(address as *const u16) == *(compare_address as *const u16),
                    4 => *(address as *const u32) == *(compare_address as *const u32),
                    8 => *(address as *const u64) == *(compare_address as *const u64),
                    _ => {
                        let a = std::slice::from_raw_parts(address as *const u8, address_size);
                        let b =
                            std::slice::from_raw_parts(compare_address as *const u8, address_size);
                        a == b
                    }
                };
                if !equal || start.elapsed() >= timeout {
                    break;
                }
                Sleep(1);
            }
            1
        }
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_WakeByAddressSingle(address: *const std::ffi::c_void) {
        static RESOLVED_WAKE1: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        static CHECKED_WAKE1: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !CHECKED_WAKE1.load(std::sync::atomic::Ordering::Acquire) {
            let kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
            let proc = if !kernel32.is_null() {
                GetProcAddress(kernel32, b"WakeByAddressSingle\0".as_ptr())
            } else {
                std::ptr::null_mut()
            };
            RESOLVED_WAKE1.store(proc, std::sync::atomic::Ordering::Release);
            CHECKED_WAKE1.store(true, std::sync::atomic::Ordering::Release);
        }
        let ptr = RESOLVED_WAKE1.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WakeByAddressFn = std::mem::transmute(ptr);
            f(address);
        }
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_WakeByAddressAll(address: *const std::ffi::c_void) {
        static RESOLVED_WAKEALL: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
            std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        static CHECKED_WAKEALL: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !CHECKED_WAKEALL.load(std::sync::atomic::Ordering::Acquire) {
            let kernel32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
            let proc = if !kernel32.is_null() {
                GetProcAddress(kernel32, b"WakeByAddressAll\0".as_ptr())
            } else {
                std::ptr::null_mut()
            };
            RESOLVED_WAKEALL.store(proc, std::sync::atomic::Ordering::Release);
            CHECKED_WAKEALL.store(true, std::sync::atomic::Ordering::Release);
        }
        let ptr = RESOLVED_WAKEALL.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WakeByAddressFn = std::mem::transmute(ptr);
            f(address);
        }
    }

    #[no_mangle]
    pub static mut __imp_WaitOnAddress: WaitOnAddressFn = hook_WaitOnAddress;
    #[no_mangle]
    pub static mut __imp_WakeByAddressSingle: WakeByAddressFn = hook_WakeByAddressSingle;
    #[no_mangle]
    pub static mut __imp_WakeByAddressAll: WakeByAddressFn = hook_WakeByAddressAll;
}

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
    }
    const MONITOR_DEFAULTTONULL: u32 = 0;

    if !x.is_finite() || !y.is_finite() {
        return false;
    }
    // Probe a point inside the title bar so a window hugging a monitor edge still counts
    let scale = unsafe { legacy_compat::get_system_dpi() } as f32 / 96.0;
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
                present_mode: eframe::egui_wgpu::wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: Some(2),
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
