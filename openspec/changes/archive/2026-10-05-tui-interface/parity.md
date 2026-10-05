# Parity checklist for 0.20.0 (task 7.1)

Every item of the design's scope table marked 0.20.0, with the tests that hold it and
what was checked by hand. "By hand" means the maintainer's own use of the preview builds
of 0.15.0 to 0.16.2 on Windows (Windows Terminal and conhost), whose review led to the
fixes of 4.10 to 4.15.

| Item | Automated (`cargo test --no-default-features --features tui --lib`) | By hand |
|---|---|---|
| Windows, focused border, dialogs | `stream_window_has_borders_title_and_counts`, `every_dialog_draws_inside_the_screen`, `the_theme_paints_the_windows_and_dialogs_cast_a_shadow` | Windows Terminal, conhost |
| Mouse, `--no-mouse` | `clicks_select_rows_and_the_wheel_scrolls_the_window_under_the_pointer`, `dialog_buttons_and_clicks_outside`, `mouse::tests::*` | Windows Terminal |
| `--ascii`, legacy console | `ascii_mode_split_and_prompt_dialog`, `colors::tests::ascii_borders_on_a_legacy_console_or_on_request` | — |
| Dock, split, floating windows, resize, close | `keys_split_move_resize_and_close_windows`, `a_dragged_divider_resizes_and_a_dragged_title_moves_the_window`, `floating_windows_move_by_the_title_resize_by_the_corner_and_overlap`, `dock::tests::*`, `a_closing_window_shrinks_away` | Windows Terminal |
| Follow, search, filters, level, collapse | `the_cursor_pages_pauses_follow_and_g_follows_again`, `the_windows_have_the_guis_stream_bar_and_filter_row_which_click`, `a_count_before_e_jumps_to_the_nth_error_and_w_finds_warnings` | Windows Terminal |
| Compressed files, archive picker | `the_entry_picker_filters_by_typing_and_opens_the_entry`, `picker::tests::*` | — |
| Cursor row, selection, bookmarks, notes | `shift_arrows_select_from_the_cursor_which_is_drawn_reversed`, `bookmarks_notes_and_their_keys_follow_the_cursor` | Windows Terminal |
| Show in context, go to line or time | `ctrl_k_shows_the_cursor_row_in_context_and_esc_returns`, `go_to_moves_the_cursor_to_a_line_or_the_next_visible_one` | — |
| HEX view | `h_shows_the_bytes_and_keeps_the_place_both_ways`, `hex_search_walks_the_byte_hits_and_row_actions_say_why_not`, `the_hex_view_uses_the_width_it_has` | — |
| ANSI colours | `a_cycles_the_ansi_modes_and_no_escape_reaches_the_terminal` | — |
| Time range (fields, calendar) | `the_time_range_dialog_checks_both_sides_and_esc_keeps_the_range`, `the_time_range_calendar_picks_a_day_and_the_time_steps` | — |
| Settings | `the_settings_dialog_applies_saves_and_refuses_a_value_out_of_range`, `settings::tests::*` | Windows Terminal (live apply, language) |
| Rules, quick labels, presets, global filter | `the_rule_editor_reorders_adds_and_saves_for_every_stream`, `a_quick_label_paints_the_search_text_and_the_rule_editor_removes_it`, `a_preset_saved_in_the_terminal_is_applied_and_reaches_the_ini`, `the_global_filter_is_edited_applied_after_the_delay_and_switched_with_f` | — |
| External tools | `tools_run_from_the_menu_by_shortcut_and_for_their_rule_with_the_limits`, `the_tools_editor_opens_from_settings_and_saves_the_tools` | — |
| PIN lock and idle lock | `the_lock_hides_everything_drops_keys_counts_failures_and_restores_the_state`, `settings::tests::the_pin_is_set_twice_checked_scrambled_and_removed` | — |
| `fasttail.ini` and sessions shared with the window | `an_idle_terminal_never_rewrites_what_a_gui_saved_meanwhile`, `a_terminal_save_carries_the_gui_only_keys_through_byte_for_byte`, `the_layout_is_saved_for_the_gui_and_comes_back_with_sessions`, `workspace::tests::*`, `o_loads_a_session_from_a_path_or_the_recent_list` | Windows: a setting, a rule and a session changed in one interface found in the other |
| Hand-offs both ways | `picking_the_graphical_interface_offers_to_switch_and_reports_a_failure`, `tui::tests::the_hand_off_flag_is_hidden_and_accepted`, integration `workspace` and `handoff` tests | Windows: `fasttail.exe --tui`, `interface=tui` by double click, `fasttail-tui --gui` (0.16.2 preview) |
| i18n in 16 languages | `every_terminal_text_is_translated`, `the_terminal_speaks_the_language_of_the_ini_and_switches_at_once`, `a_cjk_language_fits_the_cells` | Windows Terminal (Italian) |
| Frame time | `frame_time_at_200_by_60` (ignored by default; release run below) | — |

Release run of `frame_time_at_200_by_60` on Windows 11 (2026-10-05): passed (p95 within 5 ms at 200 x 60 on 200,000 lines).

Not checked by hand for 0.20.0: Linux in a terminal and over SSH (the CI builds and tests
the Linux terminal-only binary, and its `cargo tree` has no GUI crate), and a double
click with `interface=tui` on Linux and macOS (no console host there: it opens the window
and says so, covered by the start tests).
