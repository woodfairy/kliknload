//! Minimal pyLoad client.
//!
//! Supported authentication, tried in this order:
//! 1. API key (`X-API-Key`, pyLoad >= 0.5 with API keys)
//! 2. Web session: `/login` form with CSRF token, then the JSON API with `X-CSRFToken`
//!    (current pyLoad, where `/api/login` is gone)
//! 3. Legacy: `/api/login` + `/json/add_package` (pyLoad 0.4.x, what pyload-clicknload used)

use super::Package;
use crate::config::{Destination, PyloadOutput};
use crate::template::{Escape, expand_env, render};
use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::{Client, StatusCode, redirect};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{debug, info};

static CSRF_INPUT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<input[^>]*name=["']csrf_token["'][^>]*>"#).unwrap());
static VALUE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"value=["']([^"']+)["']"#).unwrap());
static CSRF_META: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<meta[^>]*name=["']csrf-token["'][^>]*content=["']([^"']+)["']"#).unwrap()
});

enum Auth {
    ApiKey(String),
    Session { csrf: String },
    Legacy,
}

pub struct Session {
    http: Client,
    base: String,
    auth: Auth,
}

fn find_csrf(html: &str) -> Option<String> {
    if let Some(input) = CSRF_INPUT.find(html)
        && let Some(c) = VALUE_ATTR.captures(input.as_str())
    {
        return Some(c[1].to_string());
    }
    CSRF_META.captures(html).map(|c| c[1].to_string())
}

fn error_text(status: StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.trim().chars().take(200).collect());
    if detail.is_empty() {
        status.to_string()
    } else {
        format!("{status}: {detail}")
    }
}

