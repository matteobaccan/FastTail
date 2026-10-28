// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Named log formats: one `*.fasttail-format.ini` file per format in the `formats` folder
//! next to `fasttail.ini`, easy to share and commit. A format gives the field parser of a
//! stream its line patterns (regexes with named groups, tried in order), which field holds
//! the timestamp (and how to read it when the built-in parser cannot), which the level
//! (with a map of the log's own level words), the message, the file names it is for, and
//! sample lines.
//!
//! Formats are read once at start (`load_default`) into a process-wide library that the
//! parser choice `format:<name>` and the detection read; the Apache / nginx and syslog
//! presets are built-in formats.

use crate::fields::RegexFields;
use crate::log_level::LogLevel;
use crate::timestamp::TimeFormat;
use ini::{EscapePolicy, Ini, ParseOption, WriteOption};
use regex::{Regex, RegexSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// The folder of the format files, next to `fasttail.ini`.
pub const FORMATS_DIR: &str = "formats";
/// The suffix of a format file.
pub const FORMAT_SUFFIX: &str = ".fasttail-format.ini";
/// The section a format file holds.
pub const FORMAT_SECTION: &str = "format";
/// Most formats loaded from the folder.
pub const MAX_FORMATS: usize = 256;
/// Most line patterns of one format (`regex`, `regex.2` ... `regex.8`).
pub const MAX_PATTERNS: usize = 8;
/// Most sample lines of one format (`sample.1` ... `sample.20`).
pub const MAX_SAMPLES: usize = 20;
/// Largest format file read.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// The name `parser=` saves for a format: `format:<name>`.
pub const CHOICE_PREFIX: &str = "format:";

/// A format as written in its file: every value as text, so the editor can hold one that
/// does not compile yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormatSpec {
    pub name: String,
    /// One to `MAX_PATTERNS` regexes with named groups.
    pub patterns: Vec<String>,
    pub timestamp_field: String,
    /// A strftime subset or `epoch_s` / `epoch_ms` (see `timestamp::TimeFormat`); empty:
    /// the built-in timestamp detection reads the field.
    pub timestamp_format: String,
    /// `local`, `utc` or `+HH:MM`: the clock the log writes (empty: local). It becomes the
    /// source zone of a stream read with the format, unless the stream has one set.
    pub timestamp_zone: String,
    pub level_field: String,
    /// `token=level` pairs separated by commas (`W=warn,E=error`), any case.
    pub level_map: String,
    pub message_field: String,
    /// A line matching it starts an entry; any other line continues the previous one.
    pub entry_start: String,
    /// File name globs (`payments-*.log`) the format is for.
    pub files: Vec<String>,
    /// The column view's default columns.
    pub columns: Vec<String>,
    pub samples: Vec<String>,
}

