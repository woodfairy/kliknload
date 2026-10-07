//! Machine-readable subcommands used by the settings window (and usable from scripts).
//! All of them print one JSON object to stdout.
//!
//! - `kliknload config get`        current config (migrated) plus metadata
//! - `kliknload config set`        reads a config from stdin, saves it, returns problems
//! - `kliknload output preview`    reads one output from stdin, describes what it would do
//! - `kliknload output test`       reads one output from stdin and runs it with sample data
//! - `kliknload autostart on|off`  start at login (macOS)

use crate::config::{Config, Output, OutputKind};
use crate::outputs::{self, Package};
use crate::{platform, template};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

fn read_stdin_json() -> Result<Value> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    serde_json::from_str(&input).context("stdin is not valid JSON")
}

fn print(value: Value) {
    println!("{value}");
}

fn meta() -> Value {
    let presets: Vec<Value> = outputs::http_presets()
        .into_iter()
        .map(|(id, label, cfg)| json!({ "id": id, "label": label, "output": cfg }))
        .collect();
    json!({
        "variables": template::VARIABLES.iter().map(|(n, d)| json!({"name": n, "description": d})).collect::<Vec<_>>(),
        "filters": template::FILTERS.iter().map(|(n, d)| json!({"name": n, "description": d})).collect::<Vec<_>>(),
        "httpPresets": presets,
        "outputDefaults": OutputKind::all_defaults()
            .into_iter()
            .map(|k| Output::new(k, true))
            .collect::<Vec<_>>(),
    })
}

pub fn config_get(path: &Path) -> Result<()> {
    let (config, error) = match Config::load(path) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(format!("{e:#}"))),
    };
    print(json!({
        "configPath": path,
        "logPath": platform::log_path(),
        "version": env!("CARGO_PKG_VERSION"),
        "autostart": platform::autostart_enabled(),
        "loadError": error,
        "problems": config.validate(),
        "config": config,
        "meta": meta(),
    }));
    Ok(())
}

pub fn config_set(path: &Path) -> Result<()> {
    let mut config = Config::from_value(read_stdin_json()?)?;
    // Keep unknown top-level keys of the existing file.
    if let Ok(existing) = Config::load(path) {
        for (k, v) in existing.extra {
            config.extra.entry(k).or_insert(v);
        }
    }
    config.save(path)?;
    print(json!({ "ok": true, "problems": config.validate(), "config": config }));
    Ok(())
}

fn read_output() -> Result<Output> {
    let mut value = read_stdin_json()?;
    if let Some(inner) = value.get_mut("output") {
        value = inner.take();
    }
    serde_json::from_value(value).context("invalid output")
}

pub fn output_preview() -> Result<()> {
    let result = read_output().and_then(|o| outputs::preview(&o, &Package::sample()));
    print(match result {
        Ok(text) => json!({ "ok": true, "text": text }),
        Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
    });
    Ok(())
}

pub async fn output_test() -> Result<()> {
    let result = match read_output() {
        Ok(o) => outputs::test(&o).await,
        Err(e) => Err(e),
    };
    print(match result {
        Ok(text) => json!({ "ok": true, "text": text }),
        Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
    });
    Ok(())
}

pub fn autostart(enable: bool) -> Result<()> {
    platform::set_autostart(enable)?;
    print(json!({ "ok": true, "autostart": platform::autostart_enabled() }));
    Ok(())
}
