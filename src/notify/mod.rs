//! Native desktop notifications without third-party crates:
//!
//! - macOS: Notification Center via the UserNotifications framework (own Objective-C
//!   runtime bindings); falls back to `osascript` when not running from an app bundle.
//! - Linux/BSD: `org.freedesktop.Notifications` over a minimal own D-Bus client.
//! - Windows: WinRT toast notifications, driven through the built-in PowerShell.
//!
//! Notifications are best effort: failures are logged, never returned.

#[cfg(all(unix, not(target_os = "macos")))]
mod dbus;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info};

/// Prepares notifications. On macOS this registers the delegate and asks for permission,
/// so it should run once on the main thread at startup of the menu bar app.
pub fn init() {
    #[cfg(target_os = "macos")]
    macos::init();
}

/// Only the first failure is logged at info level, the rest at debug (e.g. in Docker
/// there is no notification service at all).
static FAILED_ONCE: AtomicBool = AtomicBool::new(false);

fn report_failure(err: &dyn std::fmt::Display) {
    if FAILED_ONCE.swap(true, Ordering::Relaxed) {
        debug!("notification failed: {err}");
    } else {
        info!("desktop notifications unavailable: {err}");
    }
}

/// Shows a desktop notification and waits for the system to accept it.
pub fn send(title: &str, message: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return macos::notify(title, message);
    #[cfg(all(unix, not(target_os = "macos")))]
    return dbus::notify("kliknload", title, message);
    #[cfg(windows)]
    return windows::notify(title, message);
}

/// Shows a desktop notification. Never blocks the caller.
pub fn notify(title: &str, message: &str) {
    debug!("[notification] {title}: {message}");
    let (title, message) = (title.to_string(), message.to_string());
    std::thread::spawn(move || {
        if let Err(e) = send(&title, &message) {
            report_failure(&e);
        }
    });
}
