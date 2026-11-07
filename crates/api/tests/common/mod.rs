//! Test harness: the real router over a fresh database and a manual clock.
//!
//! The database is in-memory SQLite, or a new MySQL database when `TEST_DATABASE_URL` is set (`store::testing`).
//!
//! Each test binary uses a different part of the harness, so unused items are expected.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use api::{AppState, CookieSettings};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use domain::sign_in::SignInLimit;
use domain::team::{Membership, Role};
use domain::{Duration, TeamId, Timestamp, UserId};
use http_body_util::BodyExt;
use serde_json::Value;
use service::{Clock, ManualClock, Services, Settings};
use store::{ConnectionTrait, DatabaseConnection, Statement};
use tower::ServiceExt;

/// The header every write from the app carries.
pub const CSRF: (&str, &str) = ("x-csrf-protection", "1");

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
    /// The same database the router uses, for fixtures the API can't create yet.
    db: DatabaseConnection,
}

/// A response with its body parsed as JSON (`Null` when empty).
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Value,
    pub session_cookie: Option<String>,
    /// The `Retry-After` header, in seconds.
    pub retry_after: Option<String>,
}

impl TestApp {
    pub async fn start() -> Result<Self> {
        init_tracing();
        let db = store::testing::fresh_database().await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(
            db.clone(),
            clock.clone(),
            Settings {
                allow_registration: true,
                session_ttl: Duration::from_days(30),
                sign_in_limit: SignInLimit::DEFAULT,
            },
        );
        let cookies = CookieSettings {
            secure: true,
            max_age: StdDuration::from_secs(3600),
        };
        Ok(Self {
            router: api::router(AppState { services, cookies }),
            clock,
            db,
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
        self.send_with(method, path, session, body, &[CSRF]).await
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
        let retry_after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
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
            retry_after,
        })
    }

    /// Creates an item in `heap` from `body`, whatever the outcome.
    pub async fn create_item(&self, session: &str, heap: &str, body: Value) -> Result<TestResponse> {
        self.send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/items"),
            Some(session),
            Some(body),
        )
        .await
    }

    /// Creates an item that must succeed, and returns its body.
    pub async fn new_item(&self, session: &str, heap: &str, body: Value) -> Result<Value> {
        let created = self.create_item(session, heap, body).await?;
        anyhow::ensure!(
            created.status == StatusCode::CREATED,
            "create item failed: {}",
            created.body
        );
        Ok(created.body)
    }

    /// Creates a tag named `name` in `heap`, whatever the outcome.
    pub async fn create_tag(&self, session: &str, heap: &str, name: &str) -> Result<TestResponse> {
        self.send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/tags"),
            Some(session),
            Some(serde_json::json!({ "name": name })),
        )
        .await
    }

    /// Creates a tag that must succeed, and returns its ID.
    pub async fn new_tag_id(&self, session: &str, heap: &str, name: &str) -> Result<String> {
        let created = self.create_tag(session, heap, name).await?;
        anyhow::ensure!(
            created.status == StatusCode::CREATED,
            "create tag failed: {}",
            created.body
        );
        Ok(created.body["id"].as_str().context("tag has no id")?.to_owned())
    }

    /// Sends a `PATCH` to `path` with `version` (as the API returns it) in `If-Match`.
    pub async fn patch(&self, session: &str, path: &str, version: &Value, body: Value) -> Result<TestResponse> {
        self.send_with(
            Method::PATCH,
            path,
            Some(session),
            Some(body),
            &[CSRF, ("if-match", &version.to_string())],
        )
        .await
    }

    /// Registers a user and returns their session token and personal heap ID.
    pub async fn register(&self, email: &str) -> Result<(String, String)> {
        self.register_as(email, "Tester").await
    }

    /// Registers a user called `display_name` and returns their session token and personal heap ID.
    pub async fn register_as(&self, email: &str, display_name: &str) -> Result<(String, String)> {
        let body =
            serde_json::json!({ "email": email, "display_name": display_name, "password": "a long test password" });
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

    /// Adds `user_id` to `team_id` with `role` straight in the database, so a test can set up any role in one step.
    /// It writes no audit entry.
    pub async fn add_member(&self, team_id: &str, user_id: &str, role: Role) -> Result<()> {
        let membership = Membership {
            team_id: TeamId::from_uuid(team_id.parse()?),
            user_id: UserId::from_uuid(user_id.parse()?),
            role,
            joined_at: self.clock.now(),
        };
        store::teams::insert_membership(&self.db, &membership)
            .await
            .context("add member")?;
        Ok(())
    }

    /// The audit entries about `entity_id`, oldest first, as `(action, diff)`.
    ///
    /// Read straight from the database, so it sees every entry whatever the caller's role; `tests/audit.rs` covers the
    /// endpoints.
    pub async fn audit_entries(&self, entity_id: &str) -> Result<Vec<(String, Value)>> {
        let id: uuid::Uuid = entity_id.parse()?;
        let sql = format!("SELECT action, diff FROM audit_log WHERE entity_id = '{id}' ORDER BY created_at, id");
        let rows = self
            .db
            .query_all_raw(Statement::from_string(self.db.get_database_backend(), sql))
            .await?;
        rows.into_iter()
            .map(|row| {
                let action: String = row.try_get("", "action")?;
                let diff: String = row.try_get("", "diff")?;
                Ok((action, serde_json::from_str(&diff)?))
            })
            .collect()
    }

    /// Archives or unarchives a heap straight in the database, without the version change or audit entry of the
    /// endpoints, so the access-control test can wrap any call in it.
    pub async fn set_heap_archived(&self, heap_id: &str, archived: bool) -> Result<()> {
        let archived_at = if archived {
            self.clock.now().as_millis().to_string()
        } else {
            "NULL".to_owned()
        };
        let id: uuid::Uuid = heap_id.parse()?;
        self.db
            .execute_unprepared(&format!(
                "UPDATE heaps SET archived_at = {archived_at} WHERE id = '{id}'"
            ))
            .await
            .context("set heap archived")?;
        Ok(())
    }
}
