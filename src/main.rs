//! kliknload – Click'n'Load (CNL2) receiver with pluggable outputs.
//! Drop-in replacement for pyload-clicknload with a macOS menu bar app.
// Parts of the shared state are only read by the menu bar UI.
#![cfg_attr(not(feature = "gui"), allow(dead_code))]
#![cfg_attr(
    test,
    allow(clippy::field_reassign_with_default, clippy::result_large_err)
)]

#[macro_use]
mod i18n;
mod app;
mod cli;
mod cnl;
mod config;
#[cfg(feature = "gui")]
mod icon;
#[cfg(test)]
mod mock_pyload;
mod notify;
mod outputs;
mod platform;
mod server;
mod template;
#[cfg(feature = "gui")]
mod tray;

use anyhow::{Result, bail};
use app::App;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

const HELP: &str = "\
kliknload – Click'n'Load receiver for pyLoad, HTTP, files, commands and the clipboard

USAGE:
    kliknload [OPTIONS]                 run (menu bar app on macOS)
    kliknload config get|set            print / replace the config as JSON (stdin)
    kliknload output preview|test       preview / try one output (JSON on stdin)
    kliknload autostart on|off          start at login (macOS)
    kliknload notify-test [TEXT]        send a test desktop notification

OPTIONS:
    --headless            Run without menu bar icon (terminal / Docker)
    --config <PATH>       Config file (default: ./pyloadConfig.json if present,
                          else ~/Library/Application Support/kliknload/pyloadConfig.json)
    --test-connection     Log into the first pyLoad output, print its version and exit
    -h, --help            Show this help
    -V, --version         Show version

ENVIRONMENT:
    KLIKNLOAD_CONFIG      Config file path
    KLIKNLOAD_LISTEN      Listen address, overrides the config (Docker: 0.0.0.0:9666)
    RUST_LOG              Log filter (default: info)
    ${NAME} in config values is replaced by the environment variable NAME.
";

struct Args {
    headless: bool,
    test_connection: bool,
    config: Option<PathBuf>,
    command: Vec<String>,
}

fn parse_args() -> Args {
    let mut args = Args {
        headless: false,
        test_connection: false,
        config: None,
        command: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--headless" | "--no-tray" => args.headless = true,
            "--test-connection" => args.test_connection = true,
            "--config" => args.config = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("kliknload {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            // macOS passes -psn_… when launched from Finder on old systems.
            a if a.starts_with("-psn_") => {}
            a if a.starts_with('-') => {
                eprintln!("unknown option: {a}\n\n{HELP}");
                std::process::exit(2);
            }
            _ => args.command.push(arg),
        }
    }
    args
}

fn init_logging(to_file: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hyper=warn,reqwest=warn"));
    let file = to_file
        .then(|| {
            let log_path = platform::log_path();
            // Keep the log file from growing forever.
            if std::fs::metadata(&log_path).is_ok_and(|m| m.len() > 5 * 1024 * 1024) {
                let _ = std::fs::remove_file(&log_path);
            }
            log_path.parent().map(std::fs::create_dir_all);
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok()
        })
        .flatten();

    let registry = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
            .with_target(false),
    );
    match file {
        Some(f) => registry
            .with(
                fmt::layer()
                    .with_writer(Mutex::new(f))
                    .with_ansi(false)
                    .with_target(false),
            )
            .init(),
        None => registry.init(),
    }
}

