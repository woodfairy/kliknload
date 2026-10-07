use super::Package;
use crate::config::{BodyType, HttpAuth, HttpOutput, KeyValue};
use crate::template::{Escape, Vars, check_json, expand_env, render};
use anyhow::{Context, Result, bail};
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use std::time::Duration;

/// A fully rendered request.
#[derive(Debug)]
struct Prepared {
    method: reqwest::Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

fn prepare(cfg: &HttpOutput, vars: &Vars) -> Result<Prepared> {
    let method = reqwest::Method::from_bytes(cfg.method.trim().to_uppercase().as_bytes())
        .with_context(|| t!(HttpInvalidMethod, method = cfg.method))?;
    let url = render(&expand_env(cfg.url.trim()), vars, Escape::Url)?;
    reqwest::Url::parse(&url).with_context(|| t!(HttpInvalidUrl, url = url))?;

    let mut headers = Vec::new();
    for KeyValue { name, value } in &cfg.headers {
        let name = expand_env(name.trim());
        if name.is_empty() {
            continue;
        }
        headers.push((name, render(&expand_env(value), vars, Escape::Header)?));
    }
    let has = |headers: &[(String, String)], n: &str| {
        headers.iter().any(|(h, _)| h.eq_ignore_ascii_case(n))
    };

    match &cfg.auth {
        HttpAuth::None => {}
        HttpAuth::Basic { username, password } => {
            use base64::Engine;
            let raw = format!("{}:{}", expand_env(username), expand_env(password));
            let token = base64::engine::general_purpose::STANDARD.encode(raw);
            headers.push(("Authorization".into(), format!("Basic {token}")));
        }
        HttpAuth::Bearer { token } => {
            headers.push((
                "Authorization".into(),
                format!("Bearer {}", expand_env(token.trim())),
            ));
        }
    }

    let (body, content_type) = match cfg.body_type {
        BodyType::None => (None, None),
        BodyType::Json => {
            let body = render(&expand_env(&cfg.body), vars, Escape::JsonString)?;
            check_json(&body)?;
            (Some(body), Some("application/json"))
        }
        BodyType::Text => (
            Some(render(&expand_env(&cfg.body), vars, Escape::None)?),
            Some("text/plain; charset=utf-8"),
        ),
        BodyType::Form => {
            let mut ser = form_urlencoded::Serializer::new(String::new());
            for KeyValue { name, value } in &cfg.form {
                if !name.trim().is_empty() {
                    ser.append_pair(
                        name.trim(),
                        &render(&expand_env(value), vars, Escape::None)?,
                    );
                }
            }
            (
                Some(ser.finish()),
                Some("application/x-www-form-urlencoded"),
            )
        }
    };
    if let Some(ct) = content_type
        && !has(&headers, "content-type")
    {
        headers.push(("Content-Type".into(), ct.into()));
    }
    Ok(Prepared {
        method,
        url,
        headers,
        body,
    })
}

fn requests(cfg: &HttpOutput, package: &Package) -> Result<Vec<Prepared>> {
    if cfg.per_link {
        (0..package.links.len())
            .map(|i| prepare(cfg, &package.vars_for_link(i)))
            .collect()
    } else {
        Ok(vec![prepare(cfg, &package.vars())?])
    }
}

fn snippet(s: &str) -> String {
    let s = s.trim();
    let short: String = s.chars().take(300).collect();
    if short.len() < s.len() {
        format!("{short}…")
    } else {
        short
    }
}

pub async fn deliver(cfg: &HttpOutput, package: &Package) -> Result<String> {
    let prepared = requests(cfg, package)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.timeout_secs.max(1)))
        .danger_accept_invalid_certs(cfg.insecure_tls)
        .user_agent(concat!("kliknload/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let mut last = String::new();
    for (i, p) in prepared.iter().enumerate() {
        let mut req = client.request(p.method.clone(), &p.url);
        for (name, value) in &p.headers {
            req = req.header(
                HeaderName::from_bytes(name.as_bytes())?,
                HeaderValue::from_str(value)?,
            );
        }
        if let Some(body) = &p.body {
            req = req.body(body.clone());
        }
        let resp = req
            .send()
            .await
            .with_context(|| format!("{} {}", p.method, p.url))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let which = if prepared.len() > 1 {
                t!(HttpRequestOf, n = i + 1, total = prepared.len())
            } else {
                String::new()
            };
            bail!("HTTP {status}{which}: {}", snippet(&text));
        }
        last = format!(
            "HTTP {status}{}",
            if text.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", snippet(&text))
            }
        );
    }
    Ok(if prepared.len() > 1 {
        t!(HttpSentMany, count = prepared.len(), last = last)
    } else {
        last
    })
}

