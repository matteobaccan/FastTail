// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn test_external_tools_ini_round_trip() {
    use fasttail::external_tools::ExternalTool;

    let mut config = FastTailConfig::default();
    let mut editor = ExternalTool::new("Editor", "code", "-g \"{file}:{lineno}\"");
    editor.shortcut = Some("Ctrl+Shift+E".to_string());
    let mut notify = ExternalTool::new("Notify", "notify-send", "\"{line}\"");
    notify.bound_rule = Some("FATAL".to_string());
    notify.use_shell = true;
    notify.match_pattern = Some("req=([0-9]+)".to_string());
    config.external_tools = vec![editor.clone(), notify.clone()];

    let ini = config.to_ini();
    let tool0 = ini.section(Some("tool.0")).expect("tool.0 section");
    assert_eq!(tool0.get("name"), Some("Editor"));
    assert_eq!(tool0.get("program"), Some("code"));
    assert_eq!(tool0.get("shortcut"), Some("Ctrl+Shift+E"));
    assert_eq!(tool0.get("shell"), Some("false"));
    let tool1 = ini.section(Some("tool.1")).expect("tool.1 section");
    assert_eq!(tool1.get("rule"), Some("FATAL"));
    assert_eq!(tool1.get("match"), Some("req=([0-9]+)"));

    let restored = FastTailConfig::from_ini(&ini);
    assert_eq!(restored.external_tools, vec![editor, notify]);

    // A section without name or program is skipped, the rest is kept.
    let mut broken = ini.clone();
    broken.with_section(Some("tool.0")).set("program", "");
    let restored = FastTailConfig::from_ini(&broken);
    assert_eq!(restored.external_tools.len(), 1);
    assert_eq!(restored.external_tools[0].name, "Notify");
}

#[test]
fn test_external_tool_expansion_one_argv_entry_per_argument() {
    use fasttail::external_tools::{build_command, expanded_args, ExternalTool, ToolContext};
    use std::path::Path;

    let tool = ExternalTool::new("Editor", "code", "-g \"{file}:{lineno}\" --line {line}");
    let ctx = ToolContext::for_row(Path::new("/var/log/app.log"), 120, "hello world", None);
    let args = expanded_args(&tool, &ctx);
    assert_eq!(
        args,
        vec![
            "-g".to_string(),
            format!("{}:120", Path::new("/var/log/app.log").display()),
            "--line".to_string(),
            "hello world".to_string(),
        ]
    );

    // The command carries the program and exactly those argv entries, no shell.
    let cmd = build_command(&tool, &ctx);
    assert_eq!(cmd.get_program(), "code");
    let argv: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(argv, args);

    // {dir} and {selection}: the selection defaults to the row itself.
    let tool = ExternalTool::new("T", "x", "{dir} {selection}");
    let args = expanded_args(&tool, &ctx);
    assert_eq!(args[0], Path::new("/var/log").display().to_string());
    assert_eq!(args[1], "hello world");
    let ctx_sel = ToolContext::for_row(Path::new("a.log"), 1, "row", Some("row\nnext"));
    assert_eq!(expanded_args(&tool, &ctx_sel)[1], "row\nnext");
}

#[test]
fn test_external_tool_hostile_line_is_a_single_literal_argument() {
    use fasttail::external_tools::{build_command, ExternalTool, ToolContext};
    use std::path::Path;

    let hostile = "x; rm -rf / && del *.* | shutdown";
    let tool = ExternalTool::new("Echo", "echo", "{line}");
    let ctx = ToolContext::for_row(Path::new("app.log"), 7, hostile, None);
    let cmd = build_command(&tool, &ctx);
    assert_eq!(
        cmd.get_program(),
        "echo",
        "no shell in front of the program"
    );
    let argv: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        argv,
        vec![hostile.to_string()],
        "the whole line is one argument"
    );

    // Shell mode is the explicit opt-in: the line is handed to cmd /c or sh -c.
    let mut shell_tool = tool.clone();
    shell_tool.use_shell = true;
    let cmd = build_command(&shell_tool, &ctx);
    let program = cmd.get_program().to_string_lossy().into_owned();
    assert!(program == "cmd" || program == "sh", "{program}");
}

#[test]
fn test_external_tool_match_placeholder() {
    use fasttail::external_tools::{expanded_args, match_value, ExternalTool, ToolContext};
    use std::path::Path;

    let mut tool = ExternalTool::new("Ticket", "open", "https://tracker/{match}");
    tool.match_pattern = Some(r"ticket=([A-Z]+-\d+)".to_string());
    let line = "2026-09-18 ERROR ticket=FT-42 payment failed";
    assert_eq!(match_value(&tool, line), "FT-42");
    let ctx = ToolContext::for_row(Path::new("app.log"), 1, line, None);
    assert_eq!(expanded_args(&tool, &ctx), vec!["https://tracker/FT-42"]);

    // No group: the whole match; no match or no pattern: empty.
    tool.match_pattern = Some(r"FT-\d+".to_string());
    assert_eq!(match_value(&tool, line), "FT-42");
    assert_eq!(match_value(&tool, "nothing here"), "");
    tool.match_pattern = None;
    assert_eq!(match_value(&tool, line), "");
    tool.match_pattern = Some("(".to_string());
    assert_eq!(
        match_value(&tool, line),
        "",
        "an invalid regex expands to nothing"
    );
}

