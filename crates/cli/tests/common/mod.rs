//! Test harness: the real API router on in-memory SQLite (`store::testing`), served on a localhost port, and the CLI
//! run against it with an in-memory token store, a temporary config directory and captured output.
//!
//! Each test binary uses a different part of the harness, so unused items are expected.
#![allow(dead_code)]

use std::io::Write;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use api::{AppState, Authentication, CookieSettings};
use cli::{Browser, Deps, Environment, MemoryTokenStore, StoredTokens, TokenStore};
use domain::sign_in::SignInLimit;
use domain::{Duration, Timestamp};
use serde_json::{Value, json};
use service::{ManualClock, Services, Settings};

/// 2027-01-01T12:00:00Z, the server's clock.
pub const START: i64 = 1_798_804_800_000;
const CSRF: (&str, &str) = ("x-csrf-protection", "1");

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

/// The server, listening on a free localhost port until the test ends.
pub struct TestServer {
    /// `http://127.0.0.1:<port>`.
    pub url: String,
    http: reqwest::Client,
}

/// A registered user, signed in to the web app.
pub struct TestUser {
    pub username: String,
    pub session: String,
}

impl TestServer {
    pub async fn start() -> Result<Self> {
        init_tracing();
        let db = store::testing::fresh_database().await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(
            db,
            clock,
            Settings {
                allow_registration: true,
                session_ttl: Duration::from_days(30),
                sign_in_limit: SignInLimit::DEFAULT,
            },
        );
        let router = api::router(AppState {
            services,
            authentication: Authentication::Sessions(CookieSettings {
                secure: false,
                max_age: std::time::Duration::from_secs(3600),
            }),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            url,
            http: reqwest::Client::new(),
        })
    }

    /// Registers `username` with the display name `display_name`, and signs them in to the web app.
    pub async fn register(&self, username: &str, display_name: &str) -> Result<TestUser> {
        let response = self
            .http
            .post(format!("{}/api/v1/auth/register", self.url))
            .header(CSRF.0, CSRF.1)
            .json(&json!({
                "email": format!("{username}@example.test"),
                "username": username,
                "display_name": display_name,
                "password": "a long test password",
            }))
            .send()
            .await?;
        anyhow::ensure!(response.status() == 201, "register failed: {}", response.text().await?);
        let session = response
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("heap_session="))
            .and_then(|v| v.split(';').next())
            .context("no session cookie")?
            .to_owned();
        Ok(TestUser {
            username: username.to_owned(),
            session,
        })
    }

    /// Sends a request to `/api/v1<path>` as `user`'s web session, and returns the JSON body (`Null` when empty).
    /// Fails unless the answer is a success.
    pub async fn send(&self, user: &TestUser, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        self.send_with(user, method, path, body, &[]).await
    }

    pub async fn send_with(
        &self,
        user: &TestUser,
        method: &str,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Result<Value> {
        let mut request = self
            .http
            .request(method.parse()?, format!("{}/api/v1{path}", self.url))
            .header(CSRF.0, CSRF.1)
            .header("cookie", format!("heap_session={}", user.session));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        anyhow::ensure!(status.is_success(), "{method} {path} failed with {status}: {text}");
        Ok(if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text)?
        })
    }

    /// Adds an item to `heap` (`team/heap`) as `user`, and returns it.
    pub async fn add_item(&self, user: &TestUser, heap: &str, body: Value) -> Result<Value> {
        let (team, heap) = heap.split_once('/').context("heap is team/heap")?;
        let mut body = body;
        if let Value::Object(fields) = &mut body {
            fields.entry("need").or_insert_with(|| Value::from("expected"));
        }
        self.send(user, "POST", &format!("/teams/{team}/heaps/{heap}/items"), Some(body))
            .await
    }

    /// Moves `item` (as the API returned it) of `heap` to `state` as `user`.
    pub async fn set_state(&self, user: &TestUser, heap: &str, item: &Value, state: &str) -> Result<Value> {
        let (team, heap) = heap.split_once('/').context("heap is team/heap")?;
        let key = item["key"].as_str().context("item has a key")?;
        let version = item["version"].to_string();
        self.send_with(
            user,
            "PATCH",
            &format!("/teams/{team}/heaps/{heap}/items/{key}"),
            Some(json!({ "state": state })),
            &[("if-match", &version)],
        )
        .await
    }

    /// Signs the CLI in as `user` the way the consent page does: approve, then exchange the code. Returns the
    /// tokens as the CLI stores them.
    pub async fn sign_in_cli(&self, user: &TestUser) -> Result<StoredTokens> {
        let redirect_uri = "http://127.0.0.1:9/callback";
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let approved = self
            .send(
                user,
                "POST",
                "/oauth/authorize",
                Some(json!({
                    "response_type": "code",
                    "client_id": "upheap-cli",
                    "redirect_uri": redirect_uri,
                    "code_challenge": "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
                    "code_challenge_method": "S256",
                    "state": "s",
                })),
            )
            .await?;
        let redirect = approved["redirect_to"].as_str().context("no redirect_to")?;
        let code = query_value(redirect, "code").context("no code")?;
        let tokens = self
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("client_id", "upheap-cli"),
                ("code", &code),
                ("redirect_uri", redirect_uri),
                ("code_verifier", verifier),
            ])
            .await?;
        anyhow::ensure!(tokens.0 == 200, "token exchange failed: {}", tokens.1);
        Ok(StoredTokens {
            refresh_token: tokens.1["refresh_token"].as_str().context("refresh token")?.to_owned(),
            access_token: tokens.1["access_token"].as_str().context("access token")?.to_owned(),
            access_expires_at: jiff::Timestamp::now().as_second() + 900,
        })
    }

    /// Posts `form` to `/api/v1/oauth/token`, and returns the status and body.
    pub async fn token_request(&self, form: &[(&str, &str)]) -> Result<(u16, Value)> {
        let response = self
            .http
            .post(format!("{}/api/v1/oauth/token", self.url))
            .form(form)
            .send()
            .await?;
        let status = response.status().as_u16();
        Ok((status, response.json().await?))
    }

    /// The status `GET /api/v1/me` answers with `access_token` as its bearer.
    pub async fn me_status(&self, access_token: &str) -> Result<u16> {
        let response = self
            .http
            .get(format!("{}/api/v1/me", self.url))
            .bearer_auth(access_token)
            .send()
            .await?;
        Ok(response.status().as_u16())
    }
}

