<p align="center">
  <img src="assets/logo.svg" width="112" alt="kliknload logo">
</p>

<h1 align="center">kliknload</h1>

<p align="center">
  Catches Click'n'Load links from your browser and sends them wherever you want:<br>
  pyLoad, any HTTP endpoint, files, shell commands or the clipboard.
</p>

---

kliknload listens on `http://127.0.0.1:9666` like JDownloader, decrypts Click'n'Load (CNL2)
packages and passes them to one or more **outputs**. It is a drop-in replacement for
[pyload-clicknload](https://github.com/Laberbear/pyload-clicknload): the existing
`pyloadConfig.json` keeps working and is migrated automatically.

- **macOS menu bar app** with a native settings window (SwiftUI, no web views)
- **Outputs**: pyLoad, HTTP request builder (with presets for aria2, Discord, Slack, Gotify, ntfy),
  files (txt, JSON, CSV, JDownloader crawljob, own template), shell commands, clipboard
- **Native desktop notifications** on macOS (Notification Center), Linux/BSD (freedesktop
  notifications over D-Bus) and Windows (toasts), implemented without extra libraries
- **Per-output link filters** (include/exclude regex), live preview and test button
- **Everything is in one JSON file**, so the GUI is optional; changes are picked up live
- **Headless mode** for any system (`--headless`), also as a Docker image; secrets via environment variables

## Install (macOS)

Download `kliknload-…-macos.zip` from the releases, unzip, move `kliknload.app` to
`Applications` and start it. Or build it yourself:

```sh
scripts/bundle.sh --install     # needs Rust and the Xcode command line tools
```

The menu bar icon has checkboxes for every output, recent packages and **Einstellungen…**
(⌘,) for the settings window. The app is ad-hoc signed; on first launch use right click → Open.

## Docker

```sh
cp .env.example .env                                   # fill in your credentials
mkdir -p docker/config
cp docker/pyloadConfig.example.json docker/config/pyloadConfig.json
docker compose up -d
```

Click'n'Load requests come from the browser to `127.0.0.1:9666` **of the same machine**, so
the container has to run where the browser runs. The compose file only publishes the port on
localhost; do not expose it to the network. The image has no clipboard and no notifications.

Images are published to `ghcr.io/woodfairy/kliknload` (`edge` = main, `latest` = newest release).

## Configuration

The config file is searched in this order: `--config <path>`, `$KLIKNLOAD_CONFIG`,
`./pyloadConfig.json`, next to the binary, then
`~/Library/Application Support/kliknload/pyloadConfig.json` (macOS default).

`${NAME}` anywhere in a value is replaced by the environment variable `NAME`, so credentials
can stay out of the file.

```jsonc
{
  "listen": "127.0.0.1:9666",          // $KLIKNLOAD_LISTEN overrides it (Docker: 0.0.0.0:9666)
  "notifications": "all",              // all | errors | off
  "clipboardFallback": true,           // copy links if no output succeeded
  "dedupeLinks": true,
  "outputs": [ /* see below, run in parallel */ ]
}
```

Every output has `id`, `name`, `enabled`, `type` and optional `include` / `exclude` regexes
that select which links it gets.

| type | fields |
|---|---|
| `pyload` | `url`, `user`, `password` or `apiKey`, `destination` (`queue` \| `collector`), `packageName` (template) |
| `clipboard` | `template` (default `{{links}}`) |
| `file` | `directory`, `filename` (template, `/` creates subfolders), `format` (`txt` \| `json` \| `csv` \| `crawljob` \| `custom`), `template` (for `custom`), `mode` (`new` \| `append`), `onConflict` (`number` \| `overwrite` \| `skip`) |
| `http` | `method`, `url`, `headers` (`[{name, value}]`), `auth` (`{type: none}` \| `{type: basic, username, password}` \| `{type: bearer, token}`), `bodyType` (`none` \| `json` \| `form` \| `text`), `body`, `form` (`[{name, value}]`), `perLink`, `timeoutSecs`, `insecureTls` |
| `command` | `command` (run with `/bin/sh -c`), `workingDir`, `perLink`, `timeoutSecs` |

### Templates

`{{variable | filter | …}}` with the variables `package`, `links`, `link`, `index`, `count`,
`password`, `source`, `host`, `date`, `time`, `datetime`, `timestamp` and the filters `json`,
`url`, `raw`, `lines`, `comma`, `space`, `first`, `safe`, `lower`, `upper`, `trim`.

Package names and links come from arbitrary websites, so values are escaped for where they
end up: URL-encoded in URLs, JSON-string-escaped in JSON bodies, without line breaks in
headers, sanitized in file names. `json`, `url` and `raw` switch that off explicitly.
Command outputs never get template values in the command line; they receive
`KLIKNLOAD_PACKAGE`, `KLIKNLOAD_LINKS`, `KLIKNLOAD_LINK`, `KLIKNLOAD_INDEX`,
`KLIKNLOAD_COUNT`, `KLIKNLOAD_PASSWORD`, `KLIKNLOAD_SOURCE`, `KLIKNLOAD_HOST`,
`KLIKNLOAD_DATE`, `KLIKNLOAD_JSON` as environment variables and the links on stdin.

### Legacy format

The pyload-clicknload format still works and becomes a pyLoad output plus a clipboard output:

```json
{ "pyloadUrl": "https://pyload.example.org", "pyloadUser": "user", "pyloadPW": "secret" }
```

## pyLoad authentication

pyLoad 0.5 removed `/api/login`, which is why pyload-clicknload fails with
`Cannot read properties of undefined (reading '0')`. kliknload uses an API key if one is set,
otherwise the web login with CSRF token and the JSON API, and falls back to
`/api/login` + `/json/add_package` for pyLoad 0.4.

## Command line

```
kliknload [--headless] [--config PATH]   run (menu bar app on macOS)
kliknload --test-connection              log into the first pyLoad output
kliknload config get | set               print / replace the config as JSON
kliknload output preview | test          preview / try an output given as JSON on stdin
kliknload autostart on | off             start at login (macOS LaunchAgent)
kliknload notify-test [TEXT]             send a test desktop notification
```

## Development

```sh
cargo test                                   # all features (macOS)
cargo test --no-default-features             # headless build as used in Docker
scripts/bundle.sh                            # dist/kliknload.app
```

`src/` is the Rust core, `settings/` the SwiftUI settings window. The window has no logic of
its own: it calls the `config` and `output` subcommands of the core.

Logs: `~/Library/Logs/kliknload.log` (menu bar app) or stderr.

## License

MIT
