//! Tiny template language for output settings: `{{ variable | filter | filter }}`.
//!
//! Package names and links come from arbitrary websites, so every value is escaped for the
//! context it ends up in (URL, JSON string, header, filename) unless a format filter
//! (`json`, `url`, `raw`) was applied explicitly.
//!
//! Environment variables can be referenced in config values as `${NAME}` (see [`expand_env`]).

use anyhow::{Result, bail};
use regex::Regex;
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    List(Vec<String>),
}

impl Value {
    fn joined(&self, sep: &str) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::List(l) => l.join(sep),
        }
    }
}

pub type Vars = BTreeMap<&'static str, Value>;

/// Where the rendered text is used, decides the automatic escaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    /// Plain text (file contents, text bodies, form values).
    None,
    /// Inside a URL: percent-encoded.
    Url,
    /// Inside a JSON string literal: escaped without surrounding quotes.
    JsonString,
    /// HTTP header value: no line breaks.
    Header,
    /// A single file name: no path separators or other unsafe characters.
    Filename,
}

/// Documented for the settings UI: (name, description).
pub const VARIABLES: &[(&str, &str)] = &[
    ("package", "Paketname"),
    ("links", "Alle Links (Standard: eine Zeile pro Link)"),
    ("link", "Aktueller Link (bei „ein Request pro Link“)"),
    ("index", "Nummer des aktuellen Links, ab 1"),
    ("count", "Anzahl Links"),
    ("password", "Archiv-Passwort"),
    ("source", "Quellseite"),
    ("host", "Hostname der Quellseite"),
    ("date", "Datum JJJJ-MM-TT"),
    ("time", "Uhrzeit HH-MM-SS"),
    ("datetime", "Zeitstempel ISO 8601"),
    ("timestamp", "Unix-Zeit in Sekunden"),
];

pub const FILTERS: &[(&str, &str)] = &[
    ("json", "als JSON (Liste → Array, Text → \"String\")"),
    ("url", "URL-kodiert"),
    ("raw", "ohne automatisches Escaping"),
    ("lines", "Liste zeilenweise verbinden"),
    ("comma", "Liste mit \", \" verbinden"),
    ("space", "Liste mit Leerzeichen verbinden"),
    ("first", "erstes Element einer Liste"),
    ("safe", "für Dateinamen bereinigen"),
    ("lower", "Kleinbuchstaben"),
    ("upper", "Großbuchstaben"),
    ("trim", "Leerraum entfernen"),
];

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([^}]*?)\s*\}\}").unwrap());
static ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap());

pub fn url_encode(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

pub fn safe_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    let cleaned: String = cleaned.chars().take(180).collect();
    if cleaned.is_empty() {
        "_".to_string()
    } else {
        cleaned
    }
}

fn json_string_content(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap_or_default();
    quoted[1..quoted.len() - 1].to_string()
}

fn escape(s: &str, mode: Escape) -> String {
    match mode {
        Escape::None => s.to_string(),
        Escape::Url => url_encode(s),
        Escape::JsonString => json_string_content(s),
        Escape::Header => s.replace(['\r', '\n'], " "),
        Escape::Filename => safe_filename(s),
    }
}

fn apply_filters(name: &str, value: &Value, filters: &[&str], mode: Escape) -> Result<String> {
    let mut v = value.clone();
    let mut formatted = false;
    for f in filters {
        v = match (*f, &v) {
            ("json", Value::Str(s)) => {
                formatted = true;
                Value::Str(serde_json::to_string(s)?)
            }
            ("json", Value::List(l)) => {
                formatted = true;
                Value::Str(serde_json::to_string(l)?)
            }
            ("url", _) => {
                formatted = true;
                Value::Str(url_encode(&v.joined("\n")))
            }
            ("raw", _) => {
                formatted = true;
                v
            }
            ("lines", _) => Value::Str(v.joined("\n")),
            ("comma", _) => Value::Str(v.joined(", ")),
            ("space", _) => Value::Str(v.joined(" ")),
            ("first", Value::List(l)) => Value::Str(l.first().cloned().unwrap_or_default()),
            ("first", Value::Str(_)) => v,
            ("safe", _) => Value::Str(safe_filename(&v.joined(" "))),
            ("lower", _) => Value::Str(v.joined("\n").to_lowercase()),
            ("upper", _) => Value::Str(v.joined("\n").to_uppercase()),
            ("trim", _) => Value::Str(v.joined("\n").trim().to_string()),
            (other, _) => bail!("unbekannter Filter „{other}“ bei {{{{{name}}}}}"),
        };
    }
    let text = v.joined("\n");
    Ok(if formatted { text } else { escape(&text, mode) })
}

