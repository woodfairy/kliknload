//! Menu bar (tray) UI. Settings live in a native SwiftUI window (`settings/`),
//! the menu only has quick toggles.

use crate::app::{App, AppEvent, Record, ServerStatus};
use crate::config::{Output, OutputKind};
use crate::outputs::Outcome;
use crate::{icon, platform};
use std::path::PathBuf;
use std::sync::Arc;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tracing::{error, warn};
use tray_icon::menu::accelerator::{Accelerator, Code, Modifiers};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[derive(Debug)]
pub enum UserEvent {
    Menu(MenuEvent),
    App(AppEvent),
}

mod id {
    pub const OPEN_PYLOAD: &str = "open_pyload";
    pub const SETTINGS: &str = "settings";
    pub const OPEN_LOG: &str = "open_log";
    pub const QUIT: &str = "quit";
    pub const OUTPUT_PREFIX: &str = "output:";
    pub const HISTORY_PREFIX: &str = "history:";
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

fn output_label(o: &Output) -> String {
    let custom = o.name.trim() != o.kind.default_name() && !o.name.trim().is_empty();
    match &o.kind {
        OutputKind::Pyload(_) => format!("An {} senden", o.display_name()),
        OutputKind::Clipboard(_) => "In Zwischenablage kopieren".into(),
        OutputKind::File(_) if custom => format!("In Datei speichern ({})", o.name.trim()),
        OutputKind::File(_) => "In Datei speichern".into(),
        OutputKind::Http(_) => format!("{} (HTTP)", o.display_name()),
        OutputKind::Command(_) => format!("{} (Befehl)", o.display_name()),
    }
}

fn record_label(rec: &Record) -> String {
    let mark = if rec.ok() { "✓" } else { "⚠" };
    format!(
        "{mark} {} ({} Links)",
        short(&rec.package.name, 45),
        rec.package.links.len()
    )
}

fn record_details(rec: &Record) -> Vec<String> {
    rec.results
        .iter()
        .map(|(name, r)| match r {
            Ok(Outcome::Done(m)) => format!("{name}: {m}"),
            Ok(Outcome::Skipped(m)) => format!("{name}: übersprungen ({m})"),
            Err(e) => format!("{name}: {e}"),
        })
        .collect()
}

fn status_label(status: &ServerStatus) -> String {
    match status {
        ServerStatus::Starting => "Startet…".to_string(),
        ServerStatus::Listening(addr) => format!("● Bereit auf {addr}"),
        ServerStatus::Failed(msg) => format!("⚠ {}", short(msg, 60)),
    }
}

/// Builds the whole menu from the current state. Cheap enough to redo on every change.
fn build_menu(app: &App, status: &ServerStatus) -> anyhow::Result<Menu> {
    let cfg = app.config();
    let menu = Menu::new();
    menu.append(&MenuItem::new(status_label(status), false, None))?;
    menu.append(&PredefinedMenuItem::separator())?;

    for o in &cfg.outputs {
        let item = CheckMenuItem::with_id(
            format!("{}{}", id::OUTPUT_PREFIX, o.id),
            output_label(o),
            true,
            o.enabled,
            None,
        );
        menu.append(&item)?;
    }
    menu.append(&PredefinedMenuItem::separator())?;

    if cfg.pyload_url().is_some() {
        menu.append(&MenuItem::with_id(
            id::OPEN_PYLOAD,
            "pyLoad öffnen",
            true,
            None,
        ))?;
    }
    let history = app.history.lock().unwrap();
    let recent = Submenu::new("Letzte Pakete", !history.is_empty());
    for (i, rec) in history.iter().enumerate() {
        recent.append(&MenuItem::with_id(
            format!("{}{i}", id::HISTORY_PREFIX),
            record_label(rec),
            true,
            None,
        ))?;
        if !rec.ok() {
            for line in record_details(rec) {
                recent.append(&MenuItem::new(
                    format!("    {}", short(&line, 70)),
                    false,
                    None,
                ))?;
            }
        }
    }
    if !history.is_empty() {
        recent.append(&PredefinedMenuItem::separator())?;
        recent.append(&MenuItem::new("Klick kopiert die Links", false, None))?;
    }
    menu.append(&recent)?;
    menu.append(&PredefinedMenuItem::separator())?;

    let settings_key = Accelerator::new(Modifiers::META, Code::Comma);
    menu.append(&MenuItem::with_id(
        id::SETTINGS,
        "Einstellungen…",
        true,
        Some(settings_key),
    ))?;
    menu.append(&MenuItem::with_id(id::OPEN_LOG, "Log anzeigen", true, None))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(
        id::QUIT,
        "kliknload beenden",
        true,
        None,
    ))?;
    Ok(menu)
}