impl FormatSpec {
    /// Reads the text of a format file. Values are taken as written (a backslash is a
    /// backslash), quotes around a value are removed.
    pub fn from_ini(text: &str) -> Result<Self, String> {
        let opt = ParseOption {
            enabled_quote: true,
            enabled_escape: false,
            ..ParseOption::default()
        };
        let conf = Ini::load_from_str_opt(text, opt).map_err(|e| e.to_string())?;
        let sec = conf
            .section(Some(FORMAT_SECTION))
            .ok_or_else(|| format!("no [{FORMAT_SECTION}] section"))?;
        let get = |key: &str| sec.get(key).unwrap_or("").trim().to_string();
        let raw = |key: &str| sec.get(key).map(str::to_string);
        let list = |key: &str| -> Vec<String> {
            get(key)
                .split([';', ','])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        };
        let patterns = std::iter::once("regex".to_string())
            .chain((2..=MAX_PATTERNS).map(|n| format!("regex.{n}")))
            .filter_map(|key| raw(&key))
            .filter(|p| !p.trim().is_empty())
            .collect();
        let samples = (1..=MAX_SAMPLES)
            .filter_map(|n| raw(&format!("sample.{n}")))
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            name: get("name"),
            patterns,
            timestamp_field: get("timestamp_field"),
            timestamp_format: get("timestamp_format"),
            timestamp_zone: get("timestamp_zone"),
            level_field: get("level_field"),
            level_map: get("level_map"),
            message_field: get("message_field"),
            entry_start: raw("entry_start").unwrap_or_default(),
            files: get("files")
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            columns: list("columns"),
            samples,
        })
    }

    /// The text of the format's file, read back the same by `from_ini`.
    pub fn to_ini(&self) -> String {
        let mut conf = Ini::new();
        let mut sec = conf.with_section(Some(FORMAT_SECTION));
        let value = crate::filter_preset::ini_value;
        sec.set("name", value(&self.name));
        for (i, p) in self.patterns.iter().take(MAX_PATTERNS).enumerate() {
            let key = if i == 0 {
                "regex".to_string()
            } else {
                format!("regex.{}", i + 1)
            };
            sec.set(key, value(p));
        }
        for (key, v) in [
            ("timestamp_field", &self.timestamp_field),
            ("timestamp_format", &self.timestamp_format),
            ("timestamp_zone", &self.timestamp_zone),
            ("level_field", &self.level_field),
            ("level_map", &self.level_map),
            ("message_field", &self.message_field),
            ("entry_start", &self.entry_start),
        ] {
            if !v.is_empty() {
                sec.set(key, value(v));
            }
        }
        if !self.files.is_empty() {
            sec.set("files", value(&self.files.join(";")));
        }
        if !self.columns.is_empty() {
            sec.set("columns", value(&self.columns.join(",")));
        }
        for (i, s) in self.samples.iter().take(MAX_SAMPLES).enumerate() {
            sec.set(format!("sample.{}", i + 1), value(s));
        }
        let mut buf = Vec::new();
        let opt = WriteOption {
            escape_policy: EscapePolicy::Nothing,
            ..WriteOption::default()
        };
        let _ = conf.write_to_opt(&mut buf, opt);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// The file name of the format in the formats folder: its name with the characters a
    /// file name cannot hold replaced.
    pub fn file_name(&self) -> String {
        let stem: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let stem = stem.trim_matches('.');
        format!(
            "{}{FORMAT_SUFFIX}",
            if stem.is_empty() { "format" } else { stem }
        )
    }
}

/// A compiled, validated format.
#[derive(Debug)]
pub struct LogFormat {
    pub spec: FormatSpec,
    pub patterns: Vec<RegexFields>,
    /// The patterns as one set, for detection.
    set: RegexSet,
    pub timestamp_format: Option<TimeFormat>,
    level_map: Vec<(String, LogLevel)>,
    pub entry_start: Option<Regex>,
    /// Shipped with FastTail: read-only, duplicated to be changed.
    pub builtin: bool,
    /// The file it was read from.
    pub file: Option<PathBuf>,
    /// The plugin that ships it (read-only, named `<plugin>/<format>`, detected like
    /// the user's formats).
    pub plugin: Option<String>,
}

impl LogFormat {
    /// Compiles `spec`; `Err` says what is wrong (a regex error with its position, a field
    /// role naming a group a pattern lacks, a timestamp format no sample reads...).
    pub fn compile(spec: FormatSpec) -> Result<Self, String> {
        if spec.name.trim().is_empty() {
            return Err("no name".to_string());
        }
        if spec.patterns.is_empty() {
            return Err("no line pattern (regex=)".to_string());
        }
        let mut patterns = Vec::new();
        for (i, p) in spec.patterns.iter().enumerate() {
            let compiled = RegexFields::new(p).map_err(|e| format!("pattern {}: {e}", i + 1))?;
            patterns.push(compiled);
        }
        for (role, field) in [
            ("timestamp_field", &spec.timestamp_field),
            ("level_field", &spec.level_field),
            ("message_field", &spec.message_field),
        ] {
            if field.is_empty() {
                continue;
            }
            if let Some(i) = patterns
                .iter()
                .position(|p| !p.names().any(|n| n == field.as_str()))
            {
                return Err(format!("{role}: pattern {} has no group {field}", i + 1));
            }
        }
        let set = RegexSet::new(&spec.patterns).map_err(|e| e.to_string())?;
        let timestamp_format = match spec.timestamp_format.trim() {
            "" => None,
            text => Some(TimeFormat::parse(text).map_err(|e| format!("timestamp_format: {e}"))?),
        };
        let level_map = parse_level_map(&spec.level_map)?;
        let entry_start = match spec.entry_start.trim() {
            "" => None,
            p => Some(Regex::new(p).map_err(|e| format!("entry_start: {e}"))?),
        };
        if !valid_zone(&spec.timestamp_zone) {
            return Err(format!(
                "timestamp_zone: {} (local, utc or +HH:MM)",
                spec.timestamp_zone
            ));
        }
        let format = Self {
            spec,
            patterns,
            set,
            timestamp_format,
            level_map,
            entry_start,
            builtin: false,
            file: None,
            plugin: None,
        };
        // A timestamp format no sample reads is a mistake the samples can show.
        if format.timestamp_format.is_some() && !format.spec.timestamp_field.is_empty() {
            let samples = &format.spec.samples;
            let read = samples.iter().any(|s| {
                crate::fields::with_spans(|spans| {
                    format.scan(s, spans) && format.timestamp(spans, s).is_some()
                })
            });
            if !samples.is_empty() && !read {
                return Err(format!(
                    "timestamp_format: no sample reads with {}",
                    format.spec.timestamp_format
                ));
            }
        }
        Ok(format)
    }