/// Renders `template` with `vars`, escaping values for `mode`.
pub fn render(template: &str, vars: &Vars, mode: Escape) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut last = 0;
    for cap in TAG.captures_iter(template) {
        let whole = cap.get(0).unwrap();
        out.push_str(&template[last..whole.start()]);
        last = whole.end();

        let mut parts = cap[1].split('|').map(str::trim);
        let name = parts.next().unwrap_or("");
        let filters: Vec<&str> = parts.filter(|p| !p.is_empty()).collect();
        let Some(value) = vars.get(name) else {
            bail!("unbekannte Variable {{{{{name}}}}}");
        };
        out.push_str(&apply_filters(name, value, &filters, mode)?);
    }
    out.push_str(&template[last..]);
    Ok(out)
}

/// Replaces `${NAME}` with the environment variable `NAME` (empty if unset).
/// Lets Docker users keep secrets out of the config file.
pub fn expand_env(s: &str) -> String {
    if !s.contains("${") {
        return s.to_string();
    }
    ENV.replace_all(s, |c: &regex::Captures| {
        std::env::var(&c[1]).unwrap_or_default()
    })
    .into_owned()
}

/// Like [`expand_env`], also expanding a leading `~/` to the home directory.
pub fn expand_path(s: &str) -> std::path::PathBuf {
    let s = expand_env(s);
    match (s.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => std::path::PathBuf::from(s),
    }
}

/// Validates that a rendered JSON body is valid JSON.
pub fn check_json(body: &str) -> Result<Json> {
    serde_json::from_str(body).map_err(|e| anyhow::anyhow!("Body ist kein gültiges JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        let mut v = Vars::new();
        v.insert("package", Value::Str("My \"Pkg\" / 1".into()));
        v.insert(
            "links",
            Value::List(vec!["http://a/1".into(), "http://a/2?x=1&y".into()]),
        );
        v.insert("password", Value::Str("p&w".into()));
        v
    }

    #[test]
    fn escapes_per_context() {
        let v = vars();
        assert_eq!(
            render("{{package}}", &v, Escape::None).unwrap(),
            "My \"Pkg\" / 1"
        );
        assert_eq!(
            render("q={{ package }}", &v, Escape::Url).unwrap(),
            "q=My%20%22Pkg%22%20%2F%201"
        );
        assert_eq!(
            render("\"{{package}}\"", &v, Escape::JsonString).unwrap(),
            r#""My \"Pkg\" / 1""#
        );
        assert_eq!(
            render("{{package}}.txt", &v, Escape::Filename).unwrap(),
            "My _Pkg_ _ 1.txt"
        );
        assert_eq!(
            render("{{links}}", &v, Escape::None).unwrap(),
            "http://a/1\nhttp://a/2?x=1&y"
        );
    }

    #[test]
    fn filters() {
        let v = vars();
        assert_eq!(
            render("{{links|json}}", &v, Escape::JsonString).unwrap(),
            r#"["http://a/1","http://a/2?x=1&y"]"#
        );
        assert_eq!(
            render("{{links|comma}}", &v, Escape::None).unwrap(),
            "http://a/1, http://a/2?x=1&y"
        );
        assert_eq!(
            render("{{links|first|upper}}", &v, Escape::None).unwrap(),
            "HTTP://A/1"
        );
        assert_eq!(render("{{password|raw}}", &v, Escape::Url).unwrap(), "p&w");
        assert!(render("{{nope}}", &v, Escape::None).is_err());
        assert!(render("{{package|nope}}", &v, Escape::None).is_err());
    }

    #[test]
    fn env_expansion() {
        // SAFETY: test-only, single variable not used elsewhere.
        unsafe { std::env::set_var("KLIKNLOAD_TEST_SECRET", "s3cret") };
        assert_eq!(expand_env("a${KLIKNLOAD_TEST_SECRET}b"), "as3cretb");
        assert_eq!(expand_env("${KLIKNLOAD_UNSET_VAR}"), "");
        assert_eq!(expand_env("plain {{package}}"), "plain {{package}}");
    }
}