/// The value of `name` in `url`'s query.
pub fn query_value(url: &str, name: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(query).ok()?;
    pairs.into_iter().find(|(key, _)| key == name).map(|(_, value)| value)
}

/// Captured output.
#[derive(Clone, Default)]
pub struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    fn take(&self) -> String {
        let mut bytes = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        String::from_utf8_lossy(&std::mem::take(&mut *bytes)).into_owned()
    }
}

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What a run of the CLI did.
#[derive(Debug)]
pub struct Outcome {
    pub code: ExitCode,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    pub fn exited(&self, code: u8) -> bool {
        self.code == ExitCode::from(code)
    }
}

/// One machine with the CLI on it: its keychain, config directory and environment, kept across runs.
pub struct TestCli {
    pub tokens: Arc<MemoryTokenStore>,
    pub config_dir: tempfile::TempDir,
    pub env: Environment,
    browser: Arc<dyn Browser>,
}

impl TestCli {
    /// A machine whose `UPHEAP_SERVER` is `server`'s, with nobody signed in and no config file.
    pub fn new(server: &TestServer) -> Result<Self> {
        let config_dir = tempfile::tempdir()?;
        Ok(Self {
            tokens: Arc::new(MemoryTokenStore::default()),
            env: Environment {
                server: Some(server.url.clone()),
                heap: None,
                no_color: false,
                stdout_is_terminal: false,
                time_zone: Some("Europe/Berlin".to_owned()),
                config_dir: Some(config_dir.path().to_path_buf()),
                now: jiff::Timestamp::now(),
            },
            config_dir,
            browser: Arc::new(NoBrowser),
        })
    }

    /// A machine with `user` signed in to `server`.
    pub async fn signed_in(server: &TestServer, user: &TestUser) -> Result<Self> {
        let cli = Self::new(server)?;
        cli.tokens.save(&server.url, &server.sign_in_cli(user).await?)?;
        Ok(cli)
    }

    /// Uses `browser` for `upheap auth login`.
    pub fn with_browser(mut self, browser: impl Browser + 'static) -> Self {
        self.browser = Arc::new(browser);
        self
    }

    /// Runs `upheap <args>`.
    pub async fn run(&self, args: &[&str]) -> Outcome {
        let stdout = Buffer::default();
        let mut outcome = self.run_into(args, Box::new(stdout.clone())).await;
        outcome.stdout = stdout.take();
        outcome
    }