    pub fn name(&self) -> &str {
        &self.spec.name
    }

    /// Whether some pattern matches `line`.
    pub fn is_match(&self, line: &str) -> bool {
        self.set.is_match(line)
    }

    /// The named groups of every pattern, each once, in pattern order.
    pub fn group_count(&self) -> usize {
        let mut names: Vec<&str> = self.patterns.iter().flat_map(|p| p.names()).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }

    /// Scans `line` with the first pattern that matches into `out`.
    pub fn scan(&self, line: &str, out: &mut crate::fields::FieldSpans) -> bool {
        out.clear();
        self.patterns
            .iter()
            .any(|p| crate::fields::scan_regex(p, line, out))
    }

    /// The level of a line scanned into `spans`: its level field through the level map,
    /// or as a level word; `None` without a level field or value.
    pub fn level(&self, spans: &crate::fields::FieldSpans, line: &str) -> Option<LogLevel> {
        if self.spec.level_field.is_empty() {
            return None;
        }
        let field = spans.get(line, &self.spec.level_field)?;
        let value = field.value();
        let value = value.trim();
        self.level_map
            .iter()
            .find(|(token, _)| token.eq_ignore_ascii_case(value))
            .map(|(_, level)| *level)
            .or_else(|| crate::log_level::level_word(value))
    }

    /// The timestamp of a line scanned into `spans`, from its timestamp field: with the
    /// format's timestamp format, or the built-in detection.
    pub fn timestamp(&self, spans: &crate::fields::FieldSpans, line: &str) -> Option<i64> {
        if self.spec.timestamp_field.is_empty() {
            return None;
        }
        let field = spans.get(line, &self.spec.timestamp_field)?;
        let value = field.value();
        let value = value.trim();
        match &self.timestamp_format {
            Some(format) => crate::timestamp::parse_with_format(value, format),
            None => crate::timestamp::detect_timestamp(value, Default::default()).map(|(t, _)| t),
        }
    }

    /// Whether the format names `file` in its globs.
    pub fn covers_file(&self, file: &str) -> bool {
        self.spec
            .files
            .iter()
            .any(|g| crate::wildcard::wildcard_match(g, file))
    }

    /// Whether `line` starts an entry: it matches the entry start pattern, or without one
    /// it is no stack-trace continuation.
    pub fn starts_entry(&self, line: &str) -> bool {
        match &self.entry_start {
            Some(start) => start.is_match(line),
            None => !crate::tail_engine::TailEngine::is_stacktrace_continuation(line),
        }
    }
}

/// `W=warn,E=error`: tokens and the levels they name.
fn parse_level_map(text: &str) -> Result<Vec<(String, LogLevel)>, String> {
    let mut out = Vec::new();
    for pair in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (token, level) = pair
            .split_once('=')
            .ok_or_else(|| format!("level_map: {pair} is not token=level"))?;
        let level = crate::log_level::level_word(level)
            .ok_or_else(|| format!("level_map: {level} is not a level"))?;
        out.push((token.trim().to_string(), level));
    }
    Ok(out)
}

/// `local`, `utc` or `+HH:MM` / `-HH:MM`, as the stream's source zone reads it (empty is
/// local).
fn valid_zone(zone: &str) -> bool {
    zone.trim().is_empty() || crate::timestamp::SourceZone::from_config(zone).is_some()
}

