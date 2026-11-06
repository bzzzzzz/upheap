//! Test harness: the real router over an in-memory SQLite database and a manual clock.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use api::{AppState, CookieSettings};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use domain::{Duration, Timestamp};
use http_body_util::BodyExt;
use serde_json::Value;
use service::{ManualClock, Services, Settings};
use tower::ServiceExt;

/// 2027-01-01T12:00:00Z.
pub const START: i64 = 1_798_804_800_000;

/// Initialises tracing once per test binary; the level comes from `RUST_LOG`.
fn init_tracing() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .try_init();
    });
}

pub struct TestApp {
    router: Router,
    pub clock: Arc<ManualClock>,
}

/// A response with its body parsed as JSON (`Null` when empty).
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Value,
    pub session_cookie: Option<String>,
}

impl TestApp {
    pub async fn start() -> Result<Self> {
        init_tracing();
        let db = store::connect("sqlite::memory:").await?;
        store::run_migrations(&db).await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(
            db,
            clock.clone(),
            Settings {
                allow_registration: true,
                session_ttl: Duration::from_days(30),
            },
        );
        let cookies = CookieSettings {
            secure: true,
            max_age: StdDuration::from_secs(3600),
        };
        Ok(Self {
            router: api::router(AppState { services, cookies }),
            clock,
        })
    }

    /// Sends a request as the app does: with the CSRF and timezone headers.
    pub async fn send(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        body: Option<Value>,
    ) -> Result<TestResponse> {
        self.send_with(method, path, session, body, &[("x-csrf-protection", "1")])
            .await
    }

    pub async fn send_with(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse> {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("time-zone", "Europe/Berlin");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(session) = session {
            request = request.header(header::COOKIE, format!("heap_session={session}"));
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))?,
            None => request.body(Body::empty())?,
        };
        let response = self.router.clone().oneshot(request).await?;
        let status = response.status();
        let session_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("heap_session="))
            .and_then(|v| v.split(';').next())
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
        let bytes = response.into_body().collect().await?.to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)?
        };
        Ok(TestResponse {
            status,
            body,
            session_cookie,
        })
    }

    /// Registers a user and returns their session token and personal heap ID.
    pub async fn register(&self, email: &str) -> Result<(String, String)> {
        let body = serde_json::json!({ "email": email, "display_name": "Tester", "password": "a long test password" });
        let response = self
            .send(Method::POST, "/api/v1/auth/register", None, Some(body))
            .await?;
        anyhow::ensure!(
            response.status == StatusCode::CREATED,
            "register failed: {}",
            response.body
        );
        let session = response.session_cookie.context("no session cookie")?;
        let me = self.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
        let heap = me.body["teams"][0]["heaps"][0]["id"]
            .as_str()
            .context("no heap")?
            .to_owned();
        Ok((session, heap))
    }
}
