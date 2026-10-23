// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

pub mod app;
pub mod calendar;
pub mod compare_tab;
pub mod debug_dialog;
pub mod dock;
pub mod export_dialog;
pub mod export_html;
pub mod find_results;
pub mod folder_dialog;
pub mod global_filter_bar;
pub mod hit_list;
pub mod json_view;
pub mod keys;
pub mod listen_dialog;
pub mod log_formats;
pub mod merge_dialog;
pub mod overview_strip;
pub mod palette;
pub mod patterns_tab;
pub mod query_tab;
pub mod remote_dialog;
pub mod report_dialog;
pub mod scratchpad;
pub mod split_dialog;
pub mod stats_tab;
pub mod system_dialog;
pub mod theme;
pub mod time_range;
pub mod timeline_strip;
pub mod zip_picker;

pub use app::FastTailApp;
