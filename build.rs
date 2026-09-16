use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");

    // Build timestamp
    let timestamp = {
        #[cfg(windows)]
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-Date -Format 'yyyy-MM-dd HH:mm:ss UTC'"])
            .output();

        #[cfg(not(windows))]
        let output = Command::new("date")
            .args(["-u", "+%Y-%m-%d %H:%M:%S UTC"])
            .output();

        output
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "2026-09-16 12:44:00 UTC".to_string())
    };
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);

    // Full Git commit hash
    let git_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_commit);

    // Git tag / commit hash
    let git_tag = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "main".to_string())
        });
    println!("cargo:rustc-env=GIT_TAG={}", git_tag);
}
