// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! The notification-area (tray) icon.
//!
//! Windows only for now: a `Shell_NotifyIconW` icon owned by a message-only window on a
//! thread of its own, with a native popup menu. Clicks and menu picks reach the app
//! through a channel (and a repaint request); the app hands the menu labels, the stream
//! names and the alert badge back. While the window is hidden in the tray, the thread
//! wakes the app every `WAKE_INTERVAL`: a hidden window gets no redraw on its own, and
//! the streams must keep being polled. Elsewhere `Tray::start` returns `None` and the
//! options say the tray is not available.

use std::sync::mpsc::Receiver;
use std::time::Duration;

/// How often a hidden window is woken to poll its streams.
pub const WAKE_INTERVAL: Duration = Duration::from_millis(250);

/// What the tray asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Left click: show the window when hidden, hide it otherwise.
    Toggle,
    /// Double click: show the window.
    Show,
    FollowAll,
    PauseAll,
    ToggleMute,
    /// Show the window with stream `n` (in the order of `TrayMenu::streams`) focused.
    Stream(usize),
    Quit,
}

/// The labels and state the menu is built from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayMenu {
    pub show: String,
    pub hide: String,
    pub follow_all: String,
    pub pause_all: String,
    pub mute: String,
    pub quit: String,
    pub visible: bool,
    pub muted: bool,
    pub streams: Vec<String>,
}

