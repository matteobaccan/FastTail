// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Ranges of the numeric settings, shared by `FastTailConfig::load` (the ini and the
//! `FASTTAIL_*` variables), the GUI Settings widgets and the terminal Settings dialog,
//! so every place accepts exactly the same values.

use std::ops::RangeInclusive;

/// `[general] poll_interval_ms`: how often the streams are polled.
pub const POLL_INTERVAL_MS: RangeInclusive<u32> = 50..=5000;
/// `[general] size_check_interval_ms`: how often a file's size is checked.
pub const SIZE_CHECK_INTERVAL_MS: RangeInclusive<u32> = 50..=10000;
/// `[general] max_fps`: frame cap of the GPU renderers.
pub const MAX_FPS: RangeInclusive<u32> = 5..=240;
/// `[general] max_fps_software`: frame cap of the software renderer.
pub const MAX_FPS_SOFTWARE: RangeInclusive<u32> = 5..=120;
/// `[general] mouse_throttle_ms`: minimum time between two pointer-move repaints.
pub const MOUSE_THROTTLE_MS: RangeInclusive<u64> = 0..=1000;
/// `[general] markdown_max_mb`: largest file the Markdown view renders.
pub const MARKDOWN_MAX_MB: RangeInclusive<u32> = 1..=100;
/// `[general] auto_bookmark_max`: automatic bookmarks kept per stream.
pub const AUTO_BOOKMARK_MAX: RangeInclusive<usize> =
    crate::tail_engine::MIN_AUTO_BOOKMARK_MAX..=crate::tail_engine::MAX_AUTO_BOOKMARK_MAX;
/// `[general] compressed_max_gb`: largest decompressed spool.
pub const COMPRESSED_MAX_GB: RangeInclusive<u32> =
    crate::compressed::MIN_MAX_GB..=crate::compressed::MAX_MAX_GB;
/// `[general] stdin_spool_max_mb`: size the standard-input spool restarts from empty at.
pub const STDIN_SPOOL_MAX_MB: RangeInclusive<u32> =
    crate::stdin_source::MIN_MAX_MB..=crate::stdin_source::MAX_MAX_MB;
/// `[general] screensaver_timeout_mins`: idle minutes before the screensaver (GUI) or the
/// idle lock (terminal); 0 turns it off.
pub const SCREENSAVER_TIMEOUT_MINS: RangeInclusive<u32> = 0..=120;
/// `[general] time_delta_gap_ms`: gap the time delta column marks, up to a day.
pub const TIME_DELTA_GAP_MS: RangeInclusive<u64> = 0..=86_400_000;

/// `value` brought into `range`.
pub fn clamp<T: Ord + Copy>(value: T, range: &RangeInclusive<T>) -> T {
    value.clamp(*range.start(), *range.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_values_in_range() {
        assert_eq!(clamp(10, &POLL_INTERVAL_MS), 50);
        assert_eq!(clamp(300, &POLL_INTERVAL_MS), 300);
        assert_eq!(clamp(9999, &POLL_INTERVAL_MS), 5000);
        assert_eq!(clamp(500, &SCREENSAVER_TIMEOUT_MINS), 120);
    }

    #[test]
    fn every_default_is_inside_its_range() {
        let cfg = crate::config::FastTailConfig::default();
        assert!(POLL_INTERVAL_MS.contains(&cfg.poll_interval_ms));
        assert!(SIZE_CHECK_INTERVAL_MS.contains(&cfg.size_check_interval_ms));
        assert!(MAX_FPS.contains(&cfg.max_fps));
        assert!(MAX_FPS_SOFTWARE.contains(&cfg.max_fps_software));
        assert!(MOUSE_THROTTLE_MS.contains(&cfg.mouse_throttle_ms));
        assert!(MARKDOWN_MAX_MB.contains(&cfg.markdown_max_mb));
        assert!(AUTO_BOOKMARK_MAX.contains(&cfg.auto_bookmark_max));
        assert!(COMPRESSED_MAX_GB.contains(&cfg.compressed_max_gb));
        assert!(STDIN_SPOOL_MAX_MB.contains(&cfg.stdin_spool_max_mb));
        assert!(SCREENSAVER_TIMEOUT_MINS.contains(&cfg.screensaver_timeout_mins));
        assert!(TIME_DELTA_GAP_MS.contains(&cfg.time_delta_gap_ms));
    }
}