    /// Runs `upheap <args>` with its stdout going to `stdout`. The outcome's `stdout` is empty.
    pub async fn run_into(&self, args: &[&str], stdout: Box<dyn Write + Send>) -> Outcome {
        let stderr = Buffer::default();
        let deps = Deps {
            env: self.env.clone(),
            tokens: self.tokens.clone(),
            browser: Box::new(SharedBrowser(self.browser.clone())),
            stdout,
            stderr: Box::new(stderr.clone()),
        };
        let code = cli::run(std::iter::once("upheap").chain(args.iter().copied()), deps).await;
        Outcome {
            code,
            stdout: String::new(),
            stderr: stderr.take(),
        }
    }

    /// The config file's text, empty when there's none.
    pub fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_dir.path().join("config.toml")).unwrap_or_default()
    }

    /// The sign-in stored for `server`.
    pub fn stored(&self, server: &TestServer) -> Result<Option<StoredTokens>> {
        Ok(self.tokens.load(&server.url)?)
    }
}

struct SharedBrowser(Arc<dyn Browser>);

impl Browser for SharedBrowser {
    fn open(&self, url: &str) -> std::io::Result<()> {
        self.0.open(url)
    }
}

/// A pipe whose reader has gone, as `upheap … | head -1` leaves it after the first line.
pub struct ClosedPipe;

impl Write for ClosedPipe {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}

/// A machine with no browser: login has to be finished by visiting the printed address.
struct NoBrowser;

impl Browser for NoBrowser {
    fn open(&self, _: &str) -> std::io::Result<()> {
        Err(std::io::Error::other("no browser"))
    }
}

/// A browser in which `user` is signed in to the web app and answers the consent page: Allow, or Cancel.
pub struct ConsentBrowser {
    pub server: String,
    pub session: String,
    pub allow: bool,
    /// Comes back to the redirect URI first with another sign-in's `state`, as a stale tab might.
    pub wrong_state_first: bool,
}

impl ConsentBrowser {
    /// A browser where `user` allows the CLI.
    pub fn allowing(server: &TestServer, user: &TestUser) -> Self {
        Self {
            server: server.url.clone(),
            session: user.session.clone(),
            allow: true,
            wrong_state_first: false,
        }
    }
}

impl Browser for ConsentBrowser {
    fn open(&self, url: &str) -> std::io::Result<()> {
        let (server, session, url) = (self.server.clone(), self.session.clone(), url.to_owned());
        let (allow, wrong_state_first) = (self.allow, self.wrong_state_first);
        // The CLI is waiting on its loopback port by the time the browser comes back, as with a real browser.
        tokio::spawn(async move {
            if let Err(err) = consent(&server, &session, allow, wrong_state_first, &url).await {
                tracing::error!(error = %err, "the consent page failed");
            }
        });
        Ok(())
    }
}

/// What the consent page does: check the request, then approve it and follow `redirect_to`, or send the browser to
/// the redirect URI with `error=access_denied`.
async fn consent(server: &str, session: &str, allow: bool, wrong_state_first: bool, url: &str) -> Result<()> {
    let page = format!("{server}/oauth/authorize?");
    let query = url.strip_prefix(&page).context("not the consent page")?;
    let request: std::collections::BTreeMap<String, String> = serde_urlencoded::from_str(query)?;
    let http = reqwest::Client::new();
    if wrong_state_first {
        let redirect_uri = request.get("redirect_uri").context("no redirect_uri")?;
        let turned_away = http
            .get(format!("{redirect_uri}?code=not-this-one&state=another-sign-in"))
            .send()
            .await?
            .text()
            .await?;
        anyhow::ensure!(
            turned_away.contains("wasn't signed in"),
            "wrong state accepted: {turned_away}"
        );
    }
    let checked = http
        .get(format!("{server}/api/v1/oauth/authorize?{query}"))
        .send()
        .await?;
    anyhow::ensure!(checked.status() == 200, "check failed: {}", checked.text().await?);
    let redirect_to = if allow {
        let approved: Value = http
            .post(format!("{server}/api/v1/oauth/authorize"))
            .header(CSRF.0, CSRF.1)
            .header("cookie", format!("heap_session={session}"))
            .json(&request)
            .send()
            .await?
            .json()
            .await?;
        approved["redirect_to"].as_str().context("no redirect_to")?.to_owned()
    } else {
        let state = request.get("state").context("no state")?;
        let redirect_uri = request.get("redirect_uri").context("no redirect_uri")?;
        format!("{redirect_uri}?error=access_denied&state={state}")
    };
    http.get(redirect_to).send().await?;
    Ok(())
}
