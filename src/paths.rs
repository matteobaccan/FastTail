//! Path comparison helpers shared by the dock and the app.

use std::path::Path;

/// Cheap equality: exact match, or (on Windows) ASCII case-insensitive string match.
/// No filesystem access, safe to call every frame.
pub fn paths_equal_fast(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        if a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()) {
            return true;
        }
    }
    false
}

/// Thorough equality: also canonicalizes both paths (touches the filesystem).
pub fn paths_equal(a: &Path, b: &Path) -> bool {
    if paths_equal_fast(a, b) {
        return true;
    }
    if let (Ok(ca), Ok(cb)) = (a.canonicalize(), b.canonicalize()) {
        if paths_equal_fast(&ca, &cb) {
            return true;
        }
    }
    false
}
