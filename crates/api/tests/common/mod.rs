//! Test harness: the real router over a fresh database and a manual clock.
//!
//! The database is in-memory SQLite, or a new MySQL or Postgres database when `TEST_DATABASE_URL` is set
//! (`store::testing`).
//!
//! Each test binary uses a different part of the harness, so unused items are expected.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use api::{AppState, Authentication, CookieSettings};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use domain::sign_in::SignInLimit;
use domain::team::{Membership, Role};
use domain::{Duration, TeamId, Timestamp, UserId};
use http_body_util::BodyExt;
use serde_json::Value;
use service::{Clock, ManualClock, RandomSource, Services, Settings, SystemRandom};
use store::{ConnectionTrait, DatabaseConnection, Statement};
use tower::ServiceExt;

/// The header every write from the app carries.
pub const CSRF: (&str, &str) = ("x-csrf-protection", "1");

/// The CLI's client ID, and a loopback redirect URI it might listen on.
pub const CLI_CLIENT: &str = "upheap-cli";
pub const CLI_REDIRECT: &str = "http://127.0.0.1:53682/callback";
/// A PKCE code verifier and its S256 challenge, from RFC 7636 Appendix B.
pub const PKCE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
pub const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

/// The CLI's authorization request, as the consent page sends it to approve.
pub fn cli_authorization() -> Value {
    serde_json::json!({
        "response_type": "code",
        "client_id": CLI_CLIENT,
        "redirect_uri": CLI_REDIRECT,
        "code_challenge": PKCE_CHALLENGE,
        "code_challenge_method": "S256",
        "state": "af0ifjsldkj",
    })
}

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
    services: Services,
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
    pub headers: axum::http::HeaderMap,
}

/// The settings every test server shares, whether registration is open or not.
fn test_settings(allow_registration: bool) -> Settings {
    Settings {
        allow_registration,
        session_ttl: Duration::from_days(30),
        sign_in_limit: SignInLimit::DEFAULT,
    }
}

/// The session cookie settings every test server shares.
fn test_cookies() -> CookieSettings {
    CookieSettings {
        secure: true,
        max_age: StdDuration::from_secs(3600),
    }
}

impl TestApp {
    pub async fn start() -> Result<Self> {
        Self::start_with_random(Arc::new(SystemRandom)).await
    }

    /// [`TestApp::start`], with item keys drawn from `random`.
    pub async fn start_with_random(random: Arc<dyn RandomSource>) -> Result<Self> {
        init_tracing();
        let db = store::testing::fresh_database().await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(db.clone(), clock.clone(), test_settings(true)).with_random(random);
        Ok(Self {
            router: api::router(AppState {
                services: services.clone(),
                authentication: Authentication::Sessions(test_cookies()),
            }),
            services,
            clock,
            db,
        })
    }

    /// The same server with registration closed: a second router over the same database and clock. Set accounts up
    /// on `self`, and exercise the closed one.
    pub fn with_registration_closed(&self) -> TestApp {
        let services = Services::new(self.db.clone(), self.clock.clone(), test_settings(false));
        TestApp {
            router: api::router(AppState {
                services: services.clone(),
                authentication: Authentication::Sessions(test_cookies()),
            }),
            services,
            clock: self.clock.clone(),
            db: self.db.clone(),
        }
    }

    /// The local build's setup: one user, made on start, who is every request's caller.
    pub async fn start_local() -> Result<Self> {
        init_tracing();
        let db = store::testing::fresh_database().await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(db.clone(), clock.clone(), Settings::default());
        let local_user = services.open_local_user().await?;
        Ok(Self {
            router: api::router(AppState {
                services: services.clone(),
                authentication: Authentication::LocalUser(local_user),
            }),
            services,
            clock,
            db,
        })
    }

    /// How many questions wait for the user with `user_id`, and how many rows the list holds when it is capped at
    /// `max_rows`, which the cap's real size makes too many questions to make in a test.
    pub async fn waiting_capped(&self, user_id: &str, max_rows: usize) -> Result<(u32, usize)> {
        let principal = service::Principal {
            user_id: UserId::from_uuid(user_id.parse()?),
            run: None,
        };
        let waiting = self.services.list_waiting_capped(principal, max_rows).await?;
        Ok((waiting.count, waiting.rows.len()))
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
        self.dispatch(request).await
    }