#[test]
fn test_external_tool_runner_throttle_and_cap() {
    use fasttail::external_tools::{RunOutcome, ToolContext, ToolRunner};
    use std::path::Path;

    let ctx = ToolContext::for_row(Path::new("app.log"), 1, "FATAL boom", None);

    // Throttle: three matches within a second run the tool once, two are dropped.
    let mut runner = ToolRunner::with_limits(Duration::from_secs(1), 10);
    let tool = quick_exit_tool("Notify");
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Throttled);
    assert_eq!(runner.run_bound(&tool, &ctx), RunOutcome::Throttled);
    assert_eq!(runner.dropped_for("Notify"), 2);
    assert_eq!(runner.dropped_for("Other"), 0);
    // Another tool has its own throttle window.
    let other = quick_exit_tool("Other");
    assert_eq!(runner.run_bound(&other, &ctx), RunOutcome::Spawned);

    // Cap: with no throttle and a cap of 2, the third long-running child is refused.
    let mut runner = ToolRunner::with_limits(Duration::ZERO, 2);
    let slow = slow_tool("Slow");
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::Spawned);
    assert_eq!(runner.running(), 2);
    assert_eq!(runner.run_bound(&slow, &ctx), RunOutcome::CapReached);
    assert_eq!(runner.dropped_for("Slow"), 1);

    // A missing program is reported, not panicked on (fresh runner: the cap above is full).
    let mut runner = ToolRunner::default();
    let missing =
        fasttail::external_tools::ExternalTool::new("Missing", "no-such-program-fasttail", "");
    assert!(matches!(
        runner.run_bound(&missing, &ctx),
        RunOutcome::Failed(_)
    ));
    assert!(runner.run_manual(&missing, &ctx).is_err());
    assert!(runner.last_error.is_some());
    assert!(runner.run_manual(&quick_exit_tool("Ok"), &ctx).is_ok());
    assert!(runner.last_error.is_none());
}

#[test]
fn test_engine_queues_hits_of_tool_bound_rules_on_append() {
    let mut tmp = NamedTempFile::new().unwrap();
    writeln!(tmp, "INFO start").unwrap();
    tmp.flush().unwrap();

    let mut engine = TailEngine::open(tmp.path()).unwrap();
    engine.set_highlight_rules(vec![
        HighlightRule::new("FATAL", [255, 0, 0], [0, 0, 0], false),
        HighlightRule::new("WARN", [255, 255, 0], [0, 0, 0], false),
    ]);
    // Only FATAL has a tool bound to it.
    engine.tool_bound_rules = ["FATAL".to_string()].into_iter().collect();

    writeln!(tmp, "FATAL first").unwrap();
    writeln!(tmp, "WARN ignored").unwrap();
    writeln!(tmp, "fatal lower case matches too").unwrap();
    tmp.flush().unwrap();
    engine.poll_updates();
    assert_eq!(engine.total_lines(), 4);
    assert_eq!(
        engine.pending_tool_hits,
        vec![("FATAL".to_string(), 1), ("FATAL".to_string(), 3)]
    );

    // The app drains the queue; the row context carries the file and 1-based line.
    let hits = std::mem::take(&mut engine.pending_tool_hits);
    let ctx = fasttail::ui::dock::tool_context_for_row(&engine, hits[0].1).unwrap();
    assert_eq!(ctx.lineno, 2);
    assert_eq!(ctx.line, "FATAL first");
    assert_eq!(ctx.file, tmp.path().display().to_string());
    assert_eq!(ctx.selection, "FATAL first");

    // Without bound rules nothing is queued.
    engine.tool_bound_rules.clear();
    writeln!(tmp, "FATAL again").unwrap();
    tmp.flush().unwrap();
    engine.poll_updates();
    assert!(engine.pending_tool_hits.is_empty());
}

#[test]
fn test_engine_current_row_prefers_selection_then_search_hit_then_last_line() {
    let mut tmp = NamedTempFile::new().unwrap();
    for i in 0..5 {
        writeln!(tmp, "line {i} {}", if i == 2 { "needle" } else { "" }).unwrap();
    }
    tmp.flush().unwrap();
    let mut engine = TailEngine::open(tmp.path()).unwrap();
    assert_eq!(engine.current_row(), Some(4), "last line by default");

    engine.update_search("needle");
    assert_eq!(engine.current_row(), Some(2), "the current search hit");

    engine.select_row(1);
    assert_eq!(engine.current_row(), Some(1), "the clicked row wins");
    engine.clear_selection();
    assert_eq!(engine.current_row(), Some(2));
}
