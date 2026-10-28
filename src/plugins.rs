// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Plugins: folders in `plugins/` next to `fasttail.ini`, each with a `plugin.toml`
//! manifest (`api = 1`). A **source** plugin names a program whose standard output is a
//! log (run without a shell, its parameters expanded one argument at a time, see
//! `plugin_source`); a **formats** plugin only ships log format files
//! (`*.fasttail-format.ini`), loaded as `<plugin>/<format>`. A plugin folder of either
//! kind may ship format files.
//!
//! A source plugin runs programs, so it runs only once the user approved it: the SHA-256
//! of its manifest is kept in `[plugins]` of `fasttail.ini`, and a changed manifest needs
//! a new approval. Formats run no code and load unless the user disabled them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::log_format::LogFormat;

/// The manifest version this build reads.
pub const API: i64 = 1;
/// The plugins folder, next to `fasttail.ini`.
pub const PLUGINS_DIR: &str = "plugins";
/// The manifest of a plugin folder.
pub const MANIFEST: &str = "plugin.toml";
/// The scheme of a plugin stream's path.
pub const SCHEME: &str = "plugin://";
/// Plugin folders looked at, at most.
const MAX_PLUGINS: usize = 64;
/// Longest manifest read.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Parameters of a plugin, at most.
const MAX_PARAMS: usize = 16;

/// What a plugin brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Source,
    Formats,
}

/// What a source plugin does when its program ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Restart {
    /// The stream stays open, saying the source ended and its exit code.
    #[default]
    Never,
    /// Started again after 1, 2, 5, 10, then every 30 seconds.
    OnExit,
}

/// The type of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Text,
    Number,
    Choice,
    /// Never written to the workspace or sessions; asked again on restore.
    Secret,
}

/// A parameter of a source plugin, `[[source.param]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub kind: ParamType,
    pub default: String,
    /// The values of a `choice`.
    pub choices: Vec<String>,
    /// `allow_dash = true`: a value may start with `-` (a negative number, an option).
    pub allow_dash: bool,
}

/// `[source]` of a manifest.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Source {
    pub command: String,
    pub args: Vec<String>,
    pub restart: Restart,
    pub params: Vec<Param>,
}

/// A plugin whose manifest could be read.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub dir: PathBuf,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: Kind,
    /// `Some` for a source plugin.
    pub source: Option<Source>,
    /// SHA-256 of `plugin.toml`, hex.
    pub hash: String,
    /// Keys the manifest has that this build does not know.
    pub warnings: Vec<String>,
}

/// A plugin folder that could not be loaded.
#[derive(Debug, Clone)]
pub struct Broken {
    pub dir: PathBuf,
    pub error: String,
}

/// Whether a plugin runs, as Settings ▸ Plugins shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Enabled,
    /// A source plugin never approved, or a formats plugin turned off.
    Disabled,
    /// Approved once, but its manifest changed since.
    Changed,
}

/// The approvals of `[plugins]` in `fasttail.ini`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Approvals {
    /// Source plugin name → SHA-256 of the manifest approved.
    pub approved: BTreeMap<String, String>,
    /// Formats plugins the user turned off.
    pub disabled: BTreeSet<String>,
}

impl Approvals {
    pub fn state(&self, plugin: &Plugin) -> State {
        match plugin.kind {
            Kind::Formats if self.disabled.contains(&plugin.name) => State::Disabled,
            Kind::Formats => State::Enabled,
            Kind::Source => match self.approved.get(&plugin.name) {
                Some(h) if *h == plugin.hash => State::Enabled,
                Some(_) => State::Changed,
                None => State::Disabled,
            },
        }
    }

    /// Turns `plugin` on (approving its manifest as it is now) or off.
    pub fn set_enabled(&mut self, plugin: &Plugin, on: bool) {
        match plugin.kind {
            Kind::Source if on => {
                self.approved
                    .insert(plugin.name.clone(), plugin.hash.clone());
            }
            Kind::Source => {
                self.approved.remove(&plugin.name);
            }
            Kind::Formats if on => {
                self.disabled.remove(&plugin.name);
            }
            Kind::Formats => {
                self.disabled.insert(plugin.name.clone());
            }
        }
    }
}