/// The built-in Apache / nginx combined format.
pub const APACHE_NAME: &str = "Apache / nginx";
/// The built-in RFC 3164 syslog format.
pub const SYSLOG_NAME: &str = "Syslog";

/// The formats shipped with FastTail.
pub fn builtins() -> Vec<Arc<LogFormat>> {
    let apache = FormatSpec {
        name: APACHE_NAME.to_string(),
        patterns: vec![crate::fields::APACHE_PATTERN.to_string()],
        timestamp_field: "time".to_string(),
        timestamp_format: "%d/%b/%Y:%H:%M:%S %z".to_string(),
        files: vec!["access*.log".to_string(), "*access_log*".to_string()],
        samples: vec![
            r#"127.0.0.1 - frank [10/Oct/2026:13:55:36 -0700] "GET /a.gif HTTP/1.0" 200 2326 "-" "curl/8""#
                .to_string(),
        ],
        ..FormatSpec::default()
    };
    let syslog = FormatSpec {
        name: SYSLOG_NAME.to_string(),
        patterns: vec![crate::fields::SYSLOG_PATTERN.to_string()],
        timestamp_field: "time".to_string(),
        timestamp_format: "%b %e %H:%M:%S".to_string(),
        message_field: "msg".to_string(),
        samples: vec!["Sep 18 14:02:05 host sshd[812]: Accepted publickey".to_string()],
        ..FormatSpec::default()
    };
    [apache, syslog]
        .into_iter()
        .filter_map(|spec| LogFormat::compile(spec).ok())
        .map(|mut f| {
            f.builtin = true;
            Arc::new(f)
        })
        .collect()
}

/// The formats known to the process: the built-in ones, then the folder's.
#[derive(Debug, Default)]
pub struct Library {
    pub formats: Vec<Arc<LogFormat>>,
    /// The format files that were not loaded, and why.
    pub errors: Vec<(PathBuf, String)>,
    /// The format files of plugins that were not loaded: the plugin, the file, why.
    pub plugin_errors: Vec<crate::plugins::FormatError>,
}

impl Library {
    /// The built-in formats only.
    pub fn builtin() -> Self {
        Self {
            formats: builtins(),
            errors: Vec::new(),
            plugin_errors: Vec::new(),
        }
    }

    /// The built-in formats and the format files of `dir` (a missing folder has none), in
    /// file name order. A later format with the name of an earlier one is an error.
    pub fn load(dir: &Path) -> Self {
        let mut lib = Self::builtin();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return lib;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().ends_with(FORMAT_SUFFIX))
            })
            .collect();
        files.sort();
        let mut loaded = 0;
        for path in files {
            if loaded >= MAX_FORMATS {
                lib.errors
                    .push((path, format!("more than {MAX_FORMATS} formats")));
                continue;
            }
            match read_file(&path) {
                Ok(mut format) => {
                    if lib.find(format.name()).is_some() {
                        let e = format!("a format named {} exists already", format.name());
                        lib.errors.push((path, e));
                        continue;
                    }
                    format.file = Some(path);
                    lib.formats.push(Arc::new(format));
                    loaded += 1;
                }
                Err(e) => lib.errors.push((path, e)),
            }
        }
        lib
    }

    /// `load` plus the formats of the enabled plugins of `catalog`.
    pub fn load_with_plugins(
        dir: &Path,
        catalog: &crate::plugins::Catalog,
        approvals: &crate::plugins::Approvals,
    ) -> Self {
        let mut lib = Self::load(dir);
        let (formats, errors) = crate::plugins::bundled_formats(catalog, approvals);
        lib.plugin_errors = errors;
        for format in formats {
            if lib.find(format.name()).is_some() {
                let e = format!("a format named {} exists already", format.name());
                let plugin = format.plugin.clone().unwrap_or_default();
                lib.plugin_errors
                    .push((plugin, format.file.clone().unwrap_or_default(), e));
                continue;
            }
            lib.formats.push(format);
        }
        lib
    }

    /// The format named `name` (any case).
    pub fn find(&self, name: &str) -> Option<&Arc<LogFormat>> {
        self.formats
            .iter()
            .find(|f| f.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Reads and compiles one format file.
pub fn read_file(path: &Path) -> Result<LogFormat, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("not a format file".to_string());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    LogFormat::compile(FormatSpec::from_ini(&text)?)
}

/// Writes `spec` into the folder `dir` (created when missing) and returns the file.
pub fn write_file(dir: &Path, spec: &FormatSpec) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(spec.file_name());
    crate::config::overwrite_regular_file(&path, spec.to_ini().as_bytes())?;
    Ok(path)
}