/// The app icon (RGBA, `size` × `size`) scaled to `out` × `out` by averaging.
pub fn scale_icon(rgba: &[u8], size: usize, out: usize) -> Vec<u8> {
    let mut result = vec![0u8; out * out * 4];
    let block = (size / out).max(1);
    for y in 0..out {
        for x in 0..out {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for dy in 0..block {
                for dx in 0..block {
                    let (sx, sy) = (x * block + dx, y * block + dy);
                    if sx < size && sy < size {
                        let i = (sy * size + sx) * 4;
                        for c in 0..4 {
                            acc[c] += u32::from(rgba[i + c]);
                        }
                        n += 1;
                    }
                }
            }
            let o = (y * out + x) * 4;
            for c in 0..4 {
                result[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    result
}

/// `icon` (RGBA, `size` × `size`) with a filled dot of `color` in its top-right corner,
/// ringed in black, as the alert badge.
pub fn with_badge(icon: &[u8], size: usize, color: [u8; 3]) -> Vec<u8> {
    let mut out = icon.to_vec();
    let r = size as f32 * 0.24;
    let (cx, cy) = (size as f32 - r - 0.5, r + 0.5);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let i = (y * size + x) * 4;
            if d <= r - 1.0 {
                out[i..i + 3].copy_from_slice(&color);
                out[i + 3] = 255;
            } else if d <= r {
                out[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    out
}

/// The tray icon, while it is shown. Dropping it removes the icon.
pub struct Tray {
    events: Receiver<TrayEvent>,
    #[cfg(windows)]
    inner: win::Handle,
}

impl Tray {
    /// Shows the icon (`icon`: RGBA, `size` × `size`) with `tooltip`, or `None` where no
    /// tray is available (every system but Windows in this version).
    #[allow(unused_variables)]
    pub fn start(
        ctx: egui::Context,
        icon: &[u8],
        size: usize,
        tooltip: &str,
        menu: TrayMenu,
    ) -> Option<Self> {
        #[cfg(windows)]
        {
            let (inner, events) = win::start(ctx, scale_icon(icon, size, 32), tooltip, menu)?;
            Some(Self { events, inner })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Whether this system has a tray FastTail can use.
    pub fn available() -> bool {
        cfg!(windows)
    }

    /// The events since the last call.
    pub fn events(&self) -> Vec<TrayEvent> {
        self.events.try_iter().collect()
    }

    /// Replaces the menu's labels and state.
    #[allow(unused_variables)]
    pub fn set_menu(&self, menu: TrayMenu) {
        #[cfg(windows)]
        self.inner.set_menu(menu);
    }

    /// Sets the badge (a dot of `color`, none with `None`) and the tooltip.
    #[allow(unused_variables)]
    pub fn set_badge(&self, color: Option<[u8; 3]>, tooltip: &str) {
        #[cfg(windows)]
        self.inner.set_badge(color, tooltip);
    }

    /// Whether the window is hidden in the tray: the thread wakes the app while it is.
    #[allow(unused_variables)]
    pub fn set_hidden(&self, hidden: bool) {
        #[cfg(windows)]
        self.inner.set_hidden(hidden);
    }
}

#[cfg(windows)]
mod win {
    use super::{with_badge, TrayEvent, TrayMenu, WAKE_INTERVAL};
    use std::ffi::c_void;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Mutex, OnceLock};

    type Hwnd = *mut c_void;
    type Handle_ = *mut c_void;

    const WM_APP: u32 = 0x8000;
    const WM_TRAY: u32 = WM_APP + 1;
    const WM_REFRESH: u32 = WM_APP + 2;
    const WM_CLOSE: u32 = 0x0010;
    const WM_DESTROY: u32 = 0x0002;
    const WM_TIMER: u32 = 0x0113;
    const WM_NULL: u32 = 0x0000;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_LBUTTONDBLCLK: u32 = 0x0203;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const NIM_ADD: u32 = 0;
    const NIM_MODIFY: u32 = 1;
    const NIM_DELETE: u32 = 2;
    const NIF_MESSAGE: u32 = 1;
    const NIF_ICON: u32 = 2;
    const NIF_TIP: u32 = 4;
    const MF_STRING: u32 = 0;
    const MF_SEPARATOR: u32 = 0x800;
    const MF_CHECKED: u32 = 0x8;
    const TPM_RETURNCMD: u32 = 0x100;
    const TPM_RIGHTBUTTON: u32 = 0x2;
    const HWND_MESSAGE: isize = -3;

    const CMD_TOGGLE: usize = 1;
    const CMD_FOLLOW: usize = 2;
    const CMD_PAUSE: usize = 3;
    const CMD_MUTE: usize = 4;
    const CMD_QUIT: usize = 9;
    const CMD_STREAM: usize = 100;

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        wnd_proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: Handle_,
        icon: Handle_,
        cursor: Handle_,
        background: Handle_,
        menu_name: *const u16,
        class_name: *const u16,
        icon_small: Handle_,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
        private: u32,
    }

    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: Hwnd,
        id: u32,
        flags: u32,
        callback_message: u32,
        icon: Handle_,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        version: u32,
        info_title: [u16; 64],
        info_flags: u32,
        guid: [u8; 16],
        balloon_icon: Handle_,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassExW(class: *const WndClassExW) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Hwnd,
            menu: Handle_,
            instance: Handle_,
            param: *mut c_void,
        ) -> Hwnd;
        fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn DestroyWindow(hwnd: Hwnd) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
        fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn PostQuitMessage(code: i32);
        fn SetTimer(hwnd: Hwnd, id: usize, ms: u32, func: *const c_void) -> usize;
        fn CreatePopupMenu() -> Handle_;
        fn AppendMenuW(menu: Handle_, flags: u32, id: usize, text: *const u16) -> i32;
        fn TrackPopupMenu(
            menu: Handle_,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: Hwnd,
            rect: *const c_void,
        ) -> i32;
        fn DestroyMenu(menu: Handle_) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn CreateIcon(
            instance: Handle_,
            width: i32,
            height: i32,
            planes: u8,
            bits_per_pixel: u8,
            and_bits: *const u8,
            xor_bits: *const u8,
        ) -> Handle_;
        fn DestroyIcon(icon: Handle_) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> Handle_;
    }

    #[link(name = "shell32")]
    extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *mut NotifyIconDataW) -> i32;
    }

    /// State shared between the app and the tray thread (one tray per process).
    struct Shared {
        tx: Sender<TrayEvent>,
        ctx: egui::Context,
        menu: TrayMenu,
        /// The icon (RGBA 32×32), its badged version to show next, and the tooltip.
        base: Vec<u8>,
        pending: Option<(Vec<u8>, String)>,
        hidden: bool,
        icon: usize,
    }

    static SHARED: OnceLock<Mutex<Option<Shared>>> = OnceLock::new();

    fn shared() -> &'static Mutex<Option<Shared>> {
        SHARED.get_or_init(|| Mutex::new(None))
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn tip(text: &str) -> [u16; 128] {
        let mut out = [0u16; 128];
        for (slot, c) in out.iter_mut().zip(text.encode_utf16().take(127)) {
            *slot = c;
        }
        out
    }

    /// An HICON from RGBA 32×32 (the colour bits as BGRA, top-down; an empty AND mask,
    /// the alpha channel does the masking).
    unsafe fn make_icon(rgba: &[u8]) -> Handle_ {
        let size = 32;
        let bgra: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let and = vec![0u8; size * size / 8];
        CreateIcon(
            GetModuleHandleW(std::ptr::null()),
            size as i32,
            size as i32,
            1,
            32,
            and.as_ptr(),
            bgra.as_ptr(),
        )
    }

    fn notify_data(hwnd: Hwnd, icon: Handle_, tooltip: &str) -> NotifyIconDataW {
        NotifyIconDataW {
            cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
            hwnd,
            id: 1,
            flags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            callback_message: WM_TRAY,
            icon,
            tip: tip(tooltip),
            state: 0,
            state_mask: 0,
            info: [0; 256],
            version: 0,
            info_title: [0; 64],
            info_flags: 0,
            guid: [0; 16],
            balloon_icon: std::ptr::null_mut(),
        }
    }

    fn send(event: TrayEvent) {
        if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = s.tx.send(event);
            s.ctx.request_repaint();
        }
    }

    unsafe fn show_menu(hwnd: Hwnd) {
        let Some(menu) = shared()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|s| s.menu.clone())
        else {
            return;
        };
        let popup = CreatePopupMenu();
        if popup.is_null() {
            return;
        }
        let add = |flags: u32, id: usize, text: &str| {
            let text = wide(text);
            AppendMenuW(popup, flags, id, text.as_ptr());
        };
        add(
            MF_STRING,
            CMD_TOGGLE,
            if menu.visible { &menu.hide } else { &menu.show },
        );
        add(MF_SEPARATOR, 0, "");
        add(MF_STRING, CMD_FOLLOW, &menu.follow_all);
        add(MF_STRING, CMD_PAUSE, &menu.pause_all);
        add(
            MF_STRING | if menu.muted { MF_CHECKED } else { 0 },
            CMD_MUTE,
            &menu.mute,
        );
        if !menu.streams.is_empty() {
            add(MF_SEPARATOR, 0, "");
            for (i, name) in menu.streams.iter().enumerate().take(30) {
                add(MF_STRING, CMD_STREAM + i, name);
            }
        }
        add(MF_SEPARATOR, 0, "");
        add(MF_STRING, CMD_QUIT, &menu.quit);
        let mut at = Point { x: 0, y: 0 };
        GetCursorPos(&mut at);
        // The documented way for a tray menu to close when clicked outside.
        SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(
            popup,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            0,
            hwnd,
            std::ptr::null(),
        ) as usize;
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(popup);
        let event = match picked {
            CMD_TOGGLE => Some(TrayEvent::Toggle),
            CMD_FOLLOW => Some(TrayEvent::FollowAll),
            CMD_PAUSE => Some(TrayEvent::PauseAll),
            CMD_MUTE => Some(TrayEvent::ToggleMute),
            CMD_QUIT => Some(TrayEvent::Quit),
            n if n >= CMD_STREAM => Some(TrayEvent::Stream(n - CMD_STREAM)),
            _ => None,
        };
        if let Some(event) = event {
            send(event);
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: Hwnd,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        match msg {
            WM_TRAY => {
                match lparam as u32 {
                    WM_LBUTTONUP => send(TrayEvent::Toggle),
                    WM_LBUTTONDBLCLK => send(TrayEvent::Show),
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                    _ => {}
                }
                0
            }
            WM_REFRESH => {
                let mut guard = shared().lock().unwrap_or_else(|e| e.into_inner());
                if let Some(s) = guard.as_mut() {
                    if let Some((rgba, tooltip)) = s.pending.take() {
                        let icon = make_icon(&rgba);
                        let mut data = notify_data(hwnd, icon, &tooltip);
                        Shell_NotifyIconW(NIM_MODIFY, &mut data);
                        if s.icon != 0 {
                            DestroyIcon(s.icon as Handle_);
                        }
                        s.icon = icon as usize;
                    }
                }
                0
            }
            WM_TIMER => {
                if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    if s.hidden {
                        s.ctx.request_repaint();
                    }
                }
                0
            }
            WM_CLOSE => {
                let mut data = notify_data(hwnd, std::ptr::null_mut(), "");
                Shell_NotifyIconW(NIM_DELETE, &mut data);
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    /// The app's side of the tray thread.
    pub struct Handle {
        hwnd: usize,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Handle {
        fn post(&self, msg: u32) {
            unsafe {
                PostMessageW(self.hwnd as Hwnd, msg, 0, 0);
            }
        }

        pub fn set_menu(&self, menu: TrayMenu) {
            if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                s.menu = menu;
            }
        }

        pub fn set_badge(&self, color: Option<[u8; 3]>, tooltip: &str) {
            if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                let rgba = match color {
                    Some(color) => with_badge(&s.base, 32, color),
                    None => s.base.clone(),
                };
                s.pending = Some((rgba, tooltip.to_string()));
            }
            self.post(WM_REFRESH);
        }

        pub fn set_hidden(&self, hidden: bool) {
            if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                s.hidden = hidden;
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            self.post(WM_CLOSE);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).take() {
                if s.icon != 0 {
                    unsafe {
                        DestroyIcon(s.icon as Handle_);
                    }
                }
            }
        }
    }

    pub fn start(
        ctx: egui::Context,
        base: Vec<u8>,
        tooltip: &str,
        menu: TrayMenu,
    ) -> Option<(Handle, Receiver<TrayEvent>)> {
        let (tx, rx) = channel();
        *shared().lock().unwrap_or_else(|e| e.into_inner()) = Some(Shared {
            tx,
            ctx,
            menu,
            base: base.clone(),
            pending: None,
            hidden: false,
            icon: 0,
        });
        let (ready_tx, ready_rx) = channel::<usize>();
        let tooltip = tooltip.to_string();
        let thread = std::thread::Builder::new()
            .name("fasttail-tray".into())
            .spawn(move || unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class = wide("FastTailTrayWindow");
                let wc = WndClassExW {
                    cb_size: std::mem::size_of::<WndClassExW>() as u32,
                    style: 0,
                    wnd_proc,
                    cls_extra: 0,
                    wnd_extra: 0,
                    instance,
                    icon: std::ptr::null_mut(),
                    cursor: std::ptr::null_mut(),
                    background: std::ptr::null_mut(),
                    menu_name: std::ptr::null(),
                    class_name: class.as_ptr(),
                    icon_small: std::ptr::null_mut(),
                };
                // Already registered by an earlier tray of this process: fine.
                RegisterClassExW(&wc);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE as Hwnd,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null_mut(),
                );
                if hwnd.is_null() {
                    let _ = ready_tx.send(0);
                    return;
                }
                let icon = make_icon(&base);
                let mut data = notify_data(hwnd, icon, &tooltip);
                if Shell_NotifyIconW(NIM_ADD, &mut data) == 0 {
                    DestroyWindow(hwnd);
                    let _ = ready_tx.send(0);
                    return;
                }
                if let Some(s) = shared().lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
                    s.icon = icon as usize;
                }
                SetTimer(hwnd, 1, WAKE_INTERVAL.as_millis() as u32, std::ptr::null());
                let _ = ready_tx.send(hwnd as usize);
                let mut msg: Msg = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok()?;
        let hwnd = ready_rx.recv().unwrap_or(0);
        if hwnd == 0 {
            let _ = thread.join();
            *shared().lock().unwrap_or_else(|e| e.into_inner()) = None;
            return None;
        }
        Some((
            Handle {
                hwnd,
                thread: Some(thread),
            },
            rx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_scales_down_by_averaging() {
        // 4×4 opaque red / transparent halves → 2×2.
        let mut icon = Vec::new();
        for _y in 0..4 {
            for x in 0..4 {
                icon.extend_from_slice(if x < 2 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        let small = scale_icon(&icon, 4, 2);
        assert_eq!(&small[0..4], &[255, 0, 0, 255]);
        assert_eq!(&small[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn the_badge_is_a_ringed_dot_in_the_top_right_corner() {
        let icon = vec![10u8; 32 * 32 * 4];
        let badged = with_badge(&icon, 32, [200, 30, 30]);
        let at = |x: usize, y: usize| &badged[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(at(24, 7), &[200, 30, 30, 255], "the dot");
        assert_eq!(at(2, 30), &[10, 10, 10, 10], "the rest untouched");
    }

    #[test]
    fn availability_follows_the_platform() {
        assert_eq!(Tray::available(), cfg!(windows));
    }
}