/// The plugins found.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub dir: PathBuf,
    pub plugins: Vec<Plugin>,
    pub broken: Vec<Broken>,
}

impl Catalog {
    pub fn find(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.name == name)
    }
}

/// The plugins folder next to `fasttail.ini`.
pub fn default_dir() -> PathBuf {
    let config = crate::config::FastTailConfig::config_path();
    let dir = config
        .parent()
        .map_or_else(|| PathBuf::from(PLUGINS_DIR), |d| d.join(PLUGINS_DIR));
    // Absolute: a program and `{plugin_dir}` must name the same place once the child
    // runs in the plugin folder (a `fasttail.ini` found in the working directory gives a
    // relative path).
    std::path::absolute(&dir).unwrap_or(dir)
}

/// SHA-256 of `bytes`, hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether `name` can name a plugin or a parameter: letters, digits, `-`, `_`, `.`.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !name.starts_with('.')
}

/// Reads every plugin folder of `dir` (a missing folder has none), in name order.
pub fn discover(dir: &Path) -> Catalog {
    let dir = &std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut catalog = Catalog {
        dir: dir.to_path_buf(),
        ..Default::default()
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return catalog;
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).is_file())
        .collect();
    folders.sort();
    for folder in folders.into_iter().take(MAX_PLUGINS) {
        match read_manifest(&folder) {
            Ok(plugin) if catalog.find(&plugin.name).is_some() => catalog.broken.push(Broken {
                dir: folder,
                error: format!("another plugin is named {}", plugin.name),
            }),
            Ok(plugin) => catalog.plugins.push(plugin),
            Err(error) => catalog.broken.push(Broken { dir: folder, error }),
        }
    }
    catalog
}

/// Reads the manifest of the plugin folder `dir`.
pub fn read_manifest(dir: &Path) -> Result<Plugin, String> {
    let path = dir.join(MANIFEST);
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(format!("{MANIFEST} is larger than 64 KB"));
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{MANIFEST} is not UTF-8"))?;
    let mut plugin = parse_manifest(text)?;
    plugin.dir = dir.to_path_buf();
    plugin.hash = sha256_hex(&bytes);
    Ok(plugin)
}

fn string(table: &toml::Table, key: &str) -> Result<Option<String>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("`{key}` must be a string")),
    }
}

fn required(table: &toml::Table, key: &str, section: &str) -> Result<String, String> {
    string(table, key)?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("`{section}{key}` is missing"))
}

fn unknown_keys(table: &toml::Table, known: &[&str], section: &str, out: &mut Vec<String>) {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            out.push(format!("unknown key `{section}{key}`"));
        }
    }
}

/// Parses a manifest (its folder and hash are left empty).
pub fn parse_manifest(text: &str) -> Result<Plugin, String> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| format!("{MANIFEST} cannot be read: {}", e.message()))?;
    match table.get("api") {
        Some(toml::Value::Integer(API)) => {}
        Some(toml::Value::Integer(n)) => {
            return Err(format!(
                "unsupported API version {n} (this FastTail reads api = {API})"
            ))
        }
        Some(_) => return Err("`api` must be a number".into()),
        None => return Err("`api` is missing".into()),
    }
    let name = required(&table, "name", "")?;
    if !valid_name(&name) {
        return Err(format!(
            "the name `{name}` may hold only letters, digits, `-`, `_` and `.`"
        ));
    }
    let version = required(&table, "version", "")?;
    let description = string(&table, "description")?.unwrap_or_default();
    let kind = match required(&table, "kind", "")?.as_str() {
        "source" => Kind::Source,
        "formats" => Kind::Formats,
        other => return Err(format!("unknown kind `{other}` (source or formats)")),
    };
    let mut warnings = Vec::new();
    unknown_keys(
        &table,
        &["api", "name", "version", "description", "kind", "source"],
        "",
        &mut warnings,
    );
    let source = match kind {
        Kind::Source => {
            let Some(toml::Value::Table(s)) = table.get("source") else {
                return Err("`[source]` is missing".into());
            };
            Some(parse_source(s, &mut warnings)?)
        }
        Kind::Formats => None,
    };
    Ok(Plugin {
        dir: PathBuf::new(),
        name,
        version,
        description,
        kind,
        source,
        hash: String::new(),
        warnings,
    })
}