    /// Posts `form` to `path` as `application/x-www-form-urlencoded`, as an OAuth client does: no cookie and no CSRF
    /// header.
    pub async fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<TestResponse> {
        let body = form
            .iter()
            .map(|(name, value)| format!("{name}={}", form_escape(value)))
            .collect::<Vec<_>>()
            .join("&");
        let request = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))?;
        self.dispatch(request).await
    }

    async fn dispatch(&self, request: Request<Body>) -> Result<TestResponse> {
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
        let headers = response.headers().clone();
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
            headers,
        })
    }

    /// Creates an item in `heap` from `body`, whatever the outcome. A body without a `need` gets `"expected"`, so
    /// tests about other fields don't have to say it; send the request yourself to leave it out.
    pub async fn create_item(&self, session: &str, heap: &str, mut body: Value) -> Result<TestResponse> {
        if let Value::Object(fields) = &mut body {
            fields.entry("need").or_insert_with(|| Value::from("expected"));
        }
        self.send(
            Method::POST,
            &format!("{}/items", self.heap_url(heap).await?),
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
            &format!("{}/tags", self.heap_url(heap).await?),
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

    /// Registers a user and returns their session token and personal heap ID. Their username is made from the
    /// email's local part.
    pub async fn register(&self, email: &str) -> Result<(String, String)> {
        self.register_as(email, "Tester").await
    }

    /// Registers a user called `display_name` and returns their session token and personal heap ID. Their username
    /// is made from the email's local part: `ada@example.test` is `ada`.
    pub async fn register_as(&self, email: &str, display_name: &str) -> Result<(String, String)> {
        let username = username_for(email);
        let body = serde_json::json!({
            "email": email,
            "username": username,
            "display_name": display_name,
            "password": "a long test password",
        });
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

    /// Approves the CLI's authorization request as the user of `session`, and returns the code its redirect URI gets.
    pub async fn approve_cli(&self, session: &str) -> Result<String> {
        let approved = self
            .send(
                Method::POST,
                "/api/v1/oauth/authorize",
                Some(session),
                Some(cli_authorization()),
            )
            .await?;
        anyhow::ensure!(approved.status == StatusCode::OK, "approve failed: {}", approved.body);
        let redirect = approved.body["redirect_to"].as_str().context("no redirect_to")?;
        let query = redirect
            .strip_prefix(&format!("{CLI_REDIRECT}?"))
            .context("redirect_to isn't the redirect URI")?;
        let code = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("code="))
            .context("no code in redirect_to")?;
        Ok(code.to_owned())
    }

    /// Exchanges `code` for tokens as the CLI does, whatever the outcome.
    pub async fn exchange_code(&self, code: &str) -> Result<TestResponse> {
        self.post_form(
            "/api/v1/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", CLI_REDIRECT),
                ("client_id", CLI_CLIENT),
                ("code_verifier", PKCE_VERIFIER),
            ],
        )
        .await
    }

    /// Signs the CLI in as the user of `session` and returns its access and refresh tokens.
    pub async fn sign_in_cli(&self, session: &str) -> Result<(String, String)> {
        let code = self.approve_cli(session).await?;
        let tokens = self.exchange_code(&code).await?;
        anyhow::ensure!(
            tokens.status == StatusCode::OK,
            "token exchange failed: {}",
            tokens.body
        );
        let access = tokens.body["access_token"].as_str().context("no access token")?;
        let refresh = tokens.body["refresh_token"].as_str().context("no refresh token")?;
        Ok((access.to_owned(), refresh.to_owned()))
    }

    /// Sends a request authenticated by `token` alone, as the CLI does: no cookie and no CSRF header.
    pub async fn send_bearer(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> Result<TestResponse> {
        let authorization = format!("Bearer {token}");
        self.send_with(method, path, None, body, &[("authorization", &authorization)])
            .await
    }

    /// Makes a team called "Shared" with a heap called "Shared" as the user of `session`, who is its admin, and returns
    /// their IDs. Personal teams take no members, so tests that share a heap use one of these.
    pub async fn shared_heap(&self, session: &str) -> Result<(String, String)> {
        let team = self
            .send(
                Method::POST,
                "/api/v1/teams",
                Some(session),
                Some(serde_json::json!({ "name": "Shared" })),
            )
            .await?;
        anyhow::ensure!(team.status == StatusCode::CREATED, "create team failed: {}", team.body);
        let slug = team.body["slug"].as_str().context("team has no slug")?;
        let heap = self
            .send(
                Method::POST,
                &format!("/api/v1/teams/{slug}/heaps"),
                Some(session),
                Some(serde_json::json!({ "name": "Shared" })),
            )
            .await?;
        anyhow::ensure!(heap.status == StatusCode::CREATED, "create heap failed: {}", heap.body);
        let team_id = team.body["id"].as_str().context("team has no id")?.to_owned();
        let heap_id = heap.body["id"].as_str().context("heap has no id")?.to_owned();
        Ok((team_id, heap_id))
    }

    /// `/api/v1/teams/{team}` for the team with `team_id`, from its slug in the database.
    pub async fn team_url(&self, team_id: impl AsRef<str>) -> Result<String> {
        let id: uuid::Uuid = team_id.as_ref().parse()?;
        let slug = self
            .query_text(&format!("SELECT slug FROM teams WHERE id = '{id}'"), "slug")
            .await?;
        Ok(format!("/api/v1/teams/{slug}"))
    }

    /// `/api/v1/teams/{team}/heaps/{heap}` for the heap with `heap_id`, from the slugs in the database.
    pub async fn heap_url(&self, heap_id: impl AsRef<str>) -> Result<String> {
        let id: uuid::Uuid = heap_id.as_ref().parse()?;
        let team_id = self
            .query_text(&format!("SELECT team_id FROM heaps WHERE id = '{id}'"), "team_id")
            .await?;
        let slug = self
            .query_text(&format!("SELECT slug FROM heaps WHERE id = '{id}'"), "slug")
            .await?;
        Ok(format!("{}/heaps/{slug}", self.team_url(&team_id).await?))
    }

    /// `…/items/{key}` for the item with `item_id`, from the handles in the database.
    pub async fn item_url(&self, item_id: impl AsRef<str>) -> Result<String> {
        let id: uuid::Uuid = item_id.as_ref().parse()?;
        let heap_id = self
            .query_text(&format!("SELECT heap_id FROM items WHERE id = '{id}'"), "heap_id")
            .await?;
        // `key` is reserved on MySQL, but not after a table name.
        let key = self
            .query_text(
                &format!("SELECT items.key AS item_key FROM items WHERE id = '{id}'"),
                "item_key",
            )
            .await?;
        Ok(format!("{}/items/{key}", self.heap_url(&heap_id).await?))
    }

    /// `/api/v1/teams/{team}/members/{username}` for `user_id` in the team with `team_id`.
    pub async fn member_url(&self, team_id: impl AsRef<str>, user_id: impl AsRef<str>) -> Result<String> {
        let id: uuid::Uuid = user_id.as_ref().parse()?;
        let username = self
            .query_text(
                &format!("SELECT slug FROM teams WHERE personal_user_id = '{id}'"),
                "slug",
            )
            .await?;
        Ok(format!("{}/members/{username}", self.team_url(team_id).await?))
    }

    /// The one text value `column` of the first row `sql` returns.
    async fn query_text(&self, sql: &str, column: &str) -> Result<String> {
        let row = self
            .db
            .query_one_raw(Statement::from_string(self.db.get_database_backend(), sql.to_owned()))
            .await?
            .with_context(|| format!("no row for {sql}"))?;
        Ok(row.try_get("", column)?)
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

    /// The run each audit entry about `entity_id` was made in, oldest first, as `(action, run ID)`.
    pub async fn audit_runs(&self, entity_id: &str) -> Result<Vec<(String, Option<String>)>> {
        let id: uuid::Uuid = entity_id.parse()?;
        let sql = format!("SELECT action, run_id FROM audit_log WHERE entity_id = '{id}' ORDER BY created_at, id");
        let rows = self
            .db
            .query_all_raw(Statement::from_string(self.db.get_database_backend(), sql))
            .await?;
        rows.into_iter()
            .map(|row| Ok((row.try_get("", "action")?, row.try_get("", "run_id")?)))
            .collect()
    }

    /// How many rows `table` holds. Read straight from the database, like [`TestApp::audit_entries`], so it sees rows
    /// no endpoint lists. `table` is spliced into the query, so pass a literal.
    pub async fn count_rows(&self, table: &str) -> Result<i64> {
        let sql = format!("SELECT COUNT(*) AS n FROM {table}");
        let row = self
            .db
            .query_one_raw(Statement::from_string(self.db.get_database_backend(), sql))
            .await?
            .context("no count row")?;
        Ok(row.try_get("", "n")?)
    }

    /// A new password reset link for the account `login` names, as the operator's command makes it, and returns its
    /// token: the last part of the link.
    pub async fn make_reset_link(&self, login: &str) -> Result<String> {
        let made = self.services.create_password_reset_link(login).await?;
        Ok(made.token.expose().to_owned())
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

/// `value` percent-encoded for a form body: everything but unreserved characters is escaped.
fn form_escape(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The username tests register with `email` under: its local part, lowercased, with anything but letters and digits
/// made a dash.
pub fn username_for(email: &str) -> String {
    let local = email.split('@').next().unwrap_or(email);
    local
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
