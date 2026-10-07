//! Translations for everything the user sees (menu, notifications, errors, previews).
//! Logs stay English.
//!
//! The catalog is checked at compile time: every message has every language, and a
//! test makes sure all translations use the same `{placeholders}`.
//!
//! ```ignore
//! t!(PyloadLoginFailed, user = "bob")
//! ```

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::De,
        _ => Lang::En,
    }
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    fn from_code(code: &str) -> Option<Lang> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "de" => Some(Lang::De),
            _ => None,
        }
    }

    /// `en`, `de` or `system`; anything unknown falls back to English.
    pub fn from_setting(setting: &str) -> Lang {
        match setting.trim() {
            "system" | "auto" => system_language(),
            other => Lang::from_code(other).unwrap_or(Lang::En),
        }
    }
}

/// The user's UI language, English if it is not one we translate to.
pub fn system_language() -> Lang {
    #[cfg(target_os = "macos")]
    {
        // First entry of the preferred languages, e.g. "de-DE".
        if let Ok(out) = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(first) = text
                .split('"')
                .nth(1)
                .or_else(|| text.lines().nth(1).map(str::trim))
                && let Some(lang) =
                    Lang::from_code(first.trim_matches(|c: char| !c.is_alphanumeric()))
            {
                return lang;
            }
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        // Primary language id: 0x07 German, 0x09 English.
        let primary = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
        return if primary == 0x07 { Lang::De } else { Lang::En };
    }
    #[allow(unreachable_code)]
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .and_then(|v| Lang::from_code(&v))
        .unwrap_or(Lang::En)
}

macro_rules! catalog {
    ($( $key:ident { en: $en:literal, de: $de:literal $(,)? } )*) => {
        /// Some messages are only used on some platforms or builds.
        #[allow(dead_code)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Msg { $($key),* }

        impl Msg {
            #[cfg(test)]
            pub const ALL: &'static [Msg] = &[$(Msg::$key),*];

            pub fn text(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $( (Msg::$key, Lang::En) => $en, (Msg::$key, Lang::De) => $de, )*
                }
            }
        }
    };
}

/// Translated message with `{name}` placeholders filled in.
macro_rules! t {
    ($key:ident) => {
        $crate::i18n::Msg::$key.text($crate::i18n::current()).to_string()
    };
    ($key:ident, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut text = $crate::i18n::Msg::$key.text($crate::i18n::current()).to_string();
        $( text = text.replace(concat!("{", stringify!($name), "}"), &$value.to_string()); )+
        text
    }};
}