/// The formats folder: next to `fasttail.ini`.
pub fn default_dir() -> PathBuf {
    let config = crate::config::FastTailConfig::config_path();
    config
        .parent()
        .map_or_else(|| PathBuf::from(FORMATS_DIR), |d| d.join(FORMATS_DIR))
}

static LIBRARY: RwLock<Option<Arc<Library>>> = RwLock::new(None);

/// The formats of the process: the ones `set_library` installed, the built-in ones until
/// then.
pub fn library() -> Arc<Library> {
    if let Some(lib) = LIBRARY.read().ok().and_then(|l| l.clone()) {
        return lib;
    }
    let lib = Arc::new(Library::builtin());
    if let Ok(mut slot) = LIBRARY.write() {
        slot.get_or_insert_with(|| lib.clone());
    }
    lib
}

/// Installs `lib` as the formats of the process.
pub fn set_library(lib: Library) {
    if let Ok(mut slot) = LIBRARY.write() {
        *slot = Some(Arc::new(lib));
    }
}

/// Loads the formats folder next to `fasttail.ini`, and the formats of the enabled
/// plugins, as the formats of the process.
pub fn load_default() {
    let approvals = crate::config::FastTailConfig::load_read_only().plugins;
    load_with_plugins(&approvals);
}

/// Loads the formats again, with the plugins `approvals` enables.
pub fn load_with_plugins(approvals: &crate::plugins::Approvals) {
    let catalog = crate::plugins::discover(&crate::plugins::default_dir());
    set_library(Library::load_with_plugins(
        &default_dir(),
        &catalog,
        approvals,
    ));
}

