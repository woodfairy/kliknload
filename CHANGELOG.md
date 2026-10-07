# Changelog

## [1.0.0] - 2026-10-07

First release.

### Click'n'Load
- Receives Click'n'Load (CNL2) on `127.0.0.1:9666` like JDownloader: `/flash/addcrypted2`,
  `/flash/add`, `/jdcheck.js`, `/crossdomain.xml`
- Drop-in replacement for pyload-clicknload: its `pyloadConfig.json` is still read and
  migrated; kliknload's own file is `kliknload.json`

### Outputs
- **pyLoad**: API key, or the web login with CSRF token used by pyLoad 0.5 (where
  `/api/login` is gone), with fallback to pyLoad 0.4; sets the archive password;
  queue or link collector
- **HTTP request builder**: method, URL, headers, basic/bearer auth, JSON/form/text body,
  one request per package or per link; presets for aria2, Discord, Slack, Gotify, ntfy
- **File**: text, JSON (Lines), CSV, JDownloader crawljob or own template; new file per
  package or append; file name template with subfolders
- **Command**: shell command with package data in environment variables and on stdin
- **Clipboard**
- Include/exclude regex per output, live preview and test with sample data
- Templates (`{{package}}`, `{{links|json}}`, …) escape values for URLs, JSON, headers and
  file names automatically; `${ENV}` in config values for secrets

### App
- macOS menu bar app with checkboxes per output and recent packages
- Native SwiftUI settings window
- Native notifications on macOS, Linux (D-Bus) and Windows (toasts)
- English and German, English by default
- Start at login, config file changes are picked up live

### Distribution
- macOS disk image (Apple Silicon and Intel)
- Docker image `ghcr.io/woodfairy/kliknload` (amd64, arm64) and docker-compose example