fn parse_source(s: &toml::Table, warnings: &mut Vec<String>) -> Result<Source, String> {
    unknown_keys(
        s,
        &["command", "args", "restart", "param"],
        "source.",
        warnings,
    );
    let command = required(s, "command", "source.")?;
    let args = match s.get("args") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or("`source.args` must be a list of strings")?,
        Some(_) => return Err("`source.args` must be a list of strings".into()),
    };
    let restart = match string(s, "restart")?.as_deref() {
        None | Some("never") => Restart::Never,
        Some("on-exit") => Restart::OnExit,
        Some(other) => {
            return Err(format!(
                "unknown `source.restart` `{other}` (never or on-exit)"
            ))
        }
    };
    let mut params = Vec::new();
    match s.get("param") {
        None => {}
        Some(toml::Value::Array(list)) => {
            if list.len() > MAX_PARAMS {
                return Err(format!("more than {MAX_PARAMS} parameters"));
            }
            for p in list {
                let toml::Value::Table(p) = p else {
                    return Err("`[[source.param]]` must be tables".into());
                };
                let param = parse_param(p, warnings)?;
                if params.iter().any(|q: &Param| q.id == param.id) {
                    return Err(format!("two parameters are named `{}`", param.id));
                }
                params.push(param);
            }
        }
        Some(_) => return Err("`[[source.param]]` must be tables".into()),
    }
    Ok(Source {
        command,
        args,
        restart,
        params,
    })
}

fn parse_param(p: &toml::Table, warnings: &mut Vec<String>) -> Result<Param, String> {
    unknown_keys(
        p,
        &["id", "label", "type", "default", "choices", "allow_dash"],
        "source.param.",
        warnings,
    );
    let id = required(p, "id", "source.param.")?;
    if !valid_name(&id) || ["plugin_dir", "config_dir"].contains(&id.as_str()) {
        return Err(format!("`{id}` cannot name a parameter"));
    }
    let kind = match string(p, "type")?.as_deref() {
        None | Some("text") => ParamType::Text,
        Some("number") => ParamType::Number,
        Some("choice") => ParamType::Choice,
        Some("secret") => ParamType::Secret,
        Some(other) => return Err(format!("unknown parameter type `{other}`")),
    };
    let default = match p.get("default") {
        None => String::new(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(toml::Value::Integer(n)) => n.to_string(),
        Some(toml::Value::Float(f)) => f.to_string(),
        Some(_) => {
            return Err(format!(
                "the default of `{id}` must be a string or a number"
            ))
        }
    };
    let choices = match p.get("choices") {
        None => Vec::new(),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("the choices of `{id}` must be strings"))?,
        Some(_) => return Err(format!("the choices of `{id}` must be a list")),
    };
    if kind == ParamType::Choice && choices.is_empty() {
        return Err(format!("the choice `{id}` has no `choices`"));
    }
    if kind == ParamType::Secret && !default.is_empty() {
        return Err(format!("the secret `{id}` cannot have a default"));
    }
    let allow_dash = match p.get("allow_dash") {
        None => false,
        Some(toml::Value::Boolean(b)) => *b,
        Some(_) => return Err(format!("`allow_dash` of `{id}` must be true or false")),
    };
    Ok(Param {
        allow_dash,
        label: string(p, "label")?.unwrap_or_else(|| id.clone()),
        id,
        kind,
        default,
        choices,
    })
}

/// Why a value does not fit its parameter, `None` when it does.
pub fn check_value(param: &Param, value: &str) -> Option<String> {
    match param.kind {
        ParamType::Number if value.trim().parse::<f64>().is_err() => {
            Some(format!("{}: a number is needed", param.label))
        }
        ParamType::Choice if !param.choices.iter().any(|c| c == value) => Some(format!(
            "{}: one of {}",
            param.label,
            param.choices.join(", ")
        )),
        _ if value.contains(['\n', '\r', '\0']) => Some(format!("{}: one line only", param.label)),
        // A value starting with `-` could be read as an option by the program (`ssh
        // -oProxyCommand=…`): only where the manifest allows it.
        _ if value.starts_with('-') && !param.allow_dash => {
            Some(format!("{}: may not start with -", param.label))
        }
        _ => None,
    }
}