/// The format detection picks for the sampled `lines` of the file named `file`: a user
/// format (built-in ones are chosen by hand) whose patterns match at least
/// `fields::DETECT_PERCENT` of the entries; one the file name is associated with first,
/// then the highest share, the most named groups, the name.
pub fn detect(lib: &Library, file: Option<&str>, lines: &[&str]) -> Option<Arc<LogFormat>> {
    // (named after the file, share in 1/10,000, named groups) and the format.
    type Rank = (bool, usize, usize);
    let mut best: Option<(Rank, &Arc<LogFormat>)> = None;
    for format in lib
        .formats
        .iter()
        .filter(|f| !f.builtin || f.plugin.is_some())
    {
        let (mut entries, mut matched) = (0usize, 0usize);
        for line in lines.iter().filter(|l| !l.trim().is_empty()) {
            if !format.starts_entry(line) {
                continue;
            }
            entries += 1;
            if format.is_match(line) {
                matched += 1;
            }
        }
        if entries == 0 || matched * 100 < entries * crate::fields::DETECT_PERCENT {
            continue;
        }
        let glob = file.is_some_and(|f| format.covers_file(f));
        let key = (glob, matched * 10_000 / entries, format.group_count());
        let better = match &best {
            None => true,
            Some((k, f)) => key > *k || (key == *k && format.name() < f.name()),
        };
        if better {
            best = Some((key, format));
        }
    }
    best.map(|(_, f)| f.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYMENTS: &str = r#"[format]
name = Payments
regex = ^(?P<ts>\d\d\.\d\d\.\d\d \d\d:\d\d:\d\d,\d{3}) \[(?P<thread>[^\]]+)\] (?P<level>\w+) (?P<msg>.*)$
timestamp_field = ts
timestamp_format = %d.%m.%y %H:%M:%S,%3f
level_field = level
level_map = W=warn,E=error, SEVERE=error
message_field = msg
files = payments-*.log; pay.log
columns = ts,thread,level
sample.1 = 28.09.26 14:02:11,123 [main] E card declined
"#;

    #[test]
    fn a_format_file_reads_compiles_and_round_trips() {
        let spec = FormatSpec::from_ini(PAYMENTS).unwrap();
        assert_eq!(spec.name, "Payments");
        assert!(spec.patterns[0].contains(r"\d\d"), "backslashes kept");
        assert_eq!(spec.files, vec!["payments-*.log", "pay.log"]);
        assert_eq!(spec.columns, vec!["ts", "thread", "level"]);
        assert_eq!(FormatSpec::from_ini(&spec.to_ini()).unwrap(), spec);
        let format = LogFormat::compile(spec).unwrap();
        let line = "28.09.26 14:02:11,123 [main] E card declined";
        crate::fields::with_spans(|spans| {
            assert!(format.scan(line, spans));
            assert_eq!(format.level(spans, line), Some(LogLevel::Error));
            let want = crate::timestamp::date_to_days(2026, 9, 28).unwrap() * 86_400_000
                + 14 * 3_600_000
                + 2 * 60_000
                + 11_123;
            assert_eq!(format.timestamp(spans, line), Some(want));
        });
        assert!(format.covers_file("payments-2026-09-28.log"));
        assert!(!format.covers_file("other.log"));
        assert_eq!(format.spec.file_name(), "Payments.fasttail-format.ini");
    }

    #[test]
    fn invalid_formats_say_why() {
        let bad = |text: &str| LogFormat::compile(FormatSpec::from_ini(text).unwrap()).unwrap_err();
        assert!(bad("[format]\nname=x\nregex=^(?P<a>\\d+\n").contains("pattern 1"));
        assert!(bad("[format]\nname=x\nregex=^\\d+$\n").contains("no named group"));
        assert!(
            bad("[format]\nname=x\nregex=(?P<a>.)\nregex.2=(?P<b>.)\nlevel_field=a\n")
                .contains("pattern 2 has no group a")
        );
        assert!(
            bad("[format]\nname=x\nregex=(?P<a>.)\nlevel_map=W=sometimes\n").contains("level_map")
        );
        assert!(bad("[format]\nname=x\nregex=(?P<a>.*)\ntimestamp_field=a\ntimestamp_format=%Y\nsample.1=no year\n")
            .contains("no sample"));
        assert!(
            bad("[format]\nname=x\nregex=(?P<a>.)\ntimestamp_zone=mars\n")
                .contains("timestamp_zone")
        );
        assert!(bad("[format]\nregex=(?P<a>.)\n").contains("no name"));
        assert!(FormatSpec::from_ini("name=x").is_err(), "no section");
    }

    #[test]
    fn the_folder_loads_with_its_errors_and_detection_prefers_the_file_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("payments.fasttail-format.ini"), PAYMENTS).unwrap();
        // A wider pattern matching the same lines, not associated with the file name.
        let wide = "[format]\nname=Any\nregex=^(?P<a>\\S+) (?P<b>\\S+) (?P<c>\\S+) (?P<d>\\S+) (?P<e>.*)$\n";
        std::fs::write(dir.path().join("any.fasttail-format.ini"), wide).unwrap();
        std::fs::write(
            dir.path().join("broken.fasttail-format.ini"),
            "[format]\nname=B\nregex=(\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("dupe.fasttail-format.ini"), PAYMENTS).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a format").unwrap();
        let lib = Library::load(dir.path());
        assert!(lib.find("payments").is_some());
        assert!(lib.find(APACHE_NAME).is_some_and(|f| f.builtin));
        assert_eq!(lib.errors.len(), 2, "{:?}", lib.errors);

        let lines = [
            "28.09.26 14:02:11,123 [main] E card declined",
            "  at Card.check(Card.java:12)",
            "28.09.26 14:02:12,000 [main] I card accepted",
        ];
        let pick = |file| detect(&lib, file, &lines).map(|f| f.name().to_string());
        assert_eq!(pick(Some("payments-1.log")).as_deref(), Some("Payments"));
        // Both match every entry: the most named groups, then the name.
        assert_eq!(pick(Some("x.log")).as_deref(), Some("Any"));
        assert_eq!(
            detect(&lib, None, &["plain text"]).map(|f| f.name().to_string()),
            None
        );
    }

    #[test]
    fn the_builtin_formats_read_their_samples() {
        assert_eq!(builtins().len(), 2, "every built-in format compiles");
        for format in builtins() {
            let sample = &format.spec.samples[0];
            crate::fields::with_spans(|spans| {
                assert!(format.scan(sample, spans), "{}", format.name());
                assert!(
                    format.timestamp(spans, sample).is_some(),
                    "{}",
                    format.name()
                );
            });
        }
    }
}