catalog! {
    // Output types
    TypePyload { en: "pyLoad", de: "pyLoad" }
    TypeClipboard { en: "Clipboard", de: "Zwischenablage" }
    TypeFile { en: "File", de: "Datei" }
    TypeHttp { en: "HTTP request", de: "HTTP-Request" }
    TypeCommand { en: "Command", de: "Befehl" }

    // Menu bar
    MenuStarting { en: "Starting…", de: "Startet…" }
    MenuListening { en: "● Ready on {addr}", de: "● Bereit auf {addr}" }
    MenuSendTo { en: "Send to {name}", de: "An {name} senden" }
    MenuCopyToClipboard { en: "Copy to clipboard", de: "In Zwischenablage kopieren" }
    MenuSaveToFile { en: "Save to file", de: "In Datei speichern" }
    MenuSaveToFileNamed { en: "Save to file ({name})", de: "In Datei speichern ({name})" }
    MenuHttpOutput { en: "{name} (HTTP)", de: "{name} (HTTP)" }
    MenuCommandOutput { en: "{name} (command)", de: "{name} (Befehl)" }
    MenuOpenPyload { en: "Open pyLoad", de: "pyLoad öffnen" }
    MenuRecent { en: "Recent packages", de: "Letzte Pakete" }
    MenuRecentEntry { en: "{mark} {name} ({count} links)", de: "{mark} {name} ({count} Links)" }
    MenuRecentHint { en: "Click copies the links", de: "Klick kopiert die Links" }
    MenuSettings { en: "Settings…", de: "Einstellungen…" }
    MenuShowLog { en: "Show log", de: "Log anzeigen" }
    MenuQuit { en: "Quit kliknload", de: "kliknload beenden" }
    MenuNotificationsOff { en: "⚠ Notifications are off – enable…", de: "⚠ Mitteilungen sind aus – aktivieren…" }

    // Notifications
    NotifyPackage { en: "{count} link(s): {package}", de: "{count} Link(s): {package}" }
    NotifyDelivered { en: "→ {outputs}", de: "→ {outputs}" }
    NotifyNoOutput { en: "No output enabled", de: "Keine Ausgabe aktiv" }
    NotifyErrorTitle { en: "kliknload – error", de: "kliknload – Fehler" }
    NotifyFallbackHint { en: "Links copied to the clipboard", de: "Links in die Zwischenablage kopiert" }
    NotifyLinksCopied { en: "{count} link(s) copied", de: "{count} Link(s) kopiert" }
    NotifyCnlError { en: "Click'n'Load error", de: "Click'n'Load-Fehler" }
    NotifyConfigInvalid { en: "Invalid configuration: {error}", de: "Konfiguration fehlerhaft: {error}" }
    NotifySaveFailed { en: "Saving failed: {error}", de: "Speichern fehlgeschlagen: {error}" }
    NotifyPortBusy { en: "{msg}. Is JDownloader or another Click'n'Load tool running?", de: "{msg}. Läuft JDownloader oder ein anderes Click'n'Load-Tool?" }
    NotifyTest { en: "Notifications work", de: "Benachrichtigungen funktionieren" }
    NotifyPermissionPrompt { en: "Please answer the macOS notification prompt …", de: "Bitte die Mitteilungs-Abfrage von macOS beantworten …" }
    NotifyPermissionDenied { en: "Notifications are turned off for kliknload (System Settings → Notifications)", de: "Mitteilungen sind für kliknload ausgeschaltet (Systemeinstellungen → Mitteilungen)" }
    NotifyPermissionUnanswered { en: "The notification prompt was not answered", de: "Die Mitteilungs-Abfrage wurde nicht beantwortet" }

    // Server
    ServerInvalidListen { en: "Invalid listen address “{addr}”", de: "Ungültige Listen-Adresse „{addr}“" }
    ServerUnavailable { en: "{addr} not available: {error}", de: "{addr} nicht verfügbar: {error}" }

    // Config validation (prefix "{name}: " is used by the settings window)
    ValidListen { en: "Listen address “{addr}” is invalid (e.g. 127.0.0.1:9666)", de: "Listen-Adresse „{addr}“ ist ungültig (z.B. 127.0.0.1:9666)" }
    ValidInclude { en: "Include", de: "Einschließen" }
    ValidExclude { en: "Exclude", de: "Ausschließen" }
    ValidRegex { en: "{name}: filter “{label}” is not a valid regex: {error}", de: "{name}: Filter „{label}“ ist kein gültiger Regex: {error}" }
    ValidPyloadUrl { en: "{name}: pyLoad URL is missing", de: "{name}: pyLoad-URL fehlt" }
    ValidPyloadAuth { en: "{name}: user or API key is missing", de: "{name}: Benutzer oder API-Key fehlt" }
    ValidDirectory { en: "{name}: folder is missing", de: "{name}: Ordner fehlt" }
    ValidFilename { en: "{name}: file name is missing", de: "{name}: Dateiname fehlt" }
    ValidUrl { en: "{name}: URL is missing", de: "{name}: URL fehlt" }
    ValidMethod { en: "{name}: HTTP method “{method}” is invalid", de: "{name}: HTTP-Methode „{method}“ ist ungültig" }
    ValidCommand { en: "{name}: command is missing", de: "{name}: Befehl fehlt" }

    // Templates
    TplUnknownFilter { en: "unknown filter “{filter}” in {{{name}}}", de: "unbekannter Filter „{filter}“ bei {{{name}}}" }
    TplUnknownVariable { en: "unknown variable {{{name}}}", de: "unbekannte Variable {{{name}}}" }
    TplInvalidJson { en: "body is not valid JSON: {error}", de: "Body ist kein gültiges JSON: {error}" }
    VarPackage { en: "Package name", de: "Paketname" }
    VarLinks { en: "All links (default: one per line)", de: "Alle Links (Standard: einer pro Zeile)" }
    VarLink { en: "Current link (with “one request per link”)", de: "Aktueller Link (bei „ein Request pro Link“)" }
    VarIndex { en: "Number of the current link, from 1", de: "Nummer des aktuellen Links, ab 1" }
    VarCount { en: "Number of links", de: "Anzahl Links" }
    VarPassword { en: "Archive password", de: "Archiv-Passwort" }
    VarSource { en: "Source page", de: "Quellseite" }
    VarHost { en: "Host name of the source page", de: "Hostname der Quellseite" }
    VarDate { en: "Date YYYY-MM-DD", de: "Datum JJJJ-MM-TT" }
    VarTime { en: "Time HH-MM-SS", de: "Uhrzeit HH-MM-SS" }
    VarDatetime { en: "Timestamp ISO 8601", de: "Zeitstempel ISO 8601" }
    VarTimestamp { en: "Unix time in seconds", de: "Unix-Zeit in Sekunden" }
    FilterJson { en: "as JSON (list → array, text → \"string\")", de: "als JSON (Liste → Array, Text → \"String\")" }
    FilterUrl { en: "URL-encoded", de: "URL-kodiert" }
    FilterRaw { en: "without automatic escaping", de: "ohne automatisches Escaping" }
    FilterLines { en: "join a list line by line", de: "Liste zeilenweise verbinden" }
    FilterComma { en: "join a list with \", \"", de: "Liste mit \", \" verbinden" }
    FilterSpace { en: "join a list with spaces", de: "Liste mit Leerzeichen verbinden" }
    FilterFirst { en: "first element of a list", de: "erstes Element einer Liste" }
    FilterSafe { en: "clean up for file names", de: "für Dateinamen bereinigen" }
    FilterLower { en: "lower case", de: "Kleinbuchstaben" }
    FilterUpper { en: "upper case", de: "Großbuchstaben" }
    FilterTrim { en: "remove surrounding whitespace", de: "Leerraum entfernen" }

    // Outputs: common
    OutNoMatchingLinks { en: "no matching links", de: "keine passenden Links" }
    OutSkipped { en: "{name}: skipped ({reason})", de: "{name}: übersprungen ({reason})" }
    OutAllFiltered { en: "All links are filtered out by this output's filters", de: "Alle Links werden von den Filtern dieser Ausgabe aussortiert" }
    OutClipboardUnavailable { en: "The clipboard is not available in this build (headless/Docker)", de: "Die Zwischenablage ist in dieser Version (headless/Docker) nicht verfügbar" }
    OutClipboardPreview { en: "To the clipboard:", de: "In die Zwischenablage:" }

    // pyLoad
    PyloadNoUrl { en: "pyLoad URL is not set up", de: "pyLoad-URL ist nicht eingerichtet" }
    PyloadNoAuth { en: "pyLoad user or API key is missing", de: "pyLoad-Benutzer oder API-Key fehlt" }
    PyloadConnect { en: "Connection to {url} failed", de: "Verbindung zu {url} fehlgeschlagen" }
    PyloadLoginFailed { en: "pyLoad login for “{user}” failed (wrong user or password?)", de: "pyLoad-Login für „{user}“ fehlgeschlagen (Benutzer oder Passwort falsch?)" }
    PyloadLoginError { en: "pyLoad login failed ({error})", de: "pyLoad-Login fehlgeschlagen ({error})" }
    PyloadApiError { en: "pyLoad API {func} failed: {error}", de: "pyLoad-API {func} fehlgeschlagen: {error}" }
    PyloadQueue { en: "queue", de: "Warteschlange" }
    PyloadCollector { en: "link collector", de: "Linksammler" }
    PyloadAdded { en: "Package “{name}” (ID {id}) added to the {target}", de: "Paket „{name}“ (ID {id}) in {target}" }
    PyloadAddedNoId { en: "Package “{name}” added to the {target}", de: "Paket „{name}“ in {target}" }
    PyloadTestOk { en: "Login successful, pyLoad {version}", de: "Login erfolgreich, pyLoad {version}" }
    PyloadPreview { en: "Target: {url}\nLogin: {auth}\nPackage: {name}\nInto: {target}\n", de: "Ziel: {url}\nAnmeldung: {auth}\nPaket: {name}\nIn: {target}\n" }
    PyloadAuthUser { en: "user/password", de: "Benutzer/Passwort" }
    PyloadAuthKey { en: "API key", de: "API-Key" }
    PreviewPassword { en: "Password: {password}\n", de: "Passwort: {password}\n" }
    PreviewLinks { en: "{count} links:", de: "{count} Links:" }

    // File
    FileInvalidName { en: "invalid file name “{name}”", de: "ungültiger Dateiname „{name}“" }
    FileEmptyName { en: "file name is empty", de: "Dateiname ist leer" }
    FileCreateDir { en: "creating folder {path}", de: "Ordner {path} anlegen" }
    FileOpen { en: "opening {path}", de: "{path} öffnen" }
    FileWrite { en: "writing {path}", de: "{path} schreiben" }
    FileAppended { en: "appended to {path}", de: "an {path} angehängt" }
    FileExists { en: "{path} already exists, skipped", de: "{path} existiert bereits, übersprungen" }
    FileSaved { en: "saved: {path}", de: "gespeichert: {path}" }
    FileModeNumber { en: "new file (number on conflict)", de: "neue Datei (bei Konflikt nummerieren)" }
    FileModeOverwrite { en: "new file (overwrite on conflict)", de: "neue Datei (bei Konflikt überschreiben)" }
    FileModeSkip { en: "new file (skip on conflict)", de: "neue Datei (bei Konflikt überspringen)" }
    FileModeAppend { en: "append to file", de: "an Datei anhängen" }
    FilePreview { en: "File: {path}\nMode: {mode}", de: "Datei: {path}\nModus: {mode}" }

    // HTTP
    HttpInvalidMethod { en: "invalid HTTP method “{method}”", de: "ungültige HTTP-Methode „{method}“" }
    HttpInvalidUrl { en: "invalid URL “{url}”", de: "ungültige URL „{url}“" }
    HttpRequestOf { en: " (request {n}/{total})", de: " (Request {n}/{total})" }
    HttpSentMany { en: "{count} requests sent, last {last}", de: "{count} Requests gesendet, zuletzt {last}" }
    HttpPreviewPerLink { en: "{count} requests (one per link)", de: "{count} Requests (einer pro Link)" }
    HttpPreviewMore { en: "… and {count} more", de: "… und {count} weitere" }
    PresetWebhook { en: "JSON webhook", de: "JSON-Webhook" }
    PresetAria2 { en: "aria2 (JSON-RPC)", de: "aria2 (JSON-RPC)" }
    PresetDiscord { en: "Discord webhook", de: "Discord-Webhook" }
    PresetSlack { en: "Slack webhook", de: "Slack-Webhook" }
    PresetGotify { en: "Gotify", de: "Gotify" }
    PresetNtfy { en: "ntfy", de: "ntfy" }

    // Command
    CmdStartFailed { en: "command could not be started", de: "Befehl konnte nicht gestartet werden" }
    CmdTimeout { en: "timed out after {secs} s", de: "Zeitüberschreitung nach {secs} s" }
    CmdExit { en: "command exited with {status}: {detail}", de: "Befehl beendet mit {status}: {detail}" }
    CmdOk { en: "command succeeded", de: "Befehl erfolgreich" }
    CmdOkOutput { en: "command succeeded: {output}", de: "Befehl erfolgreich: {output}" }
    CmdOkMany { en: "{count} runs succeeded", de: "{count} Aufrufe erfolgreich" }
    CmdRunsPerLink { en: "{count} runs (one per link)", de: "{count} Aufrufe (einer pro Link)" }
    CmdRunsOne { en: "1 run", de: "1 Aufruf" }
    CmdPreview { en: "{runs}, stdin: links line by line\n\nEnvironment variables:", de: "{runs}, stdin: Links zeilenweise\n\nUmgebungsvariablen:" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let re = regex::Regex::new(r"\{([a-z_]+)\}").unwrap();
        let mut v: Vec<String> = re.captures_iter(s).map(|c| c[1].to_string()).collect();
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn translations_use_the_same_placeholders() {
        for msg in Msg::ALL {
            assert_eq!(
                placeholders(msg.text(Lang::En)),
                placeholders(msg.text(Lang::De)),
                "{msg:?}"
            );
        }
    }

    #[test]
    fn fills_placeholders() {
        // Tests run in parallel and share the global language, so only English here.
        assert_eq!(Msg::MenuSendTo.text(Lang::De), "An {name} senden");
        assert_eq!(t!(MenuSendTo, name = "pyLoad"), "Send to pyLoad");
        assert_eq!(t!(TplUnknownVariable, name = "x"), "unknown variable {{x}}");
    }

    #[test]
    fn parses_settings() {
        assert_eq!(Lang::from_setting("de"), Lang::De);
        assert_eq!(Lang::from_setting("de_DE.UTF-8"), Lang::De);
        assert_eq!(Lang::from_setting("fr"), Lang::En);
        assert_eq!(Lang::from_setting(""), Lang::En);
    }
}