/// The arguments of `source` with the placeholders replaced: `{id}` by the parameter's
/// value, `{plugin_dir}` and `{config_dir}` by the folders. Each argument stays one
/// argument whatever the values hold: no shell ever sees them.
pub fn expand_args(
    source: &Source,
    values: &BTreeMap<String, String>,
    plugin_dir: &Path,
    config_dir: &Path,
) -> Vec<String> {
    source
        .args
        .iter()
        .map(|arg| {
            let mut out = String::with_capacity(arg.len());
            let mut rest = arg.as_str();
            while let Some(open) = rest.find('{') {
                out.push_str(&rest[..open]);
                let after = &rest[open + 1..];
                let Some(close) = after.find('}') else {
                    out.push_str(&rest[open..]);
                    rest = "";
                    break;
                };
                let key = &after[..close];
                let value = match key {
                    "plugin_dir" => Some(plugin_dir.to_string_lossy().into_owned()),
                    "config_dir" => Some(config_dir.to_string_lossy().into_owned()),
                    _ => source.params.iter().find(|p| p.id == key).map(|p| {
                        values
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| p.default.clone())
                    }),
                };
                match value {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            out.push_str(rest);
            out
        })
        .collect()
}

/// The program to run: a path relative to the plugin folder when the folder has it,
/// else the command as written (a name looked up on `PATH`, or an absolute path).
pub fn resolve_command(plugin_dir: &Path, command: &str) -> PathBuf {
    // A bare name (`kcat`) is always looked up on `PATH`: a file dropped into the plugin
    // folder cannot take its place. A path with a separator (`./run.sh`, `bin/tool`) is
    // the plugin's own program.
    let path = Path::new(command);
    if path.is_absolute() || !command.contains(['/', '\\']) {
        return path.to_path_buf();
    }
    let local = plugin_dir.join(command);
    #[cfg(windows)]
    if path.extension().is_none() && !local.is_file() {
        for ext in ["exe", "cmd", "bat"] {
            let with = local.with_extension(ext);
            if with.is_file() {
                return with;
            }
        }
    }
    local
}

/// A plugin stream's identity: `plugin://<name>?<param>=<value>&…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginUri {
    pub name: String,
    pub values: BTreeMap<String, String>,
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl PluginUri {
    /// Reads `path` as a plugin URI; `None` when it is not one.
    pub fn parse(path: &Path) -> Option<Self> {
        let text = path.to_str()?;
        let rest = text.strip_prefix(SCHEME)?;
        let (name, query) = rest.split_once('?').unwrap_or((rest, ""));
        let name = name.trim_end_matches('/');
        if !valid_name(name) {
            return None;
        }
        let mut values = BTreeMap::new();
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            values.insert(
                crate::web::url_decode(&k.replace('+', "%2B")),
                crate::web::url_decode(&v.replace('+', "%2B")),
            );
        }
        Some(Self {
            name: name.to_string(),
            values,
        })
    }

    /// The URI, without the values of `secret` parameters (all values kept when
    /// `plugin` is not known).
    pub fn to_uri(&self, plugin: Option<&Plugin>) -> String {
        // Only the values of known parameters that are not secret are kept: for a plugin
        // that is not installed, none (they could be secrets).
        let kept = |k: &str| {
            plugin.and_then(|p| p.source.as_ref()).is_some_and(|s| {
                s.params
                    .iter()
                    .any(|p| p.id == k && p.kind != ParamType::Secret)
            })
        };
        let query: Vec<String> = self
            .values
            .iter()
            .filter(|(k, _)| kept(k))
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect();
        if query.is_empty() {
            format!("{SCHEME}{}", self.name)
        } else {
            format!("{SCHEME}{}?{}", self.name, query.join("&"))
        }
    }

    /// The values asked again before the stream can start: the secrets missing.
    pub fn missing_secrets(&self, plugin: &Plugin) -> bool {
        plugin.source.as_ref().is_some_and(|s| {
            s.params
                .iter()
                .any(|p| p.kind == ParamType::Secret && !self.values.contains_key(&p.id))
        })
    }
}

