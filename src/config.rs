//! Configuration file. Everything the settings window can do is plain JSON here.
//!
//! The legacy `pyloadConfig.json` format of pyload-clicknload (`pyloadUrl`, `pyloadUser`,
//! `pyloadPW`) is still read and migrated into a pyLoad output on the fly.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = "pyloadConfig.json";
pub const DEFAULT_LISTEN: &str = "127.0.0.1:9666";

fn yes() -> bool {
    true
}

fn default_listen() -> String {
    DEFAULT_LISTEN.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Notifications {
    #[default]
    All,
    Errors,
    Off,
}

/// Accepts the legacy boolean as well as the mode name.
#[derive(Deserialize)]
#[serde(untagged)]
enum NotificationsRaw {
    Bool(bool),
    Mode(Notifications),
}

fn de_notifications<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Notifications, D::Error> {
    Ok(match NotificationsRaw::deserialize(d)? {
        NotificationsRaw::Bool(true) => Notifications::All,
        NotificationsRaw::Bool(false) => Notifications::Errors,
        NotificationsRaw::Mode(m) => m,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Address of the Click'n'Load listener. Browsers always talk to 127.0.0.1:9666;
    /// use `0.0.0.0:9666` inside Docker. `$KLIKNLOAD_LISTEN` overrides it.
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default, deserialize_with = "de_notifications")]
    pub notifications: Notifications,
    /// Copy links to the clipboard when no output succeeded.
    #[serde(default = "yes")]
    pub clipboard_fallback: bool,
    /// Drop duplicate links inside a package.
    #[serde(default = "yes")]
    pub dedupe_links: bool,
    #[serde(default)]
    pub outputs: Vec<Output>,

    /// Unknown keys are kept so saving never loses anything.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Only links matching this regex are passed to the output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<String>,
    /// Links matching this regex are not passed to the output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    #[serde(flatten)]
    pub kind: OutputKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputKind {
    Pyload(PyloadOutput),
    Clipboard(ClipboardOutput),
    File(FileOutput),
    Http(HttpOutput),
    Command(CommandOutput),
}

impl OutputKind {
    /// One default instance per output type, for "add output" in the settings window.
    pub fn all_defaults() -> Vec<OutputKind> {
        vec![
            OutputKind::Pyload(PyloadOutput::default()),
            OutputKind::Clipboard(ClipboardOutput::default()),
            OutputKind::File(FileOutput::default()),
            OutputKind::Http(HttpOutput::default()),
            OutputKind::Command(CommandOutput::default()),
        ]
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            OutputKind::Pyload(_) => "pyload",
            OutputKind::Clipboard(_) => "clipboard",
            OutputKind::File(_) => "file",
            OutputKind::Http(_) => "http",
            OutputKind::Command(_) => "command",
        }
    }

    pub fn default_name(&self) -> &'static str {
        match self {
            OutputKind::Pyload(_) => "pyLoad",
            OutputKind::Clipboard(_) => "Zwischenablage",
            OutputKind::File(_) => "Datei",
            OutputKind::Http(_) => "HTTP-Request",
            OutputKind::Command(_) => "Befehl",
        }
    }
}

