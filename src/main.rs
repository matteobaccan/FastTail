// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

#![windows_subsystem = "windows"]

use eframe::{egui, egui_wgpu::wgpu};
use fasttail::cli::{CliArgs, USAGE};
use fasttail::config::{FastTailConfig, Interface};
use fasttail::i18n::Language;
use fasttail::renderer::{self, RendererChoice, RendererKind};
use fasttail::ui::FastTailApp;
use std::sync::Arc;

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
        pub fn GetModuleHandleA(lp_module_name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        pub fn LoadLibraryA(lp_lib_name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        pub fn GetProcAddress(
            h_module: *mut std::ffi::c_void,
            lp_proc_name: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
    }

    // Dynamic GetDpiForSystem (Win10 1607+) or fallback to 96 (Win7/2008 R2)
    pub unsafe fn get_system_dpi() -> u32 {
        let mut user32 = GetModuleHandleA(c"user32.dll".as_ptr());
        if user32.is_null() {
            user32 = LoadLibraryA(c"user32.dll".as_ptr());
        }
        if !user32.is_null() {
            let proc = GetProcAddress(user32, c"GetDpiForSystem".as_ptr());
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
            let kernel32 = GetModuleHandleA(c"kernel32.dll".as_ptr());
            if !kernel32.is_null() {
                let proc = GetProcAddress(kernel32, c"GetSystemTimePreciseAsFileTime".as_ptr());
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
            let mut ole32 = GetModuleHandleA(c"ole32.dll".as_ptr());
            if ole32.is_null() {
                ole32 = LoadLibraryA(c"ole32.dll".as_ptr());
            }
            if !ole32.is_null() {
                let proc = GetProcAddress(ole32, c"CoTaskMemFree".as_ptr());
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
            let mut bcrypt = GetModuleHandleA(c"bcryptprimitives.dll".as_ptr());
            if bcrypt.is_null() {
                bcrypt = LoadLibraryA(c"bcryptprimitives.dll".as_ptr());
            }
            if !bcrypt.is_null() {
                let proc = GetProcAddress(bcrypt, c"ProcessPrng".as_ptr());
                RESOLVED_PRNG.store(proc, std::sync::atomic::Ordering::Release);
            }
            CHECKED_PRNG.store(true, std::sync::atomic::Ordering::Release);
        }

        let ptr = RESOLVED_PRNG.load(std::sync::atomic::Ordering::Relaxed);
        if !ptr.is_null() {
            let f: ProcessPrngFn = std::mem::transmute(ptr);
            f(buffer, size)
        } else {
            let mut advapi = GetModuleHandleA(c"advapi32.dll".as_ptr());
            if advapi.is_null() {
                advapi = LoadLibraryA(c"advapi32.dll".as_ptr());
            }
            if !advapi.is_null() {
                let proc = GetProcAddress(advapi, c"SystemFunction036".as_ptr());
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
            let mut kb = GetModuleHandleA(c"KernelBase.dll".as_ptr());
            if kb.is_null() {
                kb = LoadLibraryA(c"KernelBase.dll".as_ptr());
            }
            if !kb.is_null() {
                let p_wait = GetProcAddress(kb, c"WaitOnAddress".as_ptr());
                let p_wake1 = GetProcAddress(kb, c"WakeByAddressSingle".as_ptr());
                let p_wakeall = GetProcAddress(kb, c"WakeByAddressAll".as_ptr());
                RESOLVED_WAIT.store(p_wait, std::sync::atomic::Ordering::Release);
                RESOLVED_WAKE1.store(p_wake1, std::sync::atomic::Ordering::Release);
                RESOLVED_WAKEALL.store(p_wakeall, std::sync::atomic::Ordering::Release);
            }
            let mut ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr());
            if ntdll.is_null() {
                ntdll = LoadLibraryA(c"ntdll.dll".as_ptr());
            }
            if !ntdll.is_null() {
                let p_ntwait = GetProcAddress(ntdll, c"NtWaitForKeyedEvent".as_ptr());
                let p_ntrel = GetProcAddress(ntdll, c"NtReleaseKeyedEvent".as_ptr());
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
/// `--version` are visible when launched from a terminal. Output already redirected to a
/// file or a pipe stays there.
fn attach_parent_console() {
    fasttail::print_mode::console::attach_if_missing();
}

/// Parses the command line; prints help/version or a usage error and exits when asked to.
fn parse_command_line() -> CliArgs {
    let mut cli = match CliArgs::from_env() {
        Ok(cli) => cli,
        Err(err) => {
            attach_parent_console();
            eprintln!("fasttail: {err}\n\n{USAGE}");
            fasttail::print_mode::console::release();
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
        fasttail::print_mode::console::release();
        std::process::exit(0);
    }
    if let Some(cfg) = &cli.config {
        // The config loader reads FASTTAIL_CONFIG first; set it before anything loads it.
        std::env::set_var("FASTTAIL_CONFIG", cfg);
    }
    if cli.print {
        // Headless: no window, no workspace, no spool; the configuration is only read.
        let code = fasttail::print_mode::run(&cli);
        fasttail::print_mode::console::release();
        std::process::exit(code);
    }
    if cli.stdin && fasttail::stdin_source::classify() != fasttail::stdin_source::StdinKind::Piped {
        // Reported now, while the parent console can still be attached; the rest of the
        // startup goes on, as for a missing file.
        attach_parent_console();
        eprintln!("fasttail: standard input is not a pipe; nothing to read");
        fasttail::print_mode::console::release();
        cli.stdin = false;
    }
    cli
}

/// Hands off to `fasttail-tui.exe` in a new console and exits (design 3 of
/// openspec/changes/tui-interface). Standard input cannot follow it there, so `-` or a
/// pipe is refused with exit code 2. Returns the notice the window shows when the
/// terminal executable is missing or does not start: a double-clicked GUI process has
/// no console, so the window is the only place to say it.
#[cfg(windows)]
fn start_terminal_interface(cli: &CliArgs, lang: Language) -> Option<String> {
    use fasttail::{handoff, i18n::t};
    let piped = fasttail::stdin_source::classify() == fasttail::stdin_source::StdinKind::Piped;
    if cli.stdin || piped {
        attach_parent_console();
        eprintln!(
            "fasttail: standard input cannot be handed to the terminal interface; \
             run fasttail-tui.exe instead (command | fasttail-tui.exe -)"
        );
        fasttail::print_mode::console::release();
        std::process::exit(2);
    }
    let name = handoff::tui_exe_name();
    let exe = handoff::sibling_exe(&name).unwrap_or_else(|| name.clone().into());
    let failure = |what: &str, detail: String| {
        Some(format!(
            "{}\n{}{detail}\n\n{}",
            t(lang, what),
            exe.display(),
            t(lang, "handoff_gui_instead")
        ))
    };
    if !exe.is_file() {
        return failure("handoff_not_found", String::new());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = handoff::forwarded_args(&args, "--tui", Some(handoff::HANDOFF_FLAG));
    match handoff::start_in_new_console(&exe, &args) {
        Ok(()) => std::process::exit(0),
        Err(err) => failure("handoff_start_failed", format!("\n{err}")),
    }
}

/// On Linux and macOS one executable has both interfaces: the terminal one runs in this
/// process when there is a terminal to draw in and to read keys from
/// (`handoff::terminal_available`). Without one, `--tui` fails with exit code 1, and
/// `interface=tui` from `fasttail.ini` opens the window with a line on stderr.
#[cfg(not(windows))]
fn start_terminal_interface(cli: &CliArgs, _lang: Language) -> Option<String> {
    if !fasttail::handoff::terminal_available() {
        if cli.tui {
            eprintln!("fasttail: --tui needs a terminal");
            std::process::exit(1);
        }
        eprintln!("fasttail: interface=tui ignored: no terminal");
        return None;
    }
    #[cfg(feature = "tui")]
    std::process::exit(fasttail::tui::run(std::env::args().skip(1)));
    #[cfg(not(feature = "tui"))]
    {
        eprintln!("fasttail: the terminal interface is not in this build");
        std::process::exit(2);
    }
}

/// Builds the window, with the hand-off notice when the terminal could not be started.
fn app_creator(cli: &CliArgs, handoff_notice: &Option<String>) -> eframe::AppCreator<'static> {
    let cli = cli.clone();
    let notice = handoff_notice.clone();
    Box::new(move |cc| {
        let mut app = FastTailApp::new(cc, cli);
        app.handoff_notice = notice;
        Ok(Box::new(app))
    })
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
    let handoff_notice = if cli.interface(config.interface) == Interface::Tui {
        start_terminal_interface(&cli, config.language)
    } else {
        None
    };

    let choice = cli
        .renderer
        .unwrap_or_else(|| RendererChoice::from_env(config.renderer));

    let app_title = format!("FastTail v{} by Matteo Baccan", env!("CARGO_PKG_VERSION"));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app_title.clone())
        .with_min_inner_size([800.0, 500.0])
        .with_decorations(!config.borderless)
        .with_drag_and_drop(true)
        // The title-bar badge as the window and taskbar icon: raw 128 × 128 RGBA
        // (generated from `assets/icon.png`), so no image decoder is linked for it.
        .with_icon(egui::IconData {
            rgba: include_bytes!("../assets/icon-128.rgba").to_vec(),
            width: 128,
            height: 128,
        });

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

    let make_options = |renderer: eframe::Renderer, software: bool| {
        let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration {
            surface: eframe::egui_wgpu::SurfaceConfig {
                // On a software rasterizer (WARP) there is no real vsync to wait on, so we
                // present immediately and let egui's own frame pacing (max_fps_software)
                // throttle the work, which keeps latency low. Measured neutral on CPU: the
                // idle cost is dominated by DWM composing the software surface, not by the
                // present mode.
                present_mode: if software {
                    eframe::egui_wgpu::wgpu::PresentMode::Immediate
                } else {
                    eframe::egui_wgpu::wgpu::PresentMode::AutoVsync
                },
                desired_maximum_frame_latency: Some(2),
            },
            ..Default::default()
        };
        if software {
            // Replace the default adapter picker so the wgpu path is forced onto the CPU
            // rasterizer (WARP on Windows), regardless of the GPUs present.
            if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
                setup.native_adapter_selector = Some(Arc::new(software_adapter_selector));
            }
        }
        eframe::NativeOptions {
            viewport: viewport.clone(),
            renderer,
            // Drivers (including NVIDIA on Windows among them) busy-wait inside presentation
            // while waiting for the vertical blank, which costs a whole core whenever egui
            // repaints continuously (e.g. during mouse moves). egui only repaints on demand,
            // so disabling vsync on the OpenGL path trades tearing on a UI that hardly
            // animates for a drastically lower CPU cost; the frame cap (`max_fps`) bounds
            // the rate. wgpu keeps `AutoVsync` on a GPU (see `present_mode` above).
            glow_options: eframe::egui_glow::GlowConfiguration {
                vsync: false,
                ..Default::default()
            },
            wgpu_options,
            ..Default::default()
        }
    };

    // wgpu first, OpenGL on failure (or whichever backend was forced).
    match choice {
        RendererChoice::Wgpu => {
            renderer::mark_starting(RendererKind::Wgpu, false);
            eframe::run_native(
                &app_title,
                make_options(eframe::Renderer::Wgpu, false),
                app_creator(&cli, &handoff_notice),
            )
        }
        RendererChoice::Glow => {
            renderer::mark_starting(RendererKind::Glow, false);
            eframe::run_native(
                &app_title,
                make_options(eframe::Renderer::Glow, false),
                app_creator(&cli, &handoff_notice),
            )
        }
        RendererChoice::Software => {
            renderer::mark_starting(RendererKind::Wgpu, false);
            let first = eframe::run_native(
                &app_title,
                make_options(eframe::Renderer::Wgpu, true),
                app_creator(&cli, &handoff_notice),
            );
            match first {
                Err(err) if !renderer::app_created() => {
                    eprintln!("renderer: wgpu software backend failed to start: {err}");
                    eprintln!("renderer: falling back to OpenGL");
                    renderer::mark_starting(RendererKind::Glow, true);
                    let second = eframe::run_native(
                        &app_title,
                        make_options(eframe::Renderer::Glow, false),
                        app_creator(&cli, &handoff_notice),
                    );
                    match second {
                        Err(err2) if !renderer::app_created() => {
                            eprintln!("renderer: glow backend also failed to start: {err2}");
                            eprintln!("renderer: no usable renderer available");
                            Err(err2)
                        }
                        other => other,
                    }
                }
                other => other,
            }
        }
        RendererChoice::Auto => {
            renderer::mark_starting(RendererKind::Wgpu, false);
            let first = eframe::run_native(
                &app_title,
                make_options(eframe::Renderer::Wgpu, false),
                app_creator(&cli, &handoff_notice),
            );
            match first {
                Err(err) if !renderer::app_created() => {
                    eprintln!("renderer: wgpu backend failed to start: {err}");
                    eprintln!("renderer: falling back to OpenGL");
                    renderer::mark_starting(RendererKind::Glow, true);
                    let second = eframe::run_native(
                        &app_title,
                        make_options(eframe::Renderer::Glow, false),
                        app_creator(&cli, &handoff_notice),
                    );
                    match second {
                        Err(err2) if !renderer::app_created() => {
                            eprintln!("renderer: glow backend also failed to start: {err2}");
                            eprintln!("renderer: no usable renderer available");
                            Err(err2)
                        }
                        other => other,
                    }
                }
                other => other,
            }
        }
    }
}

/// Adapter picker that forces wgpu onto a CPU rasterizer, used when the user asks for
/// software rendering. Keeps the list of available adapters in the error message so a
/// failure is diagnosable.
fn software_adapter_selector(
    adapters: &[wgpu::Adapter],
    _surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    adapters
        .iter()
        .find(|adapter| adapter.get_info().device_type == wgpu::DeviceType::Cpu)
        .cloned()
        .ok_or_else(|| {
            let names: Vec<String> = adapters
                .iter()
                .map(|adapter| adapter.get_info().name.clone())
                .collect();
            format!(
                "no software (CPU) adapter available (found: {})",
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            )
        })
}
