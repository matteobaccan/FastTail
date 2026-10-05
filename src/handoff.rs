// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Hand-offs between the two interfaces (openspec/changes/tui-interface, design 3).
//!
//! On Windows the window and the terminal are two executables: `fasttail.exe` is a
//! GUI-subsystem process, so a shell does not wait for it and it has no console to draw
//! in. When it resolves to the terminal interface it starts `fasttail-tui.exe` from its
//! own directory in a new console, with its arguments minus `--tui` plus the hidden
//! `--handoff` (wait for a key after an error exit, so the console does not vanish), and
//! exits. The command line is passed on as typed, not re-built from `CliArgs`.
//!
//! The other way, `fasttail-tui --gui` starts `fasttail` from its own directory, detached,
//! and exits. It passes `--gui` on: without it, `interface=tui` in `fasttail.ini` would
//! send the window straight back to the terminal.

use std::path::{Path, PathBuf};

use crate::cli::VALUE_OPTIONS;

/// Hidden option of `fasttail-tui`: started by a hand-off, in a console of its own.
pub const HANDOFF_FLAG: &str = "--handoff";

/// The terminal executable's file name on this platform.
pub fn tui_exe_name() -> String {
    format!("fasttail-tui{}", std::env::consts::EXE_SUFFIX)
}

/// The graphical executable's file name on this platform.
pub fn gui_exe_name() -> String {
    format!("fasttail{}", std::env::consts::EXE_SUFFIX)
}

/// What a build without the graphical interface answers to `--gui`: the archive it came
/// from, and the one that has the window.
pub fn terminal_only_message() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    let platform = format!(
        "{}-{arch}-{}",
        std::env::consts::OS,
        env!("CARGO_PKG_VERSION")
    );
    format!(
        "the graphical interface is not in this build (fasttail-tui-{platform}); \
         use the fasttail-{platform} archive"
    )
}

/// Whether the terminal interface can run here (Linux and macOS, in process): standard
/// output is a terminal, `TERM` is not `dumb`, and keys can be read, from standard input
/// or, when that is a pipe (`cmd | fasttail --tui -`), from `/dev/tty`.
pub fn terminal_usable(
    stdout_is_terminal: bool,
    term: Option<&str>,
    stdin_is_terminal: bool,
    dev_tty_opens: bool,
) -> bool {
    stdout_is_terminal && term != Some("dumb") && (stdin_is_terminal || dev_tty_opens)
}

/// `terminal_usable` for this process. `/dev/tty` is tried only when standard input is
/// not a terminal.
#[cfg(unix)]
pub fn terminal_available() -> bool {
    use std::io::IsTerminal;
    let stdin_is_terminal = std::io::stdin().is_terminal();
    let dev_tty_opens = || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .is_ok()
    };
    terminal_usable(
        std::io::stdout().is_terminal(),
        std::env::var("TERM").ok().as_deref(),
        stdin_is_terminal,
        !stdin_is_terminal && dev_tty_opens(),
    )
}

/// `name` in the directory of the running executable (where the archives put both).
pub fn sibling_exe(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(name))
}

/// `args` (the command line without the program name) without the flag `drop`, plus
/// `add`. Option values and everything after `--` are passed on untouched, so a filter
/// that reads `--tui` stays a filter.
pub fn forwarded_args(args: &[String], drop: &str, add: Option<&str>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() + 1);
    if let Some(flag) = add {
        out.push(flag.to_string());
    }
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            out.push(arg.clone());
            out.extend(iter.cloned());
            break;
        }
        if arg == drop {
            continue;
        }
        out.push(arg.clone());
        if VALUE_OPTIONS.contains(&arg.as_str()) {
            if let Some(value) = iter.next() {
                out.push(value.clone());
            }
        }
    }
    out
}

/// `arg` quoted for a Windows command line, so the C runtime and `CommandLineToArgvW`
/// read it back unchanged: quotes around it when it is empty or holds a space, a tab or a
/// quote; inside, a quote is escaped with a backslash, and the backslashes before a quote
/// or before the closing quote are doubled.
pub fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// The full command line for `program` and `args`. The program name is read without
/// escapes, so it is only put in quotes (a path cannot hold a quote).
pub fn windows_command_line(program: &Path, args: &[String]) -> String {
    let mut line = format!("\"{}\"", program.display());
    for arg in args {
        line.push(' ');
        line.push_str(&quote_windows_arg(arg));
    }
    line
}

