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
    /// Notification permission changed (checked periodically).
    NotificationsDenied(bool),
}

mod id {
    pub const OPEN_PYLOAD: &str = "open_pyload";
    pub const SETTINGS: &str = "settings";
    pub const OPEN_LOG: &str = "open_log";
    pub const NOTIFY_SETTINGS: &str = "notify_settings";
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
        OutputKind::Pyload(_) => t!(MenuSendTo, name = o.display_name()),
        OutputKind::Clipboard(_) => t!(MenuCopyToClipboard),
        OutputKind::File(_) if custom => t!(MenuSaveToFileNamed, name = o.name.trim()),
        OutputKind::File(_) => t!(MenuSaveToFile),
        OutputKind::Http(_) => t!(MenuHttpOutput, name = o.display_name()),
        OutputKind::Command(_) => t!(MenuCommandOutput, name = o.display_name()),
    }
}

fn record_label(rec: &Record) -> String {
    let mark = if rec.ok() { "✓" } else { "⚠" };
    t!(
        MenuRecentEntry,
        mark = mark,
        name = short(&rec.package.name, 45),
        count = rec.package.links.len()
    )
}

fn record_details(rec: &Record) -> Vec<String> {
    rec.results
        .iter()
        .map(|(name, r)| match r {
            Ok(Outcome::Done(m)) => format!("{name}: {m}"),
            Ok(Outcome::Skipped(m)) => t!(OutSkipped, name = name, reason = m),
            Err(e) => format!("{name}: {e}"),
        })
        .collect()
}

fn status_label(status: &ServerStatus) -> String {
    match status {
        ServerStatus::Starting => t!(MenuStarting),
        ServerStatus::Listening(addr) => t!(MenuListening, addr = addr),
        ServerStatus::Failed(msg) => format!("⚠ {}", short(msg, 60)),
    }
}

/// Builds the whole menu from the current state. Cheap enough to redo on every change.
fn build_menu(
    app: &App,
    status: &ServerStatus,
    notifications_denied: bool,
) -> anyhow::Result<Menu> {
    let cfg = app.config();
    let menu = Menu::new();
    menu.append(&MenuItem::new(status_label(status), false, None))?;
    if notifications_denied {
        menu.append(&MenuItem::with_id(
            id::NOTIFY_SETTINGS,
            t!(MenuNotificationsOff),
            true,
            None,
        ))?;
    }
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
            t!(MenuOpenPyload),
            true,
            None,
        ))?;
    }
    let history = app.history.lock().unwrap();
    let recent = Submenu::new(t!(MenuRecent), !history.is_empty());
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
        recent.append(&MenuItem::new(t!(MenuRecentHint), false, None))?;
    }
    menu.append(&recent)?;
    menu.append(&PredefinedMenuItem::separator())?;

    let settings_key = Accelerator::new(Modifiers::META, Code::Comma);
    menu.append(&MenuItem::with_id(
        id::SETTINGS,
        t!(MenuSettings),
        true,
        Some(settings_key),
    ))?;
    menu.append(&MenuItem::with_id(
        id::OPEN_LOG,
        t!(MenuShowLog),
        true,
        None,
    ))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(id::QUIT, t!(MenuQuit), true, None))?;
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
                // The window uses native .lproj localization; pick kliknload's language.
                .arg("-AppleLanguages")
                .arg(format!("({})", crate::i18n::current().code()))
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
        id::NOTIFY_SETTINGS => crate::notify::open_settings(),
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
                    platform::notify("kliknload", &t!(NotifySaveFailed, error = format!("{e:#}")));
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
                            &t!(NotifyLinksCopied, count = links.len()),
                        ),
                        Err(e) => error!("copy failed: {e:#}"),
                    }
                }
            }
        }
    }
}

fn create_tray() -> anyhow::Result<TrayIcon> {
    #[allow(unused_mut)]
    let mut rgba = icon::tray_rgba()?;
    let builder = TrayIconBuilder::new().with_tooltip("kliknload – Click'n'Load");
    // macOS tints template images for light and dark menu bars itself.
    #[cfg(target_os = "macos")]
    let builder =
        builder.with_icon_templated(Icon::from_rgba(rgba, icon::TRAY_SIZE, icon::TRAY_SIZE)?);
    // Elsewhere the taskbar is usually dark: draw the silhouette in white.
    #[cfg(not(target_os = "macos"))]
    let builder = {
        for px in rgba.chunks_mut(4) {
            px[..3].fill(255);
        }
        builder.with_icon(Icon::from_rgba(rgba, icon::TRAY_SIZE, icon::TRAY_SIZE)?)
    };
    Ok(builder.build()?)
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

    // macOS never asks again once notifications were turned off; watch for that and
    // offer a shortcut to the right System Settings page.
    let permission_proxy = proxy.clone();
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(if last.is_none() {
                3
            } else {
                20
            }));
            let denied = crate::notify::permission() == crate::notify::Permission::Denied;
            if last != Some(denied) {
                last = Some(denied);
                if permission_proxy
                    .send_event(UserEvent::NotificationsDenied(denied))
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    let app = make_app(proxy);
    let mut notifications_denied = false;
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
            Event::UserEvent(UserEvent::NotificationsDenied(denied)) => {
                notifications_denied = denied;
                rebuild = true;
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                handle_menu(&app, &e, control_flow);
                rebuild = true;
            }
            _ => {}
        }
        if rebuild && let Some(tray) = &tray {
            match build_menu(&app, &status, notifications_denied) {
                Ok(menu) => tray.set_menu(Some(Box::new(menu))),
                Err(e) => error!("menu: {e:#}"),
            }
        }
    })
}