impl Session {
    /// Connects and authenticates against the configured pyLoad instance.
    pub async fn open(cfg: &PyloadOutput) -> Result<Self> {
        let base = expand_env(cfg.url.trim()).trim_end_matches('/').to_string();
        if base.is_empty() {
            bail!("pyLoad-URL ist nicht eingerichtet");
        }
        let http = Client::builder()
            .cookie_store(true)
            .redirect(redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("kliknload/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let api_key = expand_env(cfg.api_key.trim());
        if !api_key.is_empty() {
            return Ok(Self {
                http,
                base,
                auth: Auth::ApiKey(api_key),
            });
        }
        let user = expand_env(cfg.user.trim());
        if user.is_empty() {
            bail!("pyLoad-Benutzer oder API-Key fehlt");
        }
        let password = expand_env(&cfg.password);

        let login_page = http
            .get(format!("{base}/login"))
            .send()
            .await
            .with_context(|| format!("Verbindung zu {base} fehlgeschlagen"))?;
        let login_status = login_page.status();
        let html = login_page.text().await.unwrap_or_default();

        match find_csrf(&html) {
            Some(token) if login_status.is_success() => {
                Self::session_login(http, base, &user, &password, token).await
            }
            _ => Self::legacy_login(http, base, &user, &password).await,
        }
    }

    async fn session_login(
        http: Client,
        base: String,
        user: &str,
        password: &str,
        token: String,
    ) -> Result<Self> {
        let resp = http
            .post(format!("{base}/login"))
            .header("Referer", format!("{base}/login"))
            .form(&[
                ("csrf_token", token.as_str()),
                ("username", user),
                ("password", password),
            ])
            .send()
            .await?;
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        // Success redirects away from the login page; failure re-renders it with 200.
        if !resp.status().is_redirection() || location.contains("/login") {
            bail!("pyLoad-Login für „{user}“ fehlgeschlagen (Benutzer oder Passwort falsch?)");
        }

        // The session is regenerated on login, so fetch a fresh CSRF token.
        let csrf = match http.get(format!("{base}/dashboard")).send().await {
            Ok(r) => find_csrf(&r.text().await.unwrap_or_default()),
            Err(_) => None,
        }
        .unwrap_or(token);

        info!("logged into pyLoad at {base} as '{user}'");
        Ok(Self {
            http,
            base,
            auth: Auth::Session { csrf },
        })
    }

    async fn legacy_login(http: Client, base: String, user: &str, password: &str) -> Result<Self> {
        debug!("no CSRF login form found, trying legacy /api/login");
        let resp = http
            .post(format!("{base}/api/login"))
            .form(&[("username", user), ("password", password)])
            .send()
            .await?;
        let status = resp.status();
        let got_cookie = resp.headers().contains_key(reqwest::header::SET_COOKIE);
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() || !got_cookie || body.trim() == "false" {
            bail!(
                "pyLoad-Login fehlgeschlagen ({})",
                error_text(status, &body)
            );
        }
        info!("logged into legacy pyLoad at {base} as '{user}'");
        Ok(Self {
            http,
            base,
            auth: Auth::Legacy,
        })
    }

    fn api_request(&self, method: reqwest::Method, func: &str) -> reqwest::RequestBuilder {
        let req = self
            .http
            .request(method, format!("{}/api/{func}", self.base))
            .header("Referer", format!("{}/", self.base));
        match &self.auth {
            Auth::ApiKey(key) => req.header("X-API-Key", key),
            Auth::Session { csrf } => req.header("X-CSRFToken", csrf),
            Auth::Legacy => req,
        }
    }

    async fn read<T: DeserializeOwned>(resp: reqwest::Response, func: &str) -> Result<T> {
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            bail!("pyLoad API {func} failed: {}", error_text(status, &body));
        }
        serde_json::from_str(&body)
            .with_context(|| format!("unexpected answer from {func}: {body:.200}"))
    }

    async fn post<T: DeserializeOwned>(&self, func: &str, body: Value) -> Result<T> {
        let resp = self
            .api_request(reqwest::Method::POST, func)
            .json(&body)
            .send()
            .await?;
        Self::read(resp, func).await
    }

    pub async fn server_version(&self) -> Result<String> {
        if matches!(self.auth, Auth::Legacy) {
            let resp = self
                .http
                .get(format!("{}/api/getServerVersion", self.base))
                .send()
                .await?;
            return Self::read(resp, "getServerVersion").await;
        }
        let resp = self
            .api_request(reqwest::Method::GET, "get_server_version")
            .send()
            .await?;
        Self::read(resp, "get_server_version").await
    }

    /// Adds a package and sets its extraction password. Returns the package id if known.
    pub async fn add_package(
        &self,
        name: &str,
        links: &[String],
        password: Option<&str>,
        to_queue: bool,
    ) -> Result<Option<i64>> {
        let dest = if to_queue { 1 } else { 0 };
        let password = password.map(str::trim).filter(|p| !p.is_empty());

        if matches!(self.auth, Auth::Legacy) {
            let form = reqwest::multipart::Form::new()
                .text("add_name", name.to_string())
                .text("add_links", links.join("\n"))
                .text("add_password", password.unwrap_or("").to_string())
                .text("add_dest", dest.to_string())
                .part(
                    "add_file",
                    reqwest::multipart::Part::bytes(Vec::new())
                        .file_name("")
                        .mime_str("application/octet-stream")?,
                );
            let resp = self
                .http
                .post(format!("{}/json/add_package", self.base))
                .multipart(form)
                .send()
                .await?;
            let status = resp.status();
            if !status.is_success() {
                bail!(
                    "pyLoad add_package failed: {}",
                    error_text(status, &resp.text().await?)
                );
            }
            return Ok(None);
        }

        let id: i64 = self
            .post(
                "add_package",
                json!({ "name": name, "links": links, "dest": dest }),
            )
            .await?;
        if let Some(pw) = password {
            let _: Value = self
                .post(
                    "set_package_data",
                    json!({ "package_id": id, "data": { "password": pw } }),
                )
                .await?;
        }
        Ok(Some(id))
    }
}

/// Logs in and returns the pyLoad version, for the "test connection" action.
pub async fn test_connection(cfg: &PyloadOutput) -> Result<String> {
    Session::open(cfg).await?.server_version().await
}

fn package_name(cfg: &PyloadOutput, package: &Package) -> Result<String> {
    let name = render(&cfg.package_name, &package.vars(), Escape::None)?;
    Ok(if name.trim().is_empty() {
        package.name.clone()
    } else {
        name.trim().to_string()
    })
}

pub async fn deliver(cfg: &PyloadOutput, package: &Package) -> Result<String> {
    let name = package_name(cfg, package)?;
    let session = Session::open(cfg).await?;
    let to_queue = cfg.destination == Destination::Queue;
    let id = session
        .add_package(&name, &package.links, package.password.as_deref(), to_queue)
        .await?;
    let target = if to_queue {
        "Warteschlange"
    } else {
        "Linksammler"
    };
    Ok(match id {
        Some(id) => format!("Paket „{name}“ (ID {id}) in {target}"),
        None => format!("Paket „{name}“ in {target}"),
    })
}

pub fn preview(cfg: &PyloadOutput, package: &Package) -> Result<String> {
    let name = package_name(cfg, package)?;
    let target = match cfg.destination {
        Destination::Queue => "Warteschlange",
        Destination::Collector => "Linksammler",
    };
    let auth = if cfg.api_key.trim().is_empty() {
        "Benutzer/Passwort"
    } else {
        "API-Key"
    };
    let mut out = format!(
        "Ziel: {}\nAnmeldung: {auth}\nPaket: {name}\nIn: {target}\n",
        expand_env(cfg.url.trim())
    );
    if let Some(pw) = package.password.as_deref().filter(|p| !p.is_empty()) {
        out.push_str(&format!("Passwort: {pw}\n"));
    }
    out.push_str(&format!(
        "\n{} Links:\n{}",
        package.links.len(),
        package.links.join("\n")
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_csrf_tokens() {
        let form = r#"<input id="csrf_token" name="csrf_token" type="hidden" value="abc.def"/>"#;
        assert_eq!(find_csrf(form).as_deref(), Some("abc.def"));
        let reversed = r#"<input value="tok" type="hidden" name="csrf_token">"#;
        assert_eq!(find_csrf(reversed).as_deref(), Some("tok"));
        let meta = r#"<meta name="csrf-token" content="m.t" />"#;
        assert_eq!(find_csrf(meta).as_deref(), Some("m.t"));
        assert_eq!(find_csrf("<html></html>"), None);
    }

    fn cfg(url: &str, user: &str, pw: &str, key: Option<&str>) -> PyloadOutput {
        PyloadOutput {
            url: format!("{url}/"),
            user: user.into(),
            password: pw.into(),
            api_key: key.unwrap_or("").into(),
            destination: Destination::Queue,
            package_name: "{{package}}".into(),
        }
    }

    #[tokio::test]
    async fn session_login_and_add_package() {
        use crate::mock_pyload::{self, PASSWORD, USER};
        let (url, calls) = mock_pyload::start().await;
        let session = Session::open(&cfg(&url, USER, PASSWORD, None))
            .await
            .unwrap();
        assert_eq!(session.server_version().await.unwrap(), "0.5.0");
        let links = vec![
            "https://a.example/1".to_string(),
            "https://a.example/2".to_string(),
        ];
        let id = session
            .add_package("Pkg", &links, Some("secret"), true)
            .await
            .unwrap();
        assert_eq!(id, Some(7));

        let calls = calls.calls.lock().unwrap();
        let add = calls.iter().find(|(f, _)| f == "add_package").unwrap();
        assert_eq!(add.1, json!({"name": "Pkg", "links": links, "dest": 1}));
        let data = calls.iter().find(|(f, _)| f == "set_package_data").unwrap();
        assert_eq!(
            data.1,
            json!({"package_id": 7, "data": {"password": "secret"}})
        );
    }

    #[tokio::test]
    async fn wrong_password_is_reported() {
        use crate::mock_pyload::{self, USER};
        let (url, _) = mock_pyload::start().await;
        let err = Session::open(&cfg(&url, USER, "nope", None))
            .await
            .err()
            .unwrap();
        assert!(err.to_string().contains("Login"), "{err}");
    }

    #[tokio::test]
    async fn api_key_auth() {
        use crate::mock_pyload::{self, API_KEY};
        let (url, calls) = mock_pyload::start().await;
        let session = Session::open(&cfg(&url, "", "", Some(API_KEY)))
            .await
            .unwrap();
        session
            .add_package("K", &["x://y".into()], None, false)
            .await
            .unwrap();
        let calls = calls.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "no password call without password");
        assert_eq!(calls[0].1["dest"], 0);
    }
}