/// Why the other interface did not start.
#[derive(Debug)]
pub enum StartError {
    /// This build has no graphical interface (the Linux terminal-only archive).
    NotInBuild,
    /// No display to open a window on (Linux without `DISPLAY` or `WAYLAND_DISPLAY`).
    NoDisplay,
    /// The other executable is not next to this one: the path looked for.
    NotFound(PathBuf),
    /// It is there but did not start.
    Failed(PathBuf, std::io::Error),
}

impl std::fmt::Display for StartError {
    /// The terminal interface's wording (English, as its other messages); the window
    /// words the hand-off failures with its translations.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::NotInBuild => f.write_str(&terminal_only_message()),
            StartError::NoDisplay => f.write_str(
                "no display (DISPLAY and WAYLAND_DISPLAY are not set): \
                 the graphical interface opens at the next start from a desktop",
            ),
            StartError::NotFound(path) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let dir = path.parent().unwrap_or(Path::new("."));
                write!(f, "{name} was not found in {}", dir.display())
            }
            StartError::Failed(path, e) => write!(f, "cannot start {}: {e}", path.display()),
        }
    }
}

/// `name` next to this executable when it is there.
fn find_sibling(name: &str) -> Result<PathBuf, StartError> {
    let exe = sibling_exe(name).unwrap_or_else(|| PathBuf::from(name));
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(StartError::NotFound(exe))
    }
}

/// Whether a window can be opened from here: always on Windows and macOS, on other
/// systems when `DISPLAY` or `WAYLAND_DISPLAY` is set.
pub fn display_available() -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        return true;
    }
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

/// Settings "Switch now" in the terminal interface: starts `fasttail --gui` from this
/// executable's directory, detached. The caller saved the workspace first and quits
/// once it started.
pub fn start_gui() -> Result<(), StartError> {
    if !cfg!(feature = "gui") {
        return Err(StartError::NotInBuild);
    }
    if !display_available() {
        return Err(StartError::NoDisplay);
    }
    let exe = find_sibling(&gui_exe_name())?;
    start_detached(&exe, &["--gui".to_string()]).map_err(|e| StartError::Failed(exe, e))
}

/// Whether the other interface's executable is next to this one (Settings warns when it
/// is not, on Windows where they are two files).
pub fn sibling_present(name: &str) -> bool {
    find_sibling(name).is_ok()
}

/// The window's words for a terminal that did not start: what happened, the path, and
/// the OS error when there is one.
pub fn tui_start_failure(lang: crate::i18n::Language, err: &StartError) -> String {
    use crate::i18n::t;
    match err {
        StartError::NotFound(path) => {
            format!("{}\n{}", t(lang, "handoff_not_found"), path.display())
        }
        StartError::Failed(path, e) => format!(
            "{}\n{}\n{e}",
            t(lang, "handoff_start_failed"),
            path.display()
        ),
        other => other.to_string(),
    }
}

/// The Windows hand-off to the terminal: `fasttail-tui.exe` from this executable's
/// directory in a new console, with `args` (`--handoff` is added).
#[cfg(windows)]
pub fn start_tui_in_new_console(args: &[String]) -> Result<(), StartError> {
    let exe = find_sibling(&tui_exe_name())?;
    let args = forwarded_args(args, "--tui", Some(HANDOFF_FLAG));
    start_in_new_console(&exe, &args).map_err(|e| StartError::Failed(exe, e))
}

/// Starts `program` with `args`, detached from this terminal, and returns without waiting:
/// no standard handle is passed on, on Windows without a console (`DETACHED_PROCESS`, the
/// window needs none), elsewhere in a process group of its own, so the window outlives
/// the terminal's job control. Same working directory and environment.
pub fn start_detached(program: &Path, args: &[String]) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command.spawn().map(|_| ())
}