fn tpl_package() -> String {
    "{{package}}".into()
}
fn tpl_links() -> String {
    "{{links}}".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Destination {
    #[default]
    Queue,
    Collector,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PyloadOutput {
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// API key (`pl_…`), preferred over user/password when set.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    #[serde(default)]
    pub destination: Destination,
    #[serde(default = "tpl_package")]
    pub package_name: String,
}

impl Default for PyloadOutput {
    fn default() -> Self {
        Self {
            url: String::new(),
            user: String::new(),
            password: String::new(),
            api_key: String::new(),
            destination: Destination::Queue,
            package_name: tpl_package(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardOutput {
    #[serde(default = "tpl_links")]
    pub template: String,
}

impl Default for ClipboardOutput {
    fn default() -> Self {
        Self {
            template: tpl_links(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    /// One link per line.
    #[default]
    Txt,
    /// JSON object per package (JSON Lines when appending).
    Json,
    /// `package,link,password,source,date`.
    Csv,
    /// JDownloader folder watch `.crawljob`.
    Crawljob,
    /// Own template.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FileMode {
    /// A new file per package.
    #[default]
    New,
    /// Append every package to the same file.
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnConflict {
    /// `name (2).txt`, `name (3).txt`, …
    #[default]
    Number,
    Overwrite,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutput {
    pub directory: String,
    pub filename: String,
    #[serde(default)]
    pub format: FileFormat,
    /// Content template for `format: custom`, rendered once per package.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub template: String,
    #[serde(default)]
    pub mode: FileMode,
    #[serde(default)]
    pub on_conflict: OnConflict,
}

impl Default for FileOutput {
    fn default() -> Self {
        Self {
            directory: "~/Downloads/kliknload".into(),
            filename: "{{date}} {{package}}.txt".into(),
            format: FileFormat::Txt,
            template: String::new(),
            mode: FileMode::New,
            on_conflict: OnConflict::Number,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyValue {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HttpAuth {
    #[default]
    None,
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    Bearer {
        #[serde(default)]
        token: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BodyType {
    #[default]
    None,
    /// Template must render to valid JSON; values are escaped as JSON string content.
    Json,
    /// `application/x-www-form-urlencoded` from the `form` fields.
    Form,
    /// Raw text body.
    Text,
}

fn default_method() -> String {
    "POST".into()
}
fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpOutput {
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub auth: HttpAuth,
    #[serde(default)]
    pub body_type: BodyType,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub form: Vec<KeyValue>,
    /// Send one request per link (`{{link}}`, `{{index}}`) instead of one per package.
    #[serde(default)]
    pub per_link: bool,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Accept invalid TLS certificates (self-signed home servers).
    #[serde(default, skip_serializing_if = "is_default")]
    pub insecure_tls: bool,
}

impl Default for HttpOutput {
    fn default() -> Self {
        Self {
            method: default_method(),
            url: String::new(),
            headers: Vec::new(),
            auth: HttpAuth::None,
            body_type: BodyType::Json,
            body: "{\n  \"package\": {{package|json}},\n  \"links\": {{links|json}},\n  \"password\": {{password|json}},\n  \"source\": {{source|json}}\n}".into(),
            form: Vec::new(),
            per_link: false,
            timeout_secs: default_timeout(),
            insecure_tls: false,
        }
    }
}

/// Runs a shell command. Package data is passed only via environment variables
/// (`KLIKNLOAD_PACKAGE`, `KLIKNLOAD_LINKS`, …) and stdin, never spliced into the command,
/// because it comes from untrusted websites.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub working_dir: String,
    #[serde(default)]
    pub per_link: bool,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl Default for CommandOutput {
    fn default() -> Self {
        Self {
            command: "echo \"$KLIKNLOAD_PACKAGE: $KLIKNLOAD_COUNT Links\"".into(),
            working_dir: String::new(),
            per_link: false,
            timeout_secs: default_timeout(),
        }
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl Output {
    pub fn new(kind: OutputKind, enabled: bool) -> Self {
        Self {
            id: kind.type_name().to_string(),
            name: kind.default_name().to_string(),
            enabled,
            include: None,
            exclude: None,
            kind,
        }
    }

    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            self.kind.default_name()
        } else {
            &self.name
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            notifications: Notifications::All,
            clipboard_fallback: true,
            dedupe_links: true,
            outputs: vec![
                Output::new(OutputKind::Pyload(PyloadOutput::default()), false),
                Output::new(OutputKind::Clipboard(ClipboardOutput::default()), true),
            ],
            extra: Map::new(),
        }
    }
}

/// Legacy keys of pyload-clicknload / kliknload 0.1.
const LEGACY_KEYS: &[&str] = &[
    "pyloadUrl",
    "pyloadUser",
    "pyloadPW",
    "pyloadApiKey",
    "sendToPyload",
    "addToQueue",
    "copyToClipboard",
];

fn take_str(map: &mut Map<String, Value>, key: &str) -> String {
    match map.remove(key) {
        Some(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn take_bool(map: &mut Map<String, Value>, key: &str, default: bool) -> bool {
    map.remove(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

impl Config {
    /// Parses a config, migrating the legacy format.
    pub fn from_value(mut value: Value) -> Result<Self> {
        let map = value
            .as_object_mut()
            .context("config must be a JSON object")?;
        let legacy = LEGACY_KEYS.iter().any(|k| map.contains_key(*k));
        if !legacy || map.contains_key("outputs") {
            for k in LEGACY_KEYS {
                map.remove(*k);
            }
            let mut cfg: Config = serde_json::from_value(value)?;
            cfg.normalize();
            return Ok(cfg);
        }

        let url = take_str(map, "pyloadUrl");
        let pyload = PyloadOutput {
            url: url.trim_end_matches('/').to_string(),
            user: take_str(map, "pyloadUser"),
            password: take_str(map, "pyloadPW"),
            api_key: take_str(map, "pyloadApiKey"),
            destination: if take_bool(map, "addToQueue", true) {
                Destination::Queue
            } else {
                Destination::Collector
            },
            package_name: tpl_package(),
        };
        let send = take_bool(map, "sendToPyload", true) && !pyload.url.trim().is_empty();
        let copy = take_bool(map, "copyToClipboard", false);

        let mut cfg: Config = serde_json::from_value(value)?;
        cfg.outputs = vec![
            Output::new(OutputKind::Pyload(pyload), send),
            Output::new(
                OutputKind::Clipboard(ClipboardOutput::default()),
                copy || !send,
            ),
        ];
        cfg.normalize();
        Ok(cfg)
    }

    /// Fills in missing ids and names and makes ids unique.
    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        for o in &mut self.outputs {
            let base = if o.id.trim().is_empty() {
                o.kind.type_name().to_string()
            } else {
                o.id.trim().to_string()
            };
            let mut id = base.clone();
            let mut n = 2;
            while !seen.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            o.id = id;
            if o.name.trim().is_empty() {
                o.name = o.kind.default_name().to_string();
            }
        }
    }

    /// Returns human readable problems; an empty list means the config is usable.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.listen.parse::<std::net::SocketAddr>().is_err() {
            errors.push(format!(
                "Listen-Adresse „{}“ ist ungültig (z.B. 127.0.0.1:9666)",
                self.listen
            ));
        }
        for o in &self.outputs {
            let n = o.display_name();
            for (label, re) in [("Einschließen", &o.include), ("Ausschließen", &o.exclude)] {
                if let Some(re) = re.as_deref().filter(|r| !r.is_empty())
                    && let Err(e) = regex::Regex::new(re)
                {
                    errors.push(format!(
                        "{n}: Filter „{label}“ ist kein gültiger Regex: {e}"
                    ));
                }
            }
            if !o.enabled {
                continue;
            }
            match &o.kind {
                OutputKind::Pyload(p) => {
                    if p.url.trim().is_empty() {
                        errors.push(format!("{n}: pyLoad-URL fehlt"));
                    }
                    if p.api_key.trim().is_empty() && p.user.trim().is_empty() {
                        errors.push(format!("{n}: Benutzer oder API-Key fehlt"));
                    }
                }
                OutputKind::File(f) => {
                    if f.directory.trim().is_empty() {
                        errors.push(format!("{n}: Ordner fehlt"));
                    }
                    if f.filename.trim().is_empty() {
                        errors.push(format!("{n}: Dateiname fehlt"));
                    }
                }
                OutputKind::Http(h) => {
                    if h.url.trim().is_empty() {
                        errors.push(format!("{n}: URL fehlt"));
                    }
                    if reqwest::Method::from_bytes(h.method.trim().as_bytes()).is_err() {
                        errors.push(format!("{n}: HTTP-Methode „{}“ ist ungültig", h.method));
                    }
                }
                OutputKind::Command(c) => {
                    if c.command.trim().is_empty() {
                        errors.push(format!("{n}: Befehl fehlt"));
                    }
                }
                OutputKind::Clipboard(_) => {}
            }
        }
        errors
    }

    /// The effective listen address (`$KLIKNLOAD_LISTEN` wins).
    pub fn listen_addr(&self) -> String {
        std::env::var("KLIKNLOAD_LISTEN")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.listen.clone())
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let value: Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Self::from_value(value).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)? + "\n";
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
    }

    /// First enabled-or-not pyLoad output, for "open pyLoad" in the menu.
    pub fn pyload_url(&self) -> Option<String> {
        self.outputs.iter().find_map(|o| match &o.kind {
            OutputKind::Pyload(p) if !p.url.trim().is_empty() => Some(
                crate::template::expand_env(p.url.trim())
                    .trim_end_matches('/')
                    .to_string(),
            ),
            _ => None,
        })
    }
}

/// Default location: `~/Library/Application Support/kliknload/pyloadConfig.json` on macOS.
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kliknload")
        .join(CONFIG_FILE_NAME)
}

/// Resolves which config file to use:
/// explicit path > `$KLIKNLOAD_CONFIG` > `./pyloadConfig.json` (like pyload-clicknload)
/// > next to the executable > the default location.
pub fn resolve_config_path(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    if let Some(p) = std::env::var_os("KLIKNLOAD_CONFIG") {
        return PathBuf::from(p);
    }
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(CONFIG_FILE_NAME));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(CONFIG_FILE_NAME));
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(default_config_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrates_legacy_config() {
        let cfg = Config::from_value(json!({
            "pyloadUser": "u", "pyloadPW": "p", "pyloadUrl": "https://x.example/",
            "notifications": false, "custom": 42
        }))
        .unwrap();
        assert_eq!(cfg.outputs.len(), 2);
        let OutputKind::Pyload(p) = &cfg.outputs[0].kind else {
            panic!()
        };
        assert_eq!(
            (p.url.as_str(), p.user.as_str(), p.password.as_str()),
            ("https://x.example", "u", "p")
        );
        assert!(cfg.outputs[0].enabled);
        assert!(matches!(cfg.outputs[1].kind, OutputKind::Clipboard(_)));
        assert!(!cfg.outputs[1].enabled);
        assert_eq!(cfg.notifications, Notifications::Errors);
        assert_eq!(cfg.extra["custom"], 42);
        assert!(cfg.validate().is_empty(), "{:?}", cfg.validate());
    }

    #[test]
    fn roundtrips_new_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        let mut cfg = Config::default();
        cfg.outputs
            .push(Output::new(OutputKind::Http(HttpOutput::default()), false));
        cfg.outputs
            .push(Output::new(OutputKind::File(FileOutput::default()), true));
        cfg.outputs.push(Output::new(
            OutputKind::Command(CommandOutput::default()),
            false,
        ));
        cfg.outputs
            .push(Output::new(OutputKind::Http(HttpOutput::default()), false));
        cfg.normalize();
        assert_eq!(cfg.outputs[5].id, "http-2");
        cfg.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&cfg).unwrap()
        );
    }

    #[test]
    fn validation_finds_problems() {
        let mut cfg = Config::default();
        cfg.listen = "nope".into();
        cfg.outputs[0].enabled = true;
        cfg.outputs[1].include = Some("(".into());
        let errors = cfg.validate();
        assert_eq!(errors.len(), 4, "{errors:?}");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let cfg = Config::load(Path::new("/nonexistent/kliknload.json")).unwrap();
        assert!(cfg.validate().is_empty());
    }
}