/// The path a stream is kept under: for a plugin URI, the form `to_uri` writes (keys
/// sorted, values encoded, secrets and unknown values left out); any other path as it is.
pub fn stored_path(path: &Path) -> PathBuf {
    match PluginUri::parse(path) {
        Some(uri) => {
            let catalog = discover(&default_dir());
            PathBuf::from(uri.to_uri(catalog.find(&uri.name)))
        }
        None => path.to_path_buf(),
    }
}

/// Whether `path` is a plugin URI.
pub fn is_plugin_path(path: &Path) -> bool {
    path.to_str().is_some_and(|s| s.starts_with(SCHEME))
}

/// A format file of a plugin that did not load: the plugin, the file, why.
pub type FormatError = (String, PathBuf, String);

/// A format file of the plugin `plugin`: compiled under the name `<plugin>/<name>`,
/// read-only, and refused when a sample line does not match its patterns.
fn read_bundled(path: &Path, plugin: &str) -> Result<LogFormat, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 256 * 1024 {
        return Err("not a format file".to_string());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut spec = crate::log_format::FormatSpec::from_ini(&text)?;
    spec.name = format!("{plugin}/{}", spec.name.trim());
    let mut format = LogFormat::compile(spec)?;
    if let Some(i) = format.spec.samples.iter().position(|s| !format.is_match(s)) {
        return Err(format!("sample {} does not match its patterns", i + 1));
    }
    format.builtin = true;
    format.plugin = Some(plugin.to_string());
    format.file = Some(path.to_path_buf());
    Ok(format)
}