/// Starts `program` with `args` in a new console window (`CREATE_NEW_CONSOLE`), with the
/// current directory and environment, and returns without waiting. No standard handle is
/// passed on: the child uses its new console's, as when started from Explorer.
#[cfg(windows)]
pub fn start_in_new_console(program: &Path, args: &[String]) -> std::io::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct StartupInfoW {
        cb: u32,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        reserved2_size: u16,
        reserved2: *mut u8,
        std_input: *mut c_void,
        std_output: *mut c_void,
        std_error: *mut c_void,
    }
    #[repr(C)]
    struct ProcessInformation {
        process: *mut c_void,
        thread: *mut c_void,
        process_id: u32,
        thread_id: u32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateProcessW(
            application_name: *const u16,
            command_line: *mut u16,
            process_attributes: *const c_void,
            thread_attributes: *const c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *const c_void,
            current_directory: *const u16,
            startup_info: *const StartupInfoW,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let application = wide(program.as_os_str());
    let mut command_line = wide(windows_command_line(program, args).as_ref());
    // SAFETY: plain C structs; zeroed is their documented initial state.
    let mut startup: StartupInfoW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<StartupInfoW>() as u32;
    let mut info: ProcessInformation = unsafe { std::mem::zeroed() };
    // SAFETY: the strings are NUL-terminated and outlive the call; `command_line` is
    // writable as CreateProcessW requires; the handles returned are closed below.
    let ok = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut info,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: both handles were just returned by CreateProcessW and are owned here.
    unsafe {
        CloseHandle(info.thread);
        CloseHandle(info.process);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn forwarded_args_drop_the_flag_but_not_values_or_paths() {
        let args = strings(&[
            "--tui", "--filter", "--tui", "app.log", "--ascii", "--", "--tui",
        ]);
        assert_eq!(
            forwarded_args(&args, "--tui", Some(HANDOFF_FLAG)),
            strings(&[
                "--handoff",
                "--filter",
                "--tui",
                "app.log",
                "--ascii",
                "--",
                "--tui"
            ])
        );
        // `--opt=value` is one argument; a trailing value option keeps its place.
        let args = strings(&["--search=x", "--tui", "--theme"]);
        assert_eq!(
            forwarded_args(&args, "--tui", None),
            strings(&["--search=x", "--theme"])
        );
    }

    #[test]
    fn windows_quoting() {
        for (arg, quoted) in [
            ("app.log", "app.log"),
            ("", "\"\""),
            ("my app.log", "\"my app.log\""),
            (r"C:\logs\", r"C:\logs\"),
            (r"C:\my logs\", r#""C:\my logs\\""#),
            (r#"say "hi""#, r#""say \"hi\"""#),
            (r#"a\"b"#, r#""a\\\"b""#),
            ("tab\there", "\"tab\there\""),
        ] {
            assert_eq!(quote_windows_arg(arg), quoted, "{arg}");
        }
        assert_eq!(
            windows_command_line(
                Path::new(r"C:\Program Files\FastTail\fasttail-tui.exe"),
                &strings(&["--handoff", "my app.log"])
            ),
            r#""C:\Program Files\FastTail\fasttail-tui.exe" --handoff "my app.log""#
        );
    }

    /// The quoting read back by Windows itself, the way the child's runtime will.
    #[cfg(windows)]
    #[test]
    fn windows_quoting_round_trips_through_command_line_to_argv() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        #[link(name = "shell32")]
        extern "system" {
            fn CommandLineToArgvW(line: *const u16, count: *mut i32) -> *mut *mut u16;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn LocalFree(mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }
        let args = strings(&[
            "plain",
            "",
            "two words",
            r"C:\dir with space\",
            r#"quote " inside"#,
            r#"\\"\"#,
            r"trailing\\",
            "--filter=a b",
            "àè 日本",
        ]);
        let line = windows_command_line(Path::new(r"C:\x y\fasttail-tui.exe"), &args);
        let wide: Vec<u16> = std::ffi::OsStr::new(&line)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut count = 0;
        // SAFETY: NUL-terminated input; the returned block is freed with LocalFree.
        let parsed = unsafe {
            let argv = CommandLineToArgvW(wide.as_ptr(), &mut count);
            assert!(!argv.is_null());
            let out: Vec<String> = (0..count as usize)
                .map(|i| {
                    let p = *argv.add(i);
                    let len = (0..).take_while(|&j| *p.add(j) != 0).count();
                    std::ffi::OsString::from_wide(std::slice::from_raw_parts(p, len))
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            LocalFree(argv.cast());
            out
        };
        assert_eq!(parsed[0], r"C:\x y\fasttail-tui.exe");
        assert_eq!(&parsed[1..], &args[..]);
    }

    #[test]
    fn hand_off_messages_are_translated_everywhere() {
        use crate::i18n::{t, Language};
        for key in [
            "handoff_failed",
            "handoff_not_found",
            "handoff_start_failed",
            "handoff_gui_instead",
            "handoff_press_key",
        ] {
            let english = t(Language::En, key);
            assert_ne!(english, "Unknown", "{key}");
            for &lang in Language::ALL.iter().filter(|l| **l != Language::En) {
                assert_ne!(t(lang, key), english, "{key} in {lang:?}");
            }
        }
    }

    /// The Unix decision table of design 3: (stdout, TERM, stdin, /dev/tty) -> terminal.
    #[test]
    fn unix_terminal_decision_table() {
        let xterm = Some("xterm-256color");
        for (stdout, term, stdin, dev_tty, usable) in [
            (true, xterm, true, false, true),        // an interactive shell
            (true, xterm, false, true, true),        // `cmd | fasttail --tui -`
            (true, None, true, false, true),         // TERM unset is not `dumb`
            (false, xterm, true, true, false),       // `fasttail --tui > out.txt`
            (true, Some("dumb"), true, true, false), // an editor's shell buffer
            (true, xterm, false, false, false),      // no controlling terminal for keys
            (false, None, false, false, false),      // a desktop launcher
        ] {
            assert_eq!(
                terminal_usable(stdout, term, stdin, dev_tty),
                usable,
                "{stdout} {term:?} {stdin} {dev_tty}"
            );
        }
    }

    /// The window's Settings entry and switch dialog. "Terminal" is the same word in
    /// several languages, so that one is only checked to exist.
    #[test]
    fn interface_switch_texts_are_translated_everywhere() {
        use crate::i18n::{t, Language};
        for key in [
            "interface_label",
            "interface_gui",
            "interface_tui",
            "interface_tui_missing",
            "interface_switch_title",
            "interface_switch_body",
            "interface_switch_now",
            "interface_switch_later",
            "interface_next_terminal_start",
        ] {
            let english = t(Language::En, key);
            assert_ne!(english, "Unknown", "{key}");
            if key == "interface_tui" {
                continue;
            }
            for &lang in Language::ALL.iter().filter(|l| **l != Language::En) {
                assert_ne!(t(lang, key), english, "{key} in {lang:?}");
            }
        }
    }

    #[test]
    fn start_errors_read_as_the_spec_words_them() {
        let path = std::env::temp_dir().join(gui_exe_name());
        let missing = StartError::NotFound(path.clone());
        assert_eq!(
            missing.to_string(),
            format!(
                "{} was not found in {}",
                gui_exe_name(),
                path.parent().unwrap().display()
            )
        );
        assert!(StartError::NoDisplay.to_string().contains("DISPLAY"));
        assert!(StartError::NotInBuild
            .to_string()
            .contains("not in this build"));
    }

    #[test]
    fn the_terminal_only_answer_names_both_archives() {
        let m = terminal_only_message();
        let version = env!("CARGO_PKG_VERSION");
        assert!(m.contains("not in this build"), "{m}");
        assert!(
            m.contains("(fasttail-tui-") && m.contains("use the fasttail-"),
            "{m}"
        );
        assert!(m.contains(version) && !m.contains("aarch64"), "{m}");
    }

    #[test]
    fn the_terminal_executable_sits_next_to_this_one() {
        let path = sibling_exe(&tui_exe_name()).unwrap();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(path.parent(), exe.parent());
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("fasttail-tui"));
    }
}
