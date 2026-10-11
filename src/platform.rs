//! OS integration: clipboard, opening things, autostart. Notifications live in `notify`.

#[cfg(target_os = "macos")]
use anyhow::Context;
use anyhow::Result;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tracing::warn;

#[cfg(feature = "gui")]
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

#[cfg(all(not(feature = "gui"), target_os = "macos"))]
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("pbcopy stdin")?
        .write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

#[cfg(all(not(feature = "gui"), not(target_os = "macos")))]
pub fn copy_to_clipboard(_text: &str) -> Result<()> {
    anyhow::bail!("{}", t!(OutClipboardUnavailable))
}

pub use crate::notify::notify;

/// Opens a URL or file with the default application.
pub fn open(target: &str) {
    #[cfg(target_os = "macos")]
    let cmd = Command::new("open").arg(target).spawn();
    #[cfg(target_os = "windows")]
    let cmd = Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let cmd = Command::new("xdg-open").arg(target).spawn();
    if let Err(e) = cmd {
        warn!("could not open {target}: {e}");
    }
}

pub fn log_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        return home.join("Library/Logs/kliknload.log");
    }
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kliknload.log")
}

// ---- Start at login (macOS) ----
//
// From the app bundle kliknload registers itself as a login item through
// SMAppService. macOS tracks the bundle itself, so the entry keeps working when the
// app is moved (e.g. from ~/Applications to /Applications) and shows up under
// System Settings → General → Login Items. Plain binaries (`cargo run`) fall back to
// a LaunchAgent with the absolute path of the executable.

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const BUNDLE_ID: &str = "io.github.woodfairy.kliknload";
#[cfg(target_os = "macos")]
const AGENT_LABEL: &str = BUNDLE_ID;

#[cfg(target_os = "macos")]
fn agent_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(format!("Library/LaunchAgents/{AGENT_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
pub fn autostart_enabled() -> bool {
    if crate::objc::bundled() {
        return login_item::enabled();
    }
    agent_path().is_some_and(|p| p.exists())
}

#[cfg(target_os = "macos")]
pub fn set_autostart(enabled: bool) -> Result<()> {
    if crate::objc::bundled() {
        // LaunchAgents written by older versions point to a fixed path and break
        // as soon as the app is moved.
        remove_launch_agent()?;
        return login_item::set(enabled);
    }
    if !enabled {
        return remove_launch_agent();
    }
    let path = agent_path().context("no home directory")?;
    let exe = std::env::current_exe()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let escape = |p: &Path| {
        p.display()
            .to_string()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        escape(&exe)
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, plist).with_context(|| format!("writing {}", path.display()))
}

#[cfg(target_os = "macos")]
fn remove_launch_agent() -> Result<()> {
    match agent_path() {
        Some(path) if path.exists() => {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Replaces a LaunchAgent from an older version with a login item. Called when the
/// menu bar app starts from its bundle.
#[cfg(target_os = "macos")]
pub fn migrate_autostart() {
    if crate::objc::bundled() && agent_path().is_some_and(|p| p.exists()) {
        match set_autostart(true) {
            Ok(()) => tracing::info!("start at login: replaced LaunchAgent with login item"),
            Err(e) => warn!("start at login: migrating LaunchAgent failed: {e:#}"),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn migrate_autostart() {}

#[cfg(target_os = "macos")]
mod login_item {
    use crate::objc::*;
    use anyhow::{Result, bail};

    #[link(name = "ServiceManagement", kind = "framework")]
    unsafe extern "C" {}

    // SMAppServiceStatus
    const ENABLED: isize = 1;
    const REQUIRES_APPROVAL: isize = 2;

    /// `[SMAppService mainAppService]`, the login item for the app bundle itself.
    fn service() -> Result<Id> {
        let class = class("SMAppService");
        if class.is_null() {
            bail!("SMAppService is not available (macOS 13 or later required)");
        }
        Ok(unsafe { msg!(class, "mainAppService"; -> Id) })
    }

    fn status() -> isize {
        service().map_or(0, |s| unsafe { msg!(s, "status"; -> isize) })
    }

    pub fn enabled() -> bool {
        matches!(status(), ENABLED | REQUIRES_APPROVAL)
    }

    pub fn set(enabled: bool) -> Result<()> {
        if enabled == self::enabled() && status() != REQUIRES_APPROVAL {
            return Ok(());
        }
        let service = service()?;
        let method = if enabled {
            "registerAndReturnError:"
        } else {
            "unregisterAndReturnError:"
        };
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let mut error: Id = std::ptr::null_mut();
            let ok = msg!(service, method, &mut error => *mut Id; -> bool);
            let text = string(if error.is_null() {
                error
            } else {
                msg!(error, "localizedDescription"; -> Id)
            });
            objc_autoreleasePoolPop(pool);
            if !ok {
                bail!("{text}");
            }
        }
        if enabled && status() == REQUIRES_APPROVAL {
            // The user switched kliknload off in System Settings before; only they
            // can switch it back on there.
            unsafe { msg!(class("SMAppService"), "openSystemSettingsLoginItems"; -> ()) };
        }
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn autostart_enabled() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn set_autostart(_enabled: bool) -> Result<()> {
    anyhow::bail!("start at login is only implemented for macOS")
}
