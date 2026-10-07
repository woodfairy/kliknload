//! A tiny fake pyLoad (current version: CSRF form login, JSON API) for tests.

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub const USER: &str = "user";
pub const PASSWORD: &str = "p&ss=wörd+";
pub const API_KEY: &str = "pl_testkey";

#[derive(Default)]
pub struct Calls {
    pub calls: Mutex<Vec<(String, Value)>>,
}

fn has_cookie(h: &HeaderMap, value: &str) -> bool {
    h.get_all(header::COOKIE).iter().any(|c| {
        c.to_str()
            .unwrap_or("")
            .contains(&format!("session={value}"))
    })
}

fn authorized(h: &HeaderMap) -> Result<(), Response> {
    if h.get("X-API-Key").is_some_and(|k| k == API_KEY) {
        return Ok(());
    }
    if !has_cookie(h, "auth") {
        return Err((
            StatusCode::UNAUTHORIZED,
            r#"{"error": "Invalid API credentials"}"#,
        )
            .into_response());
    }
    if !h.get("X-CSRFToken").is_some_and(|t| t == "tok2") {
        return Err((
            StatusCode::BAD_REQUEST,
            r#"{"error": "CSRF token is invalid"}"#,
        )
            .into_response());
    }
    Ok(())
}

async fn api(
    State(calls): State<Arc<Calls>>,
    axum::extract::Path(func): axum::extract::Path<String>,
    headers: HeaderMap,
    body: String,
) -> Response {
    if let Err(r) = authorized(&headers) {
        return r;
    }
    let args: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    calls.calls.lock().unwrap().push((func.clone(), args));
    match func.as_str() {
        "add_package" => axum::Json(json!(7)).into_response(),
        "set_package_data" => axum::Json(Value::Null).into_response(),
        "get_server_version" => axum::Json(json!("0.5.0")).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn start() -> (String, Arc<Calls>) {
    let calls = Arc::new(Calls::default());
    let router = Router::new()
        .route(
            "/login",
            get(|| async {
                (
                    [(header::SET_COOKIE, "session=anon; Path=/")],
                    axum::response::Html(
                        r#"<form><input id="csrf_token" name="csrf_token" type="hidden" value="tok1"/></form>"#,
                    ),
                )
            })
            .post(|body: String| async move {
                let form: Vec<(String, String)> = form_urlencoded::parse(body.as_bytes()).into_owned().collect();
                let get = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
                if get("csrf_token") == Some("tok1") && get("username") == Some(USER) && get("password") == Some(PASSWORD) {
                    (
                        StatusCode::FOUND,
                        [(header::LOCATION, "http://elsewhere/dashboard"), (header::SET_COOKIE, "session=auth; Path=/")],
                    )
                        .into_response()
                } else {
                    axum::response::Html("<p>login failed</p>").into_response()
                }
            }),
        )
        .route(
            "/dashboard",
            get(|h: HeaderMap| async move {
                if has_cookie(&h, "auth") {
                    axum::response::Html(r#"<meta name="csrf-token" content="tok2" />"#).into_response()
                } else {
                    (StatusCode::FOUND, [(header::LOCATION, "/login")]).into_response()
                }
            }),
        )
        .route("/api/login", post(|| async { (StatusCode::NOT_FOUND, "Obsolete API") }))
        .route("/api/{func}", get(api).post(api))
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await });
    (url, calls)
}
