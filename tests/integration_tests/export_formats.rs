// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

use super::{wait_for_jobs, write_lines};
use fasttail::config::FastTailConfig;
use fasttail::export::{CsvOptions, CsvWriter, ExportJob, Scope, TextWriter};
use fasttail::tail_engine::TailEngine;

const LOG: [&str; 4] = [
    "2026-09-28 14:02:11.123 ERROR payment, \"declined\"",
    "    at Card.check",
    "2026-09-28 14:02:12 INFO =SUM(A1) <script>",
    "-5 plain line",
];

fn open(path: &std::path::Path) -> TailEngine {
    let mut e = TailEngine::open(path).unwrap();
    wait_for_jobs(&mut e);
    e
}

fn export(
    e: &TailEngine,
    scope: Scope,
    writer: Box<dyn fasttail::export::RowWriter>,
    max: Option<usize>,
) -> (String, ExportJob) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out");
    let job = ExportJob::start(e, scope, path.clone(), writer, max)
        .unwrap()
        .run(e);
    (std::fs::read_to_string(&path).unwrap(), job)
}

#[test]
fn csv_has_line_time_level_text_quoted_and_protected() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let e = open(&log);
    let (csv, job) = export(
        &e,
        Scope::Visible,
        Box::new(CsvWriter::new(CsvOptions::default())),
        None,
    );
    assert_eq!(job.lines, 4);
    let expected = "\u{feff}line,time,level,text\r\n\
        1,2026-09-28T14:02:11.123,ERROR,\"2026-09-28 14:02:11.123 ERROR payment, \"\"declined\"\"\"\r\n\
        2,,,    at Card.check\r\n\
        3,2026-09-28T14:02:12,INFO,2026-09-28 14:02:12 INFO =SUM(A1) <script>\r\n\
        4,,,'-5 plain line\r\n";
    assert_eq!(csv, expected);
    // No BOM, no protection, a semicolon.
    let options = CsvOptions {
        sep: b';',
        bom: false,
        protect: false,
    };
    let (csv, _) = export(&e, Scope::Visible, Box::new(CsvWriter::new(options)), None);
    assert!(csv.starts_with("line;time;level;text\r\n"));
    assert!(csv.ends_with("4;;;-5 plain line\r\n"));
    // The text export stays as it was.
    let (text, _) = export(&e, Scope::Visible, Box::new(TextWriter), None);
    assert_eq!(text, LOG.join("\n") + "\n");
}

#[test]
fn csv_columns_are_the_fields_on_a_stream_with_a_parser() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.json");
    write_lines(
        &log,
        &[
            r#"{"ts":"2026-09-28T14:02:11Z","level":"error","msg":"boom","user":"ann"}"#,
            "not json",
        ],
    );
    let mut e = open(&log);
    e.set_field_choice(fasttail::fields::ParserChoice::Json);
    e.set_field_columns(vec!["level".into(), "user".into()]);
    let options = CsvOptions {
        bom: false,
        ..CsvOptions::default()
    };
    let (csv, _) = export(&e, Scope::Visible, Box::new(CsvWriter::new(options)), None);
    assert_eq!(
        csv,
        "line,level,user,message\r\n1,error,ann,boom ts=2026-09-28T14:02:11Z\r\n2,,,not json\r\n"
    );
}

#[test]
fn scopes_and_the_html_page() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let mut e = open(&log);
    e.select_row(2);
    let (text, _) = export(&e, Scope::Selected, Box::new(TextWriter), None);
    assert_eq!(text, format!("{}\n", LOG[2]));
    e.update_search("payment");
    wait_for_jobs(&mut e);
    let (text, _) = export(&e, Scope::Matches, Box::new(TextWriter), None);
    assert_eq!(text, format!("{}\n", LOG[0]));

    let html = fasttail::ui::export_html::HtmlWriter::new(
        &e,
        fasttail::theme::CyberTheme::Tron,
        true,
        "app.log <x>",
    );
    let (page, job) = export(&e, Scope::Visible, Box::new(html), Some(3));
    assert!(job.capped, "3 of 4 lines");
    assert_eq!(job.lines, 3);
    assert!(page.starts_with("<!DOCTYPE html>"));
    assert!(page.contains("<title>app.log &lt;x&gt;</title>"));
    assert!(page.contains("&lt;script&gt;"), "escaped");
    assert!(!page.contains("<script>"));
    assert!(page.contains("<span class=\"n\">3</span>"));
    assert!(!page.contains("<span class=\"n\">4</span>"));
    // The search hit and the ERROR level have their classes, defined at the end.
    assert!(page.contains("payment</span>"));
    assert!(
        page.trim_end().ends_with("</style>\n</body></html>")
            || page.trim_end().ends_with("</body></html>")
    );
    assert!(page.contains(".c0{"));
}

#[test]
fn the_menu_opens_the_dialog_which_exports_and_remembers_the_format() {
    use fasttail::ui::FastTailApp;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("app.log");
    write_lines(&log, &LOG);
    let mut app = FastTailApp::from_config(FastTailConfig {
        spool_dir: Some(dir.path().join("spool")),
        ..Default::default()
    });
    app.open_log_file(log);
    app.engines[0].export_request = Some(Scope::Visible);
    app.apply_merged_views();
    let theme = app.config.theme;
    let dialog = app.export_dialog.as_mut().expect("the dialog");
    assert_eq!(dialog.format, fasttail::export::Format::Text);
    dialog.format = fasttail::export::Format::Csv;
    dialog.remember(&mut app.config);
    let out = dir.path().join("out.csv");
    dialog
        .start(&app.engines[0], out.clone(), theme, true)
        .unwrap();
    let ctx = egui::Context::default();
    for _ in 0..20 {
        let mut o = ctx.run_ui(Default::default(), |ui| app.render_ui(ui));
        o.textures_delta.clear();
        if app.export_dialog.as_ref().is_some_and(|d| d.job.is_none()) {
            break;
        }
    }
    let notice = app
        .export_dialog
        .as_ref()
        .unwrap()
        .notice
        .clone()
        .unwrap()
        .0;
    assert!(notice.starts_with("4 lines written"), "{notice}");
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains("line,time,level,text"));
    let back = FastTailConfig::from_ini(&app.config.to_ini());
    assert_eq!(back.export_format, "csv");
    assert!(back.export_csv_bom && back.export_csv_protect);
    assert_eq!(back.export_csv_separator, "comma");
}
