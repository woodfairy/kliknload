//! Windows toast notifications through the WinRT `ToastNotificationManager`,
//! driven by the PowerShell that ships with Windows.
//!
//! Title and text are passed as environment variables and XML-escaped inside the
//! script, so package names from websites can neither break the script nor the XML.

use anyhow::{Result, bail};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// PowerShell's own AppUserModelID, registered on every Windows 10/11 install,
/// so toasts are shown without installing a Start menu shortcut first.
const APP_ID: &str =
    r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null
$title = [Security.SecurityElement]::Escape($env:KLIKNLOAD_TOAST_TITLE)
$body = [Security.SecurityElement]::Escape($env:KLIKNLOAD_TOAST_BODY)
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml("<toast><visual><binding template='ToastGeneric'><text>$title</text><text>$body</text></binding></visual></toast>")
$toast = New-Object Windows.UI.Notifications.ToastNotification $xml
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($env:KLIKNLOAD_TOAST_APP).Show($toast)
"#;

pub fn notify(title: &str, message: &str) -> Result<()> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            SCRIPT,
        ])
        .env("KLIKNLOAD_TOAST_TITLE", format!("kliknload: {title}"))
        .env("KLIKNLOAD_TOAST_BODY", message)
        .env("KLIKNLOAD_TOAST_APP", APP_ID)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        bail!(
            "toast failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
