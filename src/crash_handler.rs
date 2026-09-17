use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub const GIT_COMMIT_HASH: &str = env!("GIT_COMMIT_HASH");
pub const GIT_TAG: &str = env!("GIT_TAG");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let leap = is_leap_year(year);
        let year_days = if leap { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }
    let leap = is_leap_year(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;
    (year, month, day)
}

pub fn format_timestamp() -> String {
    let now = std::time::SystemTime::now();
    if let Ok(duration) = now.duration_since(std::time::UNIX_EPOCH) {
        let secs = duration.as_secs();
        let days = secs / 86400;
        let day_secs = secs % 86400;
        let hours = day_secs / 3600;
        let minutes = (day_secs % 3600) / 60;
        let seconds = day_secs % 60;
        let (year, month, day) = days_to_date(days);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            year, month, day, hours, minutes, seconds
        )
    } else {
        "Unknown timestamp".to_string()
    }
}

pub fn build_crash_report(payload: &str, location: Option<&str>, backtrace: &Backtrace) -> String {
    let now = format_timestamp();
    format!(
        "================================================================================\n\
         FASTTAIL CRASH REPORT\n\
         ================================================================================\n\
         Application Version : {}\n\
         Git Commit ID       : {}\n\
         Git Tag / Ref       : {}\n\
         Timestamp           : {}\n\
         OS / Architecture   : {} / {}\n\
         Panic Location      : {}\n\
         Error / Message     : {}\n\
         \n\
         CALLSTACK / BACKTRACE:\n\
         {}\n\
         ================================================================================\n\
         Please report this issue at: https://github.com/baccan/fasttail/issues\n\
         Attach this crash log file to help identify and resolve the problem.\n\
         ================================================================================\n",
        APP_VERSION,
        GIT_COMMIT_HASH,
        GIT_TAG,
        now,
        std::env::consts::OS,
        std::env::consts::ARCH,
        location.unwrap_or("unknown"),
        payload,
        backtrace
    )
}

pub fn write_crash_log(report: &str) -> Vec<PathBuf> {
    let mut written_paths = Vec::new();
    let mut candidate_paths = Vec::new();

    // 1. Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        candidate_paths.push(cwd.join("fasttail_crash.log"));
    }

    // 2. Executable directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("fasttail_crash.log");
            if !candidate_paths.contains(&p) {
                candidate_paths.push(p);
            }
        }
    }

    // Attempt to write to candidate paths
    for path in &candidate_paths {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
        {
            if file.write_all(report.as_bytes()).is_ok() {
                written_paths.push(path.clone());
            }
        }
    }

    // 3. Fallback to temp directory if all candidates failed
    if written_paths.is_empty() {
        let temp_path = std::env::temp_dir().join("fasttail_crash.log");
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)
        {
            if file.write_all(report.as_bytes()).is_ok() {
                written_paths.push(temp_path);
            }
        }
    }

    written_paths
}

pub fn install_crash_handler() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any> (unknown panic payload)".to_string()
        };

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));

        let backtrace = Backtrace::force_capture();
        let report = build_crash_report(&payload, location.as_deref(), &backtrace);

        // Print to standard error for CLI runs
        eprintln!("{}", report);

        // Write to fasttail_crash.log
        let written = write_crash_log(&report);
        let saved_info = if !written.is_empty() {
            written
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            "Unable to write to disk (read-only filesystem)".to_string()
        };

        // Display modal message box via RFD
        let desc = format!(
            "FastTail has encountered an unexpected fatal error and had to close.\n\n\
             Git Commit ID:\n{}\n\n\
             Error Message:\n{}\n\n\
             Location:\n{}\n\n\
             A detailed crash log with callstack has been saved to:\n{}\n\n\
             Please report this issue on GitHub:\nhttps://github.com/baccan/fasttail/issues",
            GIT_COMMIT_HASH,
            payload,
            location.as_deref().unwrap_or("unknown"),
            saved_info
        );

        rfd::MessageDialog::new()
            .set_title("FastTail - Unexpected Crash")
            .set_description(&desc)
            .set_level(rfd::MessageLevel::Error)
            .show();
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp();
        assert!(ts.contains("UTC"));
        assert_eq!(ts.len(), 23); // "YYYY-MM-DD HH:MM:SS UTC"
    }

    #[test]
    fn test_days_to_date() {
        // 1970-01-01
        assert_eq!(days_to_date(0), (1970, 1, 1));
        // 2026-09-16
        let (y, m, d) = days_to_date(20712);
        assert_eq!(y, 2026);
        assert_eq!(m, 9);
        assert_eq!(d, 16);
    }

    #[test]
    fn test_build_crash_report() {
        let bt = Backtrace::disabled();
        let report = build_crash_report("test panic message", Some("src/main.rs:10:5"), &bt);
        assert!(report.contains("FASTTAIL CRASH REPORT"));
        assert!(report.contains("Git Commit ID"));
        assert!(report.contains(GIT_COMMIT_HASH));
        assert!(report.contains("src/main.rs:10:5"));
        assert!(report.contains("test panic message"));
        assert!(report.contains("CALLSTACK / BACKTRACE:"));
    }

    #[test]
    fn test_write_crash_log() {
        let report = "FASTTAIL TEST CRASH REPORT";
        let paths = write_crash_log(report);
        assert!(!paths.is_empty());
        for p in paths {
            assert!(p.exists());
            let content = std::fs::read_to_string(&p).unwrap();
            assert_eq!(content, report);
            let _ = std::fs::remove_file(p);
        }
    }
}