pub fn preview(cfg: &HttpOutput, package: &Package) -> Result<String> {
    let prepared = requests(cfg, package)?;
    let mut out = String::new();
    if prepared.len() > 1 {
        out.push_str(&format!(
            "{}\n\n",
            t!(HttpPreviewPerLink, count = prepared.len())
        ));
    }
    for p in prepared.iter().take(3) {
        out.push_str(&format!("{} {}\n", p.method, p.url));
        for (name, value) in &p.headers {
            let shown = if name.eq_ignore_ascii_case("authorization") {
                "••••••"
            } else {
                value
            };
            out.push_str(&format!("{name}: {shown}\n"));
        }
        if let Some(body) = &p.body {
            let pretty = p
                .headers
                .iter()
                .any(|(n, v)| n.eq_ignore_ascii_case(CONTENT_TYPE.as_str()) && v.contains("json"))
                .then(|| serde_json::from_str::<serde_json::Value>(body).ok())
                .flatten()
                .and_then(|v| serde_json::to_string_pretty(&v).ok());
            out.push_str(&format!("\n{}\n", pretty.as_deref().unwrap_or(body)));
        }
        out.push('\n');
    }
    if prepared.len() > 3 {
        out.push_str(&format!(
            "{}\n",
            t!(HttpPreviewMore, count = prepared.len() - 3)
        ));
    }
    Ok(out.trim_end().to_string())
}

/// Ready-made request templates for the settings window: (id, label, config).
pub fn presets() -> Vec<(&'static str, String, HttpOutput)> {
    let json = |url: &str, body: &str, per_link: bool| HttpOutput {
        method: "POST".into(),
        url: url.into(),
        body_type: BodyType::Json,
        body: body.into(),
        per_link,
        ..HttpOutput::default()
    };
    vec![
        (
            "webhook",
            t!(PresetWebhook),
            HttpOutput {
                url: "https://example.org/webhook".into(),
                ..HttpOutput::default()
            },
        ),
        (
            "aria2",
            t!(PresetAria2),
            json(
                "http://localhost:6800/jsonrpc",
                "{\n  \"jsonrpc\": \"2.0\",\n  \"id\": \"kliknload\",\n  \"method\": \"aria2.addUri\",\n  \"params\": [\"token:${ARIA2_SECRET}\", [{{link|json}}], {}]\n}",
                true,
            ),
        ),
        (
            "discord",
            t!(PresetDiscord),
            json(
                "https://discord.com/api/webhooks/ID/TOKEN",
                "{\n  \"content\": \"**{{package}}** ({{count}} Links)\\n{{links}}\"\n}",
                false,
            ),
        ),
        (
            "slack",
            t!(PresetSlack),
            json(
                "https://hooks.slack.com/services/XXX/YYY/ZZZ",
                "{\n  \"text\": \"*{{package}}* ({{count}} Links)\\n{{links}}\"\n}",
                false,
            ),
        ),
        (
            "gotify",
            t!(PresetGotify),
            json(
                "https://gotify.example.org/message?token=${GOTIFY_TOKEN}",
                "{\n  \"title\": \"kliknload: {{package}}\",\n  \"message\": \"{{links}}\",\n  \"priority\": 5\n}",
                false,
            ),
        ),
        (
            "ntfy",
            t!(PresetNtfy),
            HttpOutput {
                method: "POST".into(),
                url: "https://ntfy.sh/mein-topic".into(),
                headers: vec![KeyValue {
                    name: "Title".into(),
                    value: "kliknload: {{package}}".into(),
                }],
                body_type: BodyType::Text,
                body: "{{count}} Links\n{{links}}".into(),
                ..HttpOutput::default()
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_render_valid_requests() {
        for (id, _, cfg) in presets() {
            let p = requests(&cfg, &Package::sample()).unwrap_or_else(|e| panic!("{id}: {e:#}"));
            assert!(!p.is_empty(), "{id}");
        }
    }

    #[test]
    fn escapes_hostile_package_names() {
        let mut pkg = Package::sample();
        pkg.name = "x\", \"evil\": true, \"y\nAuth: z".into();
        let mut cfg = HttpOutput::default();
        cfg.url = "https://h.example/add?name={{package}}".into();
        cfg.headers = vec![KeyValue {
            name: "X-Name".into(),
            value: "{{package}}".into(),
        }];
        let p = &requests(&cfg, &pkg).unwrap()[0];
        let body: serde_json::Value = serde_json::from_str(p.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["package"], pkg.name);
        assert!(body.get("evil").is_none());
        assert!(!p.url.contains(' ') && !p.url.contains('"'));
        assert!(!p.headers[0].1.contains('\n'));
    }

    #[tokio::test]
    async fn sends_form_request() {
        use axum::{Router, routing::post};
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        let app = Router::new().route(
            "/hook",
            post(move |headers: axum::http::HeaderMap, body: String| {
                let tx = tx.clone();
                async move {
                    let auth = headers
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_string();
                    tx.send((auth, body)).unwrap();
                    "ok"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let cfg = HttpOutput {
            url: format!("http://{addr}/hook"),
            auth: HttpAuth::Bearer {
                token: "t0k".into(),
            },
            body_type: BodyType::Form,
            form: vec![KeyValue {
                name: "links".into(),
                value: "{{links|comma}}".into(),
            }],
            ..HttpOutput::default()
        };
        let msg = deliver(&cfg, &Package::sample()).await.unwrap();
        assert!(msg.starts_with("HTTP 200"), "{msg}");
        let (auth, body) = rx.recv().await.unwrap();
        assert_eq!(auth, "Bearer t0k");
        assert!(body.starts_with("links=https%3A%2F%2Fhoster.example"));
    }
}