fn run_command(
    args: &Args,
    config_path: &std::path::Path,
    rt: &tokio::runtime::Runtime,
) -> Result<()> {
    let words: Vec<&str> = args.command.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["config", "get"] => cli::config_get(config_path),
        ["config", "set"] => cli::config_set(config_path),
        ["output", "preview"] => cli::output_preview(),
        ["output", "test"] => rt.block_on(cli::output_test()),
        ["autostart", "on"] => cli::autostart(true),
        ["autostart", "off"] => cli::autostart(false),
        ["notify-test", rest @ ..] => {
            if notify::permission() == notify::Permission::NotDetermined {
                eprintln!("{}", t!(NotifyPermissionPrompt));
            }
            match notify::wait_for_decision(std::time::Duration::from_secs(120)) {
                notify::Permission::Denied => {
                    notify::open_settings();
                    bail!("{}", t!(NotifyPermissionDenied));
                }
                notify::Permission::NotDetermined => {
                    bail!("{}", t!(NotifyPermissionUnanswered))
                }
                _ => {}
            }
            let message = if rest.is_empty() {
                t!(NotifyTest)
            } else {
                rest.join(" ")
            };
            notify::send("kliknload", &message)?;
            // Give asynchronous system APIs a moment before the process exits.
            std::thread::sleep(std::time::Duration::from_secs(1));
            println!("notification sent");
            Ok(())
        }
        ["notify-status"] => {
            println!(
                "{}",
                serde_json::json!({ "status": notify::permission().as_str() })
            );
            Ok(())
        }
        ["notify-settings"] => {
            notify::open_settings();
            Ok(())
        }
        #[cfg(feature = "gui")]
        ["render-icon", size, out] => {
            icon::write_app_icon_png(size.parse()?, std::path::Path::new(out))
        }
        _ => bail!("unknown command: {}\n\n{HELP}", args.command.join(" ")),
    }
}

fn main() -> Result<()> {
    let args = parse_args();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let config_path = config::resolve_config_path(args.config.clone());

    if !args.command.is_empty() {
        // Machine-readable mode: logs only to stderr, and only warnings.
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(EnvFilter::new("warn"))
            .init();
        // Messages for the settings window follow the configured language.
        if let Ok(cfg) = config::Config::load(&config_path) {
            i18n::set(i18n::Lang::from_setting(&cfg.language));
        }
        return run_command(&args, &config_path, &rt);
    }

    let headless = args.headless
        || cfg!(not(feature = "gui"))
        || cfg!(not(any(target_os = "macos", target_os = "windows")));
    // The menu bar app logs to a file too; terminal and Docker use stderr only.
    init_logging(!headless && !args.test_connection);
    info!(
        "kliknload {} – config: {}",
        env!("CARGO_PKG_VERSION"),
        config_path.display()
    );
    let cfg = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!("{e:#}");
            platform::notify(
                "kliknload",
                &t!(NotifyConfigInvalid, error = format!("{e:#}")),
            );
            config::Config::default()
        }
    };
    i18n::set(i18n::Lang::from_setting(&cfg.language));
    for problem in cfg.validate() {
        warn!("config: {problem}");
    }
    for o in cfg.outputs.iter().filter(|o| o.enabled) {
        info!(
            "output enabled: {} ({})",
            o.display_name(),
            o.kind.type_name()
        );
    }

    if args.test_connection {
        let Some(pyload) = cfg
            .outputs
            .iter()
            .find(|o| matches!(o.kind, config::OutputKind::Pyload(_)))
        else {
            bail!("no pyLoad output configured");
        };
        return match rt.block_on(outputs::test(pyload)) {
            Ok(msg) => {
                println!("OK – {msg}");
                Ok(())
            }
            Err(e) => {
                eprintln!("FAILED – {e:#}");
                std::process::exit(1);
            }
        };
    }

    if headless {
        let app = App::new(cfg, config_path, |_| {});
        rt.block_on(async {
            tokio::spawn(app.clone().watch_config());
            tokio::select! {
                _ = server::run(app) => {}
                _ = shutdown_signal() => info!("bye"),
            }
        });
        return Ok(());
    }

    #[cfg(feature = "gui")]
    {
        tray::run(move |proxy| {
            let app = App::new(cfg, config_path, move |event| {
                let _ = proxy.send_event(tray::UserEvent::App(event));
            });
            let server_app = app.clone();
            // Keep the runtime alive even if the server fails to bind, the menu still uses it.
            std::thread::spawn(move || {
                rt.block_on(async {
                    tokio::spawn(server_app.clone().watch_config());
                    server::run(server_app).await;
                    std::future::pending::<()>().await
                })
            });
            app
        })
    }
    #[cfg(not(feature = "gui"))]
    unreachable!()
}

/// Ctrl+C, and SIGTERM for `docker stop`.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