/// The format files of the enabled plugins, compiled and named `<plugin>/<format>`;
/// the files that failed, with the plugin and the reason.
pub fn bundled_formats(
    catalog: &Catalog,
    approvals: &Approvals,
) -> (Vec<Arc<LogFormat>>, Vec<FormatError>) {
    let mut formats = Vec::new();
    let mut errors = Vec::new();
    for plugin in &catalog.plugins {
        if approvals.state(plugin) != State::Enabled {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&plugin.dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.to_ascii_lowercase()
                        .ends_with(crate::log_format::FORMAT_SUFFIX)
                })
            })
            .collect();
        files.sort();
        for path in files {
            match read_bundled(&path, &plugin.name) {
                Ok(format) => formats.push(Arc::new(format)),
                Err(e) => errors.push((plugin.name.clone(), path, e)),
            }
        }
    }
    (formats, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KCAT: &str = r#"
api = 1
name = "kafka-topic"
version = "0.1.0"
description = "A Kafka topic through kcat"
kind = "source"
colour = "red"
[source]
command = "kcat"
args = ["-C", "-b", "{broker}", "-t", "{topic}", "-o", "end", "-u", "{nope}", "x{plugin_dir}y"]
restart = "on-exit"
[[source.param]]
id = "broker"
label = "Broker"
type = "text"
default = "localhost:9092"
[[source.param]]
id = "topic"
label = "Topic"
[[source.param]]
id = "token"
type = "secret"
"#;

    #[test]
    fn source_manifest_parses_with_warnings_for_unknown_keys() {
        let p = parse_manifest(KCAT).unwrap();
        assert_eq!((p.name.as_str(), p.kind), ("kafka-topic", Kind::Source));
        let s = p.source.as_ref().unwrap();
        assert_eq!(s.restart, Restart::OnExit);
        assert_eq!(s.params.len(), 3);
        assert_eq!(s.params[1].label, "Topic");
        assert_eq!(p.warnings, vec!["unknown key `colour`".to_string()]);
    }

    #[test]
    fn bad_manifests_say_why() {
        let err = |t: &str| parse_manifest(t).unwrap_err();
        assert!(err("api = 2\nname='x'\nversion='1'\nkind='formats'")
            .contains("unsupported API version 2"));
        assert!(err("name='x'").contains("`api` is missing"));
        assert!(err("api = 1\nversion='1'\nkind='formats'").contains("`name`"));
        assert!(err("api = 1\nname='a b'\nversion='1'\nkind='formats'").contains("may hold only"));
        assert!(err("api = 1\nname='x'\nversion='1'\nkind='source'").contains("[source]"));
        assert!(err("api = [").contains("cannot be read"));
        assert!(err("api = 1\nname='x'\nversion='1'\nkind='source'\n[source]\ncommand='a'\n[[source.param]]\nid='c'\ntype='choice'").contains("no `choices`"));
        assert!(parse_manifest("api = 1\nname='x'\nversion='1'\nkind='formats'").is_ok());
    }

    #[test]
    fn values_stay_one_argument_each() {
        let p = parse_manifest(KCAT).unwrap();
        let s = p.source.as_ref().unwrap();
        let mut values = BTreeMap::new();
        values.insert("topic".to_string(), "orders; rm -rf /".to_string());
        let args = expand_args(s, &values, Path::new("/p"), Path::new("/c"));
        assert_eq!(args[2], "localhost:9092");
        assert_eq!(args[4], "orders; rm -rf /");
        assert_eq!(args[8], "{nope}");
        assert_eq!(args[9], "x/py");
        assert_eq!(args.len(), s.args.len());
    }

    #[test]
    fn plugin_folders_and_commands_are_absolute_or_on_path() {
        let catalog = discover(Path::new("no-such-plugins-folder"));
        assert!(catalog.dir.is_absolute());
        let dir = std::path::absolute("plugins/p").unwrap();
        assert_eq!(resolve_command(&dir, "kcat"), PathBuf::from("kcat"));
        assert_eq!(resolve_command(&dir, "./run.sh"), dir.join("./run.sh"));
        assert!(resolve_command(&dir, "bin/tool").is_absolute());
    }

    #[test]
    fn approval_follows_the_manifest_hash() {
        let mut p = parse_manifest(KCAT).unwrap();
        p.hash = sha256_hex(KCAT.as_bytes());
        let mut a = Approvals::default();
        assert_eq!(a.state(&p), State::Disabled);
        a.set_enabled(&p, true);
        assert_eq!(a.state(&p), State::Enabled);
        p.hash = sha256_hex(b"changed");
        assert_eq!(a.state(&p), State::Changed);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn uri_round_trips_without_secrets() {
        let p = parse_manifest(KCAT).unwrap();
        let uri = PluginUri::parse(Path::new(
            "plugin://kafka-topic?broker=localhost%3A9092&topic=a%26b+c&token=s3cret",
        ))
        .unwrap();
        assert_eq!(uri.values["topic"], "a&b+c");
        let text = uri.to_uri(Some(&p));
        assert!(!text.contains("s3cret") && !text.contains("token"));
        let back = PluginUri::parse(Path::new(&text)).unwrap();
        assert_eq!(back.values["topic"], "a&b+c");
        assert!(back.missing_secrets(&p));
        // A plugin that is not installed keeps no value: any could be a secret.
        assert_eq!(uri.to_uri(None), "plugin://kafka-topic");
        assert!(PluginUri::parse(Path::new("plugin://../x")).is_none());
        assert!(PluginUri::parse(Path::new("/var/log/x")).is_none());
    }

    #[test]
    fn values_are_checked_against_their_type() {
        let p = Param {
            id: "n".into(),
            label: "N".into(),
            kind: ParamType::Number,
            default: String::new(),
            choices: Vec::new(),
            allow_dash: false,
        };
        assert!(check_value(&p, "12.5").is_none());
        assert!(check_value(&p, "-3").is_some(), "a dash needs allow_dash");
        let signed = Param {
            allow_dash: true,
            ..p.clone()
        };
        assert!(check_value(&signed, "-3").is_none());
        assert!(check_value(&p, "x").is_some());
        let c = Param {
            kind: ParamType::Choice,
            choices: vec!["a".into(), "b".into()],
            ..p.clone()
        };
        assert!(check_value(&c, "b").is_none() && check_value(&c, "z").is_some());
        let t = Param {
            kind: ParamType::Text,
            ..p
        };
        assert!(check_value(&t, "a\nb").is_some());
    }
}