/// `kliknload.app/Contents/Helpers/kliknload Settings.app`, next to our executable.
fn settings_app() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KLIKNLOAD_SETTINGS_APP") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let app = exe
        .parent()?
        .parent()?
        .join("Helpers/kliknload Settings.app");
    app.exists().then_some(app)
}

fn open_settings(app: &App) {
    let exe = std::env::current_exe().unwrap_or_default();
    match settings_app() {
        Some(settings) => {
            let result = std::process::Command::new("open")
                .arg(&settings)
                .arg("--args")
                .arg("--config")
                .arg(&app.config_path)
                .arg("--kliknload")
                .arg(&exe)
                .spawn();
            if let Err(e) = result {
                error!("could not open settings: {e}");
            }
        }
        None => {
            warn!(
                "settings app not found (not running from the app bundle?), opening the config file"
            );
            if !app.config_path.exists() {
                let _ = app.config().save(&app.config_path);
            }
            platform::open(&app.config_path.display().to_string());
        }
    }
}

fn handle_menu(app: &Arc<App>, event: &MenuEvent, control_flow: &mut ControlFlow) {
    match event.id.0.as_str() {
        id::OPEN_PYLOAD => {
            if let Some(url) = app.config().pyload_url() {
                platform::open(&url);
            }
        }
        id::SETTINGS => open_settings(app),
        id::OPEN_LOG => platform::open(&platform::log_path().display().to_string()),
        id::QUIT => *control_flow = ControlFlow::Exit,
        other => {
            if let Some(output_id) = other.strip_prefix(id::OUTPUT_PREFIX) {
                let result = app.update_config(|c| {
                    if let Some(o) = c.outputs.iter_mut().find(|o| o.id == output_id) {
                        o.enabled = !o.enabled;
                    }
                });
                if let Err(e) = result {
                    error!("saving config failed: {e:#}");
                    platform::notify("kliknload", &format!("Speichern fehlgeschlagen: {e:#}"));
                }
            } else if let Some(idx) = other
                .strip_prefix(id::HISTORY_PREFIX)
                .and_then(|i| i.parse::<usize>().ok())
            {
                let links = app
                    .history
                    .lock()
                    .unwrap()
                    .get(idx)
                    .map(|r| r.package.links.clone());
                if let Some(links) = links {
                    match platform::copy_to_clipboard(&links.join("\n")) {
                        Ok(()) => platform::notify(
                            "kliknload",
                            &format!("{} Link(s) kopiert", links.len()),
                        ),
                        Err(e) => error!("copy failed: {e:#}"),
                    }
                }
            }
        }
    }
}

fn create_tray() -> anyhow::Result<TrayIcon> {
    let icon = Icon::from_rgba(icon::tray_rgba()?, icon::TRAY_SIZE, icon::TRAY_SIZE)?;
    Ok(TrayIconBuilder::new()
        .with_icon_templated(icon)
        .with_tooltip("kliknload – Click'n'Load")
        .build()?)
}

/// Runs the menu bar UI on the main thread. Never returns.
pub fn run(make_app: impl FnOnce(EventLoopProxy<UserEvent>) -> Arc<App>) -> ! {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }

    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));

    let app = make_app(proxy);
    let mut tray: Option<TrayIcon> = None;
    let mut status = ServerStatus::Starting;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let mut rebuild = false;
        match event {
            Event::NewEvents(StartCause::Init) => match create_tray() {
                Ok(t) => {
                    tray = Some(t);
                    rebuild = true;
                    // Registers for Notification Center and asks for permission once.
                    crate::notify::init();
                }
                Err(e) => {
                    error!("could not create menu bar icon: {e:#}");
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::UserEvent(UserEvent::App(AppEvent::Server(s))) => {
                status = s;
                rebuild = true;
            }
            Event::UserEvent(UserEvent::App(AppEvent::Handled | AppEvent::ConfigChanged)) => {
                rebuild = true
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                handle_menu(&app, &e, control_flow);
                rebuild = true;
            }
            _ => {}
        }
        if rebuild && let Some(tray) = &tray {
            match build_menu(&app, &status) {
                Ok(menu) => tray.set_menu(Some(Box::new(menu))),
                Err(e) => error!("menu: {e:#}"),
            }
        }
    })
}
