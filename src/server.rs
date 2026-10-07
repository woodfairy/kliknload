//! The Click'n'Load HTTP endpoint on port 9666, emulating JDownloader.

use crate::app::{App, AppEvent, ServerStatus};
use crate::cnl;
use crate::outputs::Package;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

type Fields = HashMap<String, String>;

fn text(body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

fn failed(err: impl std::fmt::Display) -> Response {
    error!("Click'n'Load request failed: {err}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("failed {err}\r\n"),
    )
        .into_response()
}

/// CNL senders post urlencoded forms, but some put fields into the query string or
/// send a wrong content type, so both are parsed leniently.
fn fields(query: Option<String>, body: &[u8]) -> Fields {
    let mut map = Fields::new();
    for (k, v) in form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .chain(form_urlencoded::parse(body))
    {
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

fn field<'a>(f: &'a Fields, key: &str) -> Option<&'a str> {
    f.get(key).map(|s| s.trim()).filter(|s| !s.is_empty())
}

fn package_name(f: &Fields) -> String {
    field(f, "package")
        .or_else(|| field(f, "source"))
        .map(str::to_string)
        .unwrap_or_else(|| {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("ClickNLoad-{ts}")
        })
}

/// Multiple passwords may be sent, one per line; pyLoad takes one, so use the first.
fn package_password(f: &Fields) -> Option<String> {
    field(f, "passwords")?
        .split(['\r', '\n'])
        .map(str::trim)
        .find(|p| !p.is_empty())
        .map(str::to_string)
}

/// Output failures are reported to the user, the website only learns that we got the links.
async fn deliver(app: &App, package: Package) -> Response {
    let _ = app.handle_package(package).await;
    text("success\r\n")
}

fn package(f: &Fields, links: Vec<String>) -> Package {
    Package::new(
        package_name(f),
        links,
        package_password(f),
        field(f, "source").map(str::to_string),
    )
}

async fn add_crypted2(State(app): State<Arc<App>>, RawQuery(q): RawQuery, body: Bytes) -> Response {
    let f = fields(q, &body);
    info!(
        "addcrypted2 from {} (package: {})",
        field(&f, "source").unwrap_or("?"),
        field(&f, "package").unwrap_or("-")
    );
    let (Some(crypted), Some(jk)) = (field(&f, "crypted"), field(&f, "jk")) else {
        return failed("missing crypted or jk");
    };
    let links = match cnl::decrypt_links(crypted, jk) {
        Ok(l) => l,
        Err(e) => {
            crate::platform::notify("Click'n'Load-Fehler", &format!("{e:#}"));
            return failed(format!("{e:#}"));
        }
    };
    deliver(&app, package(&f, links)).await
}

async fn add_plain(State(app): State<Arc<App>>, RawQuery(q): RawQuery, body: Bytes) -> Response {
    let f = fields(q, &body);
    // safelinking.net sends a literal "/r/n" as separator.
    let urls = field(&f, "urls").unwrap_or("").replace("/r/n", "\n");
    let links = cnl::split_links(&urls);
    if links.is_empty() {
        return failed("no urls");
    }
    deliver(&app, package(&f, links)).await
}

async fn add_dlc() -> Response {
    warn!("DLC containers (/flash/addcrypted) are not supported");
    (StatusCode::NOT_IMPLEMENTED, "failed DLC not supported\r\n").into_response()
}

const CROSSDOMAIN: &str = r#"<?xml version="1.0"?>
<!DOCTYPE cross-domain-policy SYSTEM "http://www.macromedia.com/xml/dtds/cross-domain-policy.dtd">
<cross-domain-policy>
<allow-access-from domain="*" />
</cross-domain-policy>
"#;

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(|| async { text("JDownloader\r\n") }))
        .route("/flash", get(|| async { text("JDownloader\r\n") }))
        .route("/flash/", get(|| async { text("JDownloader\r\n") }))
        .route(
            "/jdcheck.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    "jdownloader=true;\r\nvar version='42707';\r\n",
                )
            }),
        )
        .route(
            "/crossdomain.xml",
            get(|| async { ([(header::CONTENT_TYPE, "text/xml")], CROSSDOMAIN) }),
        )
        .route("/flash/add", post(add_plain))
        .route("/flash/addcrypted2", post(add_crypted2))
        .route("/flash/addcrypted", post(add_dlc))
        .route(
            "/flash/checkSupportForUrl",
            post(|| async { text("true\r\n") }),
        )
        .layer(CorsLayer::permissive().allow_private_network(true))
        .with_state(app)
}

