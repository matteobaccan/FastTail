## 1. Scan job

- [ ] 1.1 Add `ScanKind::Timestamps`, `JobSpec::Timestamps { inherited, hint }` and `ScanBatch::Timestamps { values, parsed, unordered, hint }` to `src/scan_job.rs`; evaluate lines with `detect_timestamp`, the format hint and inheritance exactly as `fill_timestamps`; one batch plus a progress message per 1 MB chunk
- [ ] 1.2 Unit tests in `scan_job`: formats, continuation-line inheritance, a start line after 0 with an inherited value, UTF-16 input, cancellation

## 2. Engine

- [ ] 2.1 Add `request_timestamps()` (synchronous below `job_threshold_bytes` of untimed bytes, job above) and replace the synchronous calls in `set_time_range`, `apply_time_range_text`, `resolve_goto` and the append path; keep `ensure_timestamps()` for tests and the bench
- [ ] 2.2 Drain and finish handling for `Timestamps` batches; `job.hits` = timed lines; `scan_progress()` reports the new kind
- [ ] 2.3 Priority rules: wait for the index, preempt `Levels`, queue behind Filter/Search, defer filter/search while a window is set or pending, otherwise be preempted and resume from the prefix; cancel on reload
- [ ] 2.4 Pending time window (`time_range_pending`) applied at completion by re-reading the field texts; clearing or editing the fields while pending
- [ ] 2.5 Pending go-to-time (`pending_goto_time`), resolved and scrolled at completion, dropped when the popup closes or a new target is entered
- [ ] 2.6 Apply `in_time_range` to Filter and Search job batches at drain time and drop the `!is_time_filtered()` exception in `recompute_filtered_lines_from`
- [ ] 2.7 Tests: sync and job paths give identical caches (values, parsed count, unordered flag, hint); a job preempted by a filter and resumed gives the same cache; pending window applies once complete and hides nothing before; go-to-time waits and lands on the same line as the sync path; filter and search jobs respect the window on a file above the threshold; truncation during the scan

## 3. UI

- [ ] 3.1 `scan_kind_key` entry for `Timestamps`; pending hint in `render_time_range`; progress line in the Ctrl+G popup while a jump waits
- [ ] 3.2 i18n keys `scan_timestamps` and `time_range_pending` in all 16 languages; `cargo test` i18n coverage passes

## 4. Bench and docs

- [ ] 4.1 `benches/filter_bench.rs`: add a phase timing the background path (request, poll until complete) next to the synchronous "timestamp cache" phase
- [ ] 4.2 README "Time range" section: the first use times the file in the background with progress in the stream bar
- [ ] 4.3 CHANGELOG 0.9.1: Fixed entry closing the 0.9.0 known issue (first use of the time controls no longer blocks the UI; window and time jump apply when timing finishes; filter/search jobs honour the window)
