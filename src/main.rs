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
    type NtKeyedEventFn = unsafe extern "system" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        u8,
        *mut std::ffi::c_void,
    ) -> u32;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimeAsFileTime(lp_system_time_as_file_time: *mut FileTime);
        pub fn GetModuleHandleA(lp_module_name: *const u8) -> *mut std::ffi::c_void;
        pub fn LoadLibraryA(lp_lib_name: *const u8) -> *mut std::ffi::c_void;
        pub fn GetProcAddress(
            h_module: *mut std::ffi::c_void,
            lp_proc_name: *const u8,
        ) -> *mut std::ffi::c_void;
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

    // 2. Hook CoTaskMemFree -> redirect from combase.dll (Win8+) to ole32.dll (Win7/2008R2)
    static RESOLVED_COTASK: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_COTASK: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    pub unsafe extern "system" fn hook_CoTaskMemFree(pv: *mut std::ffi::c_void) {
        if !CHECKED_COTASK.load(std::sync::atomic::Ordering::Acquire) {
            let mut ole32 = GetModuleHandleA(b"ole32.dll\0".as_ptr());
            if ole32.is_null() {
                ole32 = LoadLibraryA(b"ole32.dll\0".as_ptr());
            }
            if !ole32.is_null() {
                let proc = GetProcAddress(ole32, b"CoTaskMemFree\0".as_ptr());
                RESOLVED_COTASK.store(proc, std::sync::atomic::Ordering::Release);
            }
            CHECKED_COTASK.store(true, std::sync::atomic::Ordering::Release);
        }

        let ptr = RESOLVED_COTASK.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: unsafe extern "system" fn(*mut std::ffi::c_void) = std::mem::transmute(ptr);
            f(pv);
        }
    }

    #[no_mangle]
    pub static mut __imp_CoTaskMemFree: unsafe extern "system" fn(*mut std::ffi::c_void) =
        hook_CoTaskMemFree;

    // 3. Hook ProcessPrng -> redirect from bcryptprimitives.dll (Win8+) to advapi32.dll!SystemFunction036 (Win7/2008R2)
    static RESOLVED_PRNG: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_PRNG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    #[no_mangle]
    pub unsafe extern "system" fn hook_ProcessPrng(buffer: *mut u8, size: usize) -> i32 {
        if !CHECKED_PRNG.load(std::sync::atomic::Ordering::Acquire) {
            let mut bcrypt = GetModuleHandleA(b"bcryptprimitives.dll\0".as_ptr());
            if bcrypt.is_null() {
                bcrypt = LoadLibraryA(b"bcryptprimitives.dll\0".as_ptr());
            }
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
            let mut advapi = GetModuleHandleA(b"advapi32.dll\0".as_ptr());
            if advapi.is_null() {
                advapi = LoadLibraryA(b"advapi32.dll\0".as_ptr());
            }
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

    // 4. Hook WaitOnAddress, WakeByAddressSingle, WakeByAddressAll -> redirect to KernelBase.dll (Win8+) or ntdll!Nt*KeyedEvent (Win7/2008R2)
    static RESOLVED_WAIT: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static RESOLVED_WAKE1: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static RESOLVED_WAKEALL: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static RESOLVED_NT_WAIT: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static RESOLVED_NT_REL: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
    static CHECKED_SYNCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    unsafe fn init_synch() {
        if !CHECKED_SYNCH.load(std::sync::atomic::Ordering::Acquire) {
            let mut kb = GetModuleHandleA(b"KernelBase.dll\0".as_ptr());
            if kb.is_null() {
                kb = LoadLibraryA(b"KernelBase.dll\0".as_ptr());
            }
            if !kb.is_null() {
                let p_wait = GetProcAddress(kb, b"WaitOnAddress\0".as_ptr());
                let p_wake1 = GetProcAddress(kb, b"WakeByAddressSingle\0".as_ptr());
                let p_wakeall = GetProcAddress(kb, b"WakeByAddressAll\0".as_ptr());
                RESOLVED_WAIT.store(p_wait, std::sync::atomic::Ordering::Release);
                RESOLVED_WAKE1.store(p_wake1, std::sync::atomic::Ordering::Release);
                RESOLVED_WAKEALL.store(p_wakeall, std::sync::atomic::Ordering::Release);
            }
            let mut ntdll = GetModuleHandleA(b"ntdll.dll\0".as_ptr());
            if ntdll.is_null() {
                ntdll = LoadLibraryA(b"ntdll.dll\0".as_ptr());
            }
            if !ntdll.is_null() {
                let p_ntwait = GetProcAddress(ntdll, b"NtWaitForKeyedEvent\0".as_ptr());
                let p_ntrel = GetProcAddress(ntdll, b"NtReleaseKeyedEvent\0".as_ptr());
                RESOLVED_NT_WAIT.store(p_ntwait, std::sync::atomic::Ordering::Release);
                RESOLVED_NT_REL.store(p_ntrel, std::sync::atomic::Ordering::Release);
            }
            CHECKED_SYNCH.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_WaitOnAddress(
        address: *const std::ffi::c_void,
        compare_address: *const std::ffi::c_void,
        address_size: usize,
        dw_milliseconds: u32,
    ) -> i32 {
        init_synch();
        let ptr = RESOLVED_WAIT.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WaitOnAddressFn = std::mem::transmute(ptr);
            f(address, compare_address, address_size, dw_milliseconds)
        } else {
            let equal = match address_size {
                1 => *(address as *const u8) == *(compare_address as *const u8),
                2 => *(address as *const u16) == *(compare_address as *const u16),
                4 => *(address as *const u32) == *(compare_address as *const u32),
                8 => *(address as *const u64) == *(compare_address as *const u64),
                _ => {
                    let a = std::slice::from_raw_parts(address as *const u8, address_size);
                    let b = std::slice::from_raw_parts(compare_address as *const u8, address_size);
                    a == b
                }
            };
            if !equal {
                return 1;
            }
            let nt_wait_ptr = RESOLVED_NT_WAIT.load(std::sync::atomic::Ordering::Relaxed);
            if !nt_wait_ptr.is_null() {
                let nt_wait: NtKeyedEventFn = std::mem::transmute(nt_wait_ptr);
                let mut timeout_val: i64 = if dw_milliseconds == u32::MAX {
                    0
                } else {
                    -((dw_milliseconds as i64) * 10_000)
                };
                let timeout_ptr = if dw_milliseconds == u32::MAX {
                    std::ptr::null_mut()
                } else {
                    &mut timeout_val as *mut i64 as *mut std::ffi::c_void
                };
                let status = nt_wait(
                    std::ptr::null_mut(),
                    address as *mut std::ffi::c_void,
                    0,
                    timeout_ptr,
                );
                if status == 0 {
                    1
                } else {
                    0
                }
            } else {
                1
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_WakeByAddressSingle(address: *const std::ffi::c_void) {
        init_synch();
        let ptr = RESOLVED_WAKE1.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WakeByAddressFn = std::mem::transmute(ptr);
            f(address);
        } else {
            let nt_rel_ptr = RESOLVED_NT_REL.load(std::sync::atomic::Ordering::Relaxed);
            if !nt_rel_ptr.is_null() {
                let nt_rel: NtKeyedEventFn = std::mem::transmute(nt_rel_ptr);
                let mut zero_timeout: i64 = 0;
                nt_rel(
                    std::ptr::null_mut(),
                    address as *mut std::ffi::c_void,
                    0,
                    &mut zero_timeout as *mut i64 as *mut std::ffi::c_void,
                );
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "system" fn hook_WakeByAddressAll(address: *const std::ffi::c_void) {
        init_synch();
        let ptr = RESOLVED_WAKEALL.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: WakeByAddressFn = std::mem::transmute(ptr);
            f(address);
        } else {
            let nt_rel_ptr = RESOLVED_NT_REL.load(std::sync::atomic::Ordering::Relaxed);
            if !nt_rel_ptr.is_null() {
                let nt_rel: NtKeyedEventFn = std::mem::transmute(nt_rel_ptr);
                let mut zero_timeout: i64 = 0;
                while nt_rel(
                    std::ptr::null_mut(),
                    address as *mut std::ffi::c_void,
                    0,
                    &mut zero_timeout as *mut i64 as *mut std::ffi::c_void,
                ) == 0
                {}
            }
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

/// Allocate or attach a console for the TUI interface when running as a Windows GUI subsystem.
#[cfg(windows)]
fn init_tui_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
        }
    }
}

#[cfg(not(windows))]
fn init_tui_console() {}

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

    let use_tui = if cli.gui {
        false
    } else if cli.tui {
        true
    } else {
        config.ui_mode == fasttail::config::UiMode::Tui
    };

    if use_tui {
        init_tui_console();
        if let Err(err) = fasttail::tui::run_tui(cli, config) {
            eprintln!("FastTail TUI error: {err}");
            std::process::exit(1);
        }
        return Ok(());
    }

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
                    let second =
                        eframe::run_native(&app_title, make_options(eframe::Renderer::Glow), {
                            let cli = cli.clone();
                            Box::new(move |cc| Ok(Box::new(FastTailApp::new(cc, cli))))
                        });
                    match second {
                        Err(err2) if !renderer::app_created() => {
                            eprintln!("renderer: glow backend also failed to start: {err2}");
                            eprintln!("renderer: falling back to terminal user interface (TUI)");
                            init_tui_console();
                            if let Err(tui_err) = fasttail::tui::run_tui(cli, config) {
                                eprintln!("FastTail TUI error: {tui_err}");
                                std::process::exit(1);
                            }
                            Ok(())
                        }
                        other => other,
                    }
                }
                other => other,
            }
        }
    }
}