/// Binds the configured address (default 127.0.0.1:9666, plus ::1 for localhost)
/// and serves until the process exits.
pub async fn run(app: Arc<App>) {
    app.emit(AppEvent::Server(ServerStatus::Starting));
    let listen = app.config().listen_addr();
    let addr: SocketAddr = match listen.parse() {
        Ok(a) => a,
        Err(_) => {
            let msg = format!("Ungültige Listen-Adresse „{listen}“");
            error!("{msg}");
            app.emit(AppEvent::Server(ServerStatus::Failed(msg)));
            return;
        }
    };
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("{addr} nicht verfügbar: {e}");
            error!("{msg} – is JDownloader or another Click'n'Load tool running?");
            crate::platform::notify(
                "kliknload",
                &format!("{msg}. Läuft JDownloader oder ein anderes Click'n'Load-Tool?"),
            );
            app.emit(AppEvent::Server(ServerStatus::Failed(msg)));
            return;
        }
    };
    let router = router(app.clone());
    if addr.ip() == std::net::IpAddr::from([127, 0, 0, 1]) {
        let v6 = SocketAddr::from(([0u16, 0, 0, 0, 0, 0, 0, 1], addr.port()));
        if let Ok(l) = tokio::net::TcpListener::bind(v6).await {
            let r = router.clone();
            tokio::spawn(async move { axum::serve(l, r).await });
        }
    }
    info!("Click'n'Load listening on http://{addr}");
    app.emit(AppEvent::Server(ServerStatus::Listening(addr.to_string())));
    if let Err(e) = axum::serve(listener, router).await {
        app.emit(AppEvent::Server(ServerStatus::Failed(e.to_string())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Notifications, Output, OutputKind, PyloadOutput};
    use crate::mock_pyload::{self, PASSWORD, USER};

    async fn start(pyload_url: &str) -> String {
        let mut cfg = Config::default();
        cfg.outputs = vec![Output::new(
            OutputKind::Pyload(PyloadOutput {
                url: pyload_url.into(),
                user: USER.into(),
                password: PASSWORD.into(),
                package_name: "{{package}}".into(),
                ..Default::default()
            }),
            true,
        )];
        cfg.notifications = Notifications::Off;
        cfg.clipboard_fallback = false;
        let app = App::new(
            cfg,
            std::env::temp_dir().join("kliknload-test.json"),
            |_| {},
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router(app)).await });
        addr
    }

    #[tokio::test]
    async fn jdcheck_and_root() {
        let addr = start("http://127.0.0.1:1").await;
        let body = reqwest::get(format!("{addr}/jdcheck.js"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(body.starts_with("jdownloader=true;"));
        let body = reqwest::get(format!("{addr}/flash/"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "JDownloader\r\n");
    }

    #[tokio::test]
    async fn addcrypted2_reaches_pyload() {
        let (pyload, calls) = mock_pyload::start().await;
        let addr = start(&pyload).await;
        let resp = reqwest::Client::new()
            .post(format!("{addr}/flash/addcrypted2"))
            .form(&[
                ("passwords", "myPassword\r\nother"),
                ("source", "http://jdownloader.org/spielwiese"),
                ("jk", "function f(){ return '31323334353637383930393837363534';}"),
                ("crypted", "DRurBGEf2ntP7Z0WDkMP8e1ZeK7PswJGeBHCg4zEYXZSE3Qqxsbi5EF1KosgkKQ9SL8qOOUAI+eDPFypAtQS9A=="),
                ("submit", "Add Link to JDownloader"),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "success\r\n");

        let calls = calls.calls.lock().unwrap();
        assert_eq!(calls[0].0, "add_package");
        assert_eq!(calls[0].1["name"], "http://jdownloader.org/spielwiese");
        assert_eq!(
            calls[0].1["links"][0],
            "http://rapidshare.com/files/285626259/jDownloader.dmg"
        );
        assert_eq!(calls[1].1["data"]["password"], "myPassword");
    }

    #[tokio::test]
    async fn plain_add_with_safelinking_separator() {
        let (pyload, calls) = mock_pyload::start().await;
        let addr = start(&pyload).await;
        let resp = reqwest::Client::new()
            .post(format!("{addr}/flash/add"))
            .form(&[("urls", "http://a/1/r/nhttp://a/2"), ("package", "Plain")])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let calls = calls.calls.lock().unwrap();
        assert_eq!(
            calls[0].1["links"],
            serde_json::json!(["http://a/1", "http://a/2"])
        );
        assert_eq!(calls[0].1["name"], "Plain");
    }

    #[tokio::test]
    async fn bad_payload_is_rejected() {
        // decryption errors are reported to the website
        let addr = start("http://127.0.0.1:1").await;
        let resp = reqwest::Client::new()
            .post(format!("{addr}/flash/addcrypted2"))
            .form(&[("crypted", "AAAA")])
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 500);
    }
}
