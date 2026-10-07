//! Shared application state and what happens with a received package.

use crate::config::{Config, Notifications, OutputKind};
use crate::outputs::{self, Outcome, Package};
use crate::platform;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct Record {
    pub package: Package,
    /// Per output: display name and outcome.
    pub results: Vec<(String, Result<Outcome, String>)>,
}

impl Record {
    pub fn ok(&self) -> bool {
        self.results.iter().all(|(_, r)| r.is_ok())
    }
}

#[derive(Debug, Clone)]
pub enum ServerStatus {
    Starting,
    Listening(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Server(ServerStatus),
    /// A package was handled, the history changed.
    Handled,
    /// The config file changed on disk and was reloaded.
    ConfigChanged,
}

pub const HISTORY_LEN: usize = 10;

pub struct App {
    pub config: RwLock<Config>,
    pub config_path: PathBuf,
    pub history: Mutex<Vec<Record>>,
    config_mtime: Mutex<Option<SystemTime>>,
    listener: Box<dyn Fn(AppEvent) + Send + Sync>,
}

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl App {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        listener: impl Fn(AppEvent) + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: RwLock::new(config),
            config_mtime: Mutex::new(mtime(&config_path)),
            config_path,
            history: Mutex::new(Vec::new()),
            listener: Box::new(listener),
        })
    }

    pub fn config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn emit(&self, event: AppEvent) {
        (self.listener)(event);
    }

    /// Applies `change` to the config and persists it.
    pub fn update_config(&self, change: impl FnOnce(&mut Config)) -> anyhow::Result<()> {
        let mut cfg = self.config.write().unwrap();
        change(&mut cfg);
        cfg.save(&self.config_path)?;
        *self.config_mtime.lock().unwrap() = mtime(&self.config_path);
        Ok(())
    }

    pub fn reload_config(&self) -> anyhow::Result<()> {
        let cfg = Config::load(&self.config_path)?;
        for problem in cfg.validate() {
            warn!("config: {problem}");
        }
        let listen_changed = cfg.listen != self.config.read().unwrap().listen;
        *self.config.write().unwrap() = cfg;
        if listen_changed {
            warn!("listen address changed, restart kliknload to apply it");
        }
        Ok(())
    }

    /// Reloads the config whenever the file changes (settings window, editor, …).
    pub async fn watch_config(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let current = mtime(&self.config_path);
            let changed = {
                let mut last = self.config_mtime.lock().unwrap();
                let changed = current.is_some() && current != *last;
                *last = current;
                changed
            };
            if !changed {
                continue;
            }
            match self.reload_config() {
                Ok(()) => {
                    info!("config reloaded from {}", self.config_path.display());
                    self.emit(AppEvent::ConfigChanged);
                }
                Err(e) => {
                    error!("config reload failed: {e:#}");
                    platform::notify("kliknload", &format!("Konfiguration fehlerhaft: {e:#}"));
                }
            }
        }
    }

    /// Sends a package to all enabled outputs, notifies the user and records it.
    pub async fn handle_package(&self, mut package: Package) -> Result<(), String> {
        let cfg = self.config();
        if cfg.dedupe_links {
            let mut seen = std::collections::HashSet::new();
            package.links.retain(|l| seen.insert(l.clone()));
        }
        let count = package.links.len();
        info!("received package '{}' with {count} link(s)", package.name);
        for link in &package.links {
            info!("  {link}");
        }

        let mut tasks = tokio::task::JoinSet::new();
        for (index, output) in cfg
            .outputs
            .iter()
            .filter(|o| o.enabled)
            .cloned()
            .enumerate()
        {
            let package = package.clone();
            tasks.spawn(async move {
                let result = outputs::deliver(&output, &package)
                    .await
                    .map_err(|e| format!("{e:#}"));
                (
                    index,
                    output.display_name().to_string(),
                    matches!(output.kind, OutputKind::Clipboard(_)),
                    result,
                )
            });
        }
        let mut results = tasks.join_all().await;
        results.sort_by_key(|r| r.0);

        let mut any_done = false;
        let mut clipboard_done = false;
        for (_, name, is_clipboard, result) in &results {
            match result {
                Ok(Outcome::Done(msg)) => {
                    any_done = true;
                    clipboard_done |= *is_clipboard;
                    info!("{name}: {msg}");
                }
                Ok(Outcome::Skipped(msg)) => info!("{name}: übersprungen ({msg})"),
                Err(e) => error!("{name}: {e}"),
            }
        }

        let mut fallback = false;
        if !any_done && cfg.clipboard_fallback && !clipboard_done {
            match platform::copy_to_clipboard(&package.links.join("\n")) {
                Ok(()) => fallback = true,
                Err(e) => warn!("clipboard fallback failed: {e:#}"),
            }
        }

        let failures: Vec<_> = results
            .iter()
            .filter_map(|(_, name, _, r)| r.as_ref().err().map(|e| format!("{name}: {e}")))
            .collect();
        let hint = if fallback {
            "\nLinks in Zwischenablage kopiert"
        } else {
            ""
        };
        if !failures.is_empty() {
            if cfg.notifications != Notifications::Off {
                platform::notify(
                    "kliknload – Fehler",
                    &format!("{}{hint}", failures.join("\n")),
                );
            }
        } else if cfg.notifications == Notifications::All {
            let done: Vec<_> = results
                .iter()
                .filter(|(_, _, _, r)| matches!(r, Ok(Outcome::Done(_))))
                .map(|(_, name, _, _)| name.as_str())
                .collect();
            let message = if done.is_empty() {
                format!("Keine Ausgabe aktiv{hint}")
            } else {
                format!("→ {}{hint}", done.join(", "))
            };
            platform::notify(&format!("{count} Link(s): {}", package.name), &message);
        }

        let record = Record {
            package,
            results: results
                .into_iter()
                .map(|(_, name, _, r)| (name, r))
                .collect(),
        };
        {
            let mut history = self.history.lock().unwrap();
            history.insert(0, record);
            history.truncate(HISTORY_LEN);
        }
        self.emit(AppEvent::Handled);
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}
