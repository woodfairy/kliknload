//! OS integration: notifications, clipboard, dialogs, opening things, autostart.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
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
    anyhow::bail!("Zwischenablage ist in dieser Version (headless/Docker) nicht verfügbar")
}

/// Escapes a string for use inside an AppleScript string literal.
#[cfg(target_os = "macos")]
fn applescript_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
pub fn notify(title: &str, message: &str) {
    let script = format!(
        "display notification {} with title {}",
        applescript_str(message),
        applescript_str(title)
    );
    // Fire and forget, osascript takes a moment and must not block the caller.
    if let Err(e) = Command::new("osascript").arg("-e").arg(script).spawn() {
        warn!("notification failed: {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn notify(title: &str, message: &str) {
    tracing::info!("[notification] {title}: {message}");
}

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

// ---- Start at login (macOS LaunchAgent) ----

pub const BUNDLE_ID: &str = "io.github.woodfairy.kliknload";
#[cfg(target_os = "macos")]
const AGENT_LABEL: &str = BUNDLE_ID;

#[cfg(target_os = "macos")]
fn agent_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(format!("Library/LaunchAgents/{AGENT_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
pub fn autostart_enabled() -> bool {
    agent_path().is_some_and(|p| p.exists())
}

#[cfg(target_os = "macos")]
pub fn set_autostart(enabled: bool) -> Result<()> {
    let path = agent_path().context("no home directory")?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        return Ok(());
    }
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

#[cfg(not(target_os = "macos"))]
pub fn autostart_enabled() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn set_autostart(_enabled: bool) -> Result<()> {
    anyhow::bail!("start at login is only implemented for macOS")
}
