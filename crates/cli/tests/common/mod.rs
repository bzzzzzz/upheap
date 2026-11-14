//! Test harness: the real API router on in-memory SQLite (`store::testing`), served on a localhost port, and the CLI
//! run against it with an in-memory token store, a temporary config directory and captured output.
//!
//! Each test binary uses a different part of the harness, so unused items are expected.
#![allow(dead_code)]

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::process::ExitCode;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use api::{AppState, CookieSettings};
use cli::{
    AgentTools, Browser, Deps, Environment, Interrupt, MemoryTokenStore, StoredTokens, TokenStore, TokenStoreError,
    Tool, ToolError, ToolReport, ToolStart, WorkDeps,
};
use domain::sign_in::SignInLimit;
use domain::{Duration, Timestamp};
use serde_json::{Value, json};
use service::{ManualClock, RandomSource, Services, Settings, SystemRandom};
use tokio::sync::mpsc::UnboundedReceiver;

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

pub const HEAP: &str = "ada/personal";

/// Ada's personal heap with three open items, keyed in the order they were added: a small tagged one with a
/// description, a large one, and one without sizes.
pub async fn ada_with_items(server: &TestServer) -> Result<(TestUser, Vec<Value>)> {
    let ada = server.register("ada", "Ada").await?;
    let tag = server
        .send(
            &ada,
            "POST",
            "/teams/ada/heaps/personal/tags",
            Some(json!({ "name": "Infra" })),
        )
        .await?;
    let items = vec![
        server
            .add_item(
                &ada,
                HEAP,
                json!({
                    "title": "Rotate the staging certs", "description": "Before they expire",
                    "effort": "s", "impact": "l", "need": "critical", "tag_ids": [tag["id"]],
                }),
            )
            .await?,
        server
            .add_item(
                &ada,
                HEAP,
                json!({ "title": "Write the migration guide", "effort": "l", "impact": "m" }),
            )
            .await?,
        server
            .add_item(&ada, HEAP, json!({ "title": "Tidy the wiki", "need": "idea" }))
            .await?,
    ];
    Ok((ada, items))
}

pub fn key(item: &Value) -> Result<&str> {
    item["key"].as_str().context("item has a key")
}

/// The CLI signed in as `user`, working in Ada's personal heap through `UPHEAP_HEAP`.
pub async fn cli_in_heap(server: &TestServer, user: &TestUser) -> Result<TestCli> {
    let mut cli = TestCli::signed_in(server, user).await?;
    cli.env.heap = Some(HEAP.to_owned());
    Ok(cli)
}

/// The server, listening on a free localhost port until the test ends.
pub struct TestServer {
    /// `http://127.0.0.1:<port>`.
    pub url: String,
    http: reqwest::Client,
    clock: Arc<ManualClock>,
    /// How many heartbeats the server has heard: answered `204`.
    heard: Arc<AtomicUsize>,
}

/// A registered user, signed in to the web app.
pub struct TestUser {
    pub username: String,
    pub session: String,
}

/// Counts the heartbeats the server answers `204`, so a test can wait for one it has heard.
async fn count_heard_heartbeats(
    heard: Arc<AtomicUsize>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let is_heartbeat = request.method() == axum::http::Method::POST && request.uri().path().ends_with("/heartbeat");
    let response = next.run(request).await;
    if is_heartbeat && response.status() == axum::http::StatusCode::NO_CONTENT {
        heard.fetch_add(1, Ordering::SeqCst);
    }
    response
}

/// Stands in for Cloud Run's front end, which answers `411 Length Required` to a POST, PUT or PATCH that has neither
/// a `Content-Length` nor a `Transfer-Encoding` header, before the request reaches the server. The router alone
/// accepts one, so without this a client that forgot the length would pass every test and fail in production.
async fn require_length(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    use axum::http::{Method, StatusCode, header};
    use axum::response::IntoResponse;

    let writes = matches!(*request.method(), Method::POST | Method::PUT | Method::PATCH);
    let headers = request.headers();
    let has_length = headers.contains_key(header::CONTENT_LENGTH) || headers.contains_key(header::TRANSFER_ENCODING);
    if writes && !has_length {
        let page = "<!DOCTYPE html><html><body><h1>Error: Length Required</h1>\n<h2>POST requests require a Content-length header.</h2></body></html>";
        return (
            StatusCode::LENGTH_REQUIRED,
            [(header::CONTENT_TYPE, "text/html; charset=UTF-8")],
            page,
        )
            .into_response();
    }
    next.run(request).await
}

impl TestServer {
    pub async fn start() -> Result<Self> {
        Self::start_with_random(Arc::new(SystemRandom)).await
    }

    /// [`TestServer::start`], with item keys drawn from `random`.
    pub async fn start_with_random(random: Arc<dyn RandomSource>) -> Result<Self> {
        init_tracing();
        let db = store::testing::fresh_database().await?;
        let clock = Arc::new(ManualClock::new(
            Timestamp::from_millis(START).context("start in range")?,
        ));
        let services = Services::new(
            db,
            clock.clone(),
            Settings {
                allow_registration: true,
                session_ttl: Duration::from_days(30),
                sign_in_limit: SignInLimit::DEFAULT,
            },
        )
        .with_random(random);
        let router = api::router(AppState {
            services,
            cookies: CookieSettings {
                secure: false,
                max_age: std::time::Duration::from_secs(3600),
            },
        });
        let heard = Arc::new(AtomicUsize::new(0));
        let counted = heard.clone();
        let router = router
            .layer(axum::middleware::from_fn(move |request, next| {
                count_heard_heartbeats(counted.clone(), request, next)
            }))
            .layer(axum::middleware::from_fn(require_length));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        tokio::spawn(async move { axum::serve(listener, router).await });
        Ok(Self {
            url,
            http: local_client()?,
            clock,
            heard,
        })
    }

    /// How many heartbeats the server has heard so far.
    pub fn heard_heartbeats(&self) -> usize {
        self.heard.load(Ordering::SeqCst)
    }

    /// Moves the server's clock on by `by`. Tokens signed in before it may have expired by then.
    pub fn advance(&self, by: Duration) {
        self.clock.advance(by);
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
        // A write without a body still says so with `Content-Length: 0`, which the server's front end requires.
        request = match body {
            Some(body) => request.json(&body),
            None if method == "GET" => request,
            None => request.header("content-length", 0).body(Vec::new()),
        };
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

    /// Adds a parameter to `ada/personal` as `user`: its name, options in priority order and the IDs of the tags it
    /// is for. Returns it as the API has it.
    pub async fn add_parameter(
        &self,
        user: &TestUser,
        name: &str,
        options: &[&str],
        tag_ids: &[&Value],
    ) -> Result<Value> {
        self.send(
            user,
            "POST",
            "/teams/ada/heaps/personal/parameters",
            Some(json!({ "name": name, "options": options, "tag_ids": tag_ids })),
        )
        .await
    }

    /// The item with `key` in `heap` (`team/heap`), as `user` reads it.
    pub async fn read_item(&self, user: &TestUser, heap: &str, key: &str) -> Result<Value> {
        let (team, heap) = heap.split_once('/').context("heap is team/heap")?;
        self.send(user, "GET", &format!("/teams/{team}/heaps/{heap}/items/{key}"), None)
            .await
    }

    /// Changes the item with `key` in `heap` (`team/heap`) as `user`: sends `patch` with the item's current version.
    pub async fn edit_item(&self, user: &TestUser, heap: &str, key: &str, patch: Value) -> Result<Value> {
        let (team, heap_slug) = heap.split_once('/').context("heap is team/heap")?;
        let version = self.read_item(user, heap, key).await?["version"].to_string();
        self.send_with(
            user,
            "PATCH",
            &format!("/teams/{team}/heaps/{heap_slug}/items/{key}"),
            Some(patch),
            &[("if-match", &version)],
        )
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

/// An HTTP client for the test server, which ignores any proxy the environment names, as the CLI does for a server on
/// this machine. Built once and shared, as the CLI's is: each build reads the system's root certificates, which is slow
/// when many tests ask at once.
fn local_client() -> Result<reqwest::Client> {
    static SHARED: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(http) = SHARED.get() {
        return Ok(http.clone());
    }
    let http = reqwest::Client::builder().no_proxy().build()?;
    Ok(SHARED.get_or_init(|| http).clone())
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
    /// Everything written so far, left in place: for a test that watches a command that is still running.
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned()
    }

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
    pub tokens: Arc<TestKeychain>,
    pub config_dir: tempfile::TempDir,
    pub env: Environment,
    /// What the CLI reads from stdin; nothing by default.
    pub stdin: Vec<u8>,
    /// How often `work run` tells the server a run is alive. Long by default, so a run that has said it reached its
    /// goal is not mistaken for one taken back while its tool is still ending; a test of heartbeats shortens it.
    pub heartbeat_every: StdDuration,
    /// How often `work run --root` sweeps its root after the sweep it does when it starts. Long by default, so a
    /// looping test doesn't ask the server about every kept directory between claims; a test of that shortens it.
    pub sweep_every: StdDuration,
    /// How often `work run` looks at the clock for a heartbeat that is due, and waits to try a failed one again.
    pub heartbeat_check: StdDuration,
    /// Milliseconds the CLI's clock runs ahead of the real one, so a test can have the machine "sleep".
    pub clock_offset: Arc<AtomicI64>,
    browser: Arc<dyn Browser>,
}

impl TestCli {
    /// A machine whose `UPHEAP_SERVER` is `server`'s, with nobody signed in and no config file.
    pub fn new(server: &TestServer) -> Result<Self> {
        let config_dir = tempfile::tempdir()?;
        Ok(Self {
            tokens: Arc::new(TestKeychain::default()),
            env: Environment {
                server: Some(server.url.clone()),
                heap: None,
                run: None,
                no_color: false,
                stdout_is_terminal: false,
                stderr_is_terminal: false,
                time_zone: Some("Europe/Berlin".to_owned()),
                config_dir: Some(config_dir.path().to_path_buf()),
                hostname: Some("ada-laptop".to_owned()),
                now: jiff::Timestamp::now(),
            },
            config_dir,
            stdin: Vec::new(),
            heartbeat_every: StdDuration::from_secs(60),
            sweep_every: StdDuration::from_secs(60 * 60),
            heartbeat_check: StdDuration::from_millis(5),
            clock_offset: Arc::new(AtomicI64::new(0)),
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

    /// Runs `upheap <args>` with its stdout going to `stdout`. The outcome's `stdout` is empty. A machine with no
    /// agent tool.
    pub async fn run_into(&self, args: &[&str], stdout: Box<dyn Write + Send>) -> Outcome {
        let (_, no_interrupts) = tokio::sync::mpsc::unbounded_channel();
        let work = self.work_deps(&StandIn::not_installed(), no_interrupts);
        self.run_with(args, stdout, &Buffer::default(), work).await
    }

    /// [`TestCli::run_into`], with the agent tools, interruptions and waits `work` has, and a `stderr` the test
    /// keeps hold of. The outcome's `stderr` is what is left in it.
    pub async fn run_with(
        &self,
        args: &[&str],
        stdout: Box<dyn Write + Send>,
        stderr: &Buffer,
        work: WorkDeps,
    ) -> Outcome {
        let offset = self.clock_offset.clone();
        let deps = Deps {
            env: self.env.clone(),
            tokens: self.tokens.clone(),
            browser: Box::new(SharedBrowser(self.browser.clone())),
            stdin: Box::new(std::io::Cursor::new(self.stdin.clone())),
            stdout,
            stderr: Box::new(stderr.clone()),
            clock: Arc::new(move || {
                let ahead = jiff::SignedDuration::from_millis(offset.load(Ordering::SeqCst));
                jiff::Timestamp::now()
                    .checked_add(ahead)
                    .unwrap_or_else(|_| jiff::Timestamp::now())
            }),
            work,
        };
        let code = cli::run(std::iter::once("upheap").chain(args.iter().copied()), deps).await;
        Outcome {
            code,
            stdout: String::new(),
            stderr: stderr.take(),
        }
    }

    /// What `work run` takes from outside: `tools`, `interrupts`, and waits of a few milliseconds, so a test that
    /// waits for something does not wait long.
    pub fn work_deps(&self, tools: &StandIn, interrupts: UnboundedReceiver<Interrupt>) -> WorkDeps {
        WorkDeps {
            tools: Box::new(tools.clone()),
            interrupts: Box::new(move || interrupts),
            idle_wait: StdDuration::from_millis(20),
            heartbeat_every: self.heartbeat_every,
            heartbeat_check: self.heartbeat_check,
            finish_retry: StdDuration::from_millis(5),
            sweep_every: self.sweep_every,
        }
    }

    /// Runs `upheap <args>` with `tools` as the agent tool, and nobody interrupting.
    pub async fn run_work(&self, args: &[&str], tools: &StandIn) -> Outcome {
        let (_, no_interrupts) = tokio::sync::mpsc::unbounded_channel();
        let (stdout, stderr) = (Buffer::default(), Buffer::default());
        self.run_work_watched(args, tools, no_interrupts, &stdout, &stderr)
            .await
    }

    /// [`TestCli::run_work`], with `interrupts`, `stdout` and `stderr` that the test keeps hold of: it can read what
    /// the command has printed so far while it runs.
    pub async fn run_work_watched(
        &self,
        args: &[&str],
        tools: &StandIn,
        interrupts: UnboundedReceiver<Interrupt>,
        stdout: &Buffer,
        stderr: &Buffer,
    ) -> Outcome {
        let work = self.work_deps(tools, interrupts);
        let mut outcome = self.run_with(args, Box::new(stdout.clone()), stderr, work).await;
        outcome.stdout = stdout.take();
        outcome
    }

    /// The hands of an agent on this machine: the same keychain, config and server, to run `upheap` as a run does.
    pub fn agent(&self) -> Agent {
        Agent {
            tokens: self.tokens.clone(),
            env: self.env.clone(),
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

/// A keychain in memory that can also hold an entry the CLI didn't write, as another program, or an older or newer
/// `upheap`, might leave.
#[derive(Default)]
pub struct TestKeychain {
    tokens: MemoryTokenStore,
    unreadable: Mutex<std::collections::HashSet<String>>,
    /// When set, saving fails, as a locked or full keychain would, and the tokens it was asked to keep are recorded.
    refusing: Mutex<Option<Vec<StoredTokens>>>,
    /// How many times the tokens were read.
    loads: AtomicUsize,
}

impl TestKeychain {
    /// How many times the tokens were read so far: each command that makes a request reads them at least once.
    pub fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }

    /// Puts an entry for `server` that doesn't parse in place of its sign-in.
    pub fn plant_unreadable(&self, server: &str) -> Result<()> {
        self.tokens.delete(server)?;
        self.unreadable().insert(server.to_owned());
        Ok(())
    }

    /// Makes every save fail from now on.
    pub fn refuse_saves(&self) {
        *self.refusing() = Some(Vec::new());
    }

    /// The tokens saves were refused for, oldest first.
    pub fn refused(&self) -> Vec<StoredTokens> {
        self.refusing().clone().unwrap_or_default()
    }

    fn refusing(&self) -> std::sync::MutexGuard<'_, Option<Vec<StoredTokens>>> {
        self.refusing.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn unreadable(&self) -> std::sync::MutexGuard<'_, std::collections::HashSet<String>> {
        self.unreadable
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl TokenStore for TestKeychain {
    fn load(&self, server: &str) -> Result<Option<StoredTokens>, TokenStoreError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        if self.unreadable().contains(server) {
            return Err(TokenStoreError::Corrupt {
                server: server.to_owned(),
            });
        }
        self.tokens.load(server)
    }

    fn save(&self, server: &str, tokens: &StoredTokens) -> Result<(), TokenStoreError> {
        if let Some(refused) = self.refusing().as_mut() {
            refused.push(tokens.clone());
            return Err(TokenStoreError::Unavailable("the keychain is locked".into()));
        }
        self.unreadable().remove(server);
        self.tokens.save(server, tokens)
    }

    fn delete(&self, server: &str) -> Result<(), TokenStoreError> {
        self.unreadable().remove(server);
        self.tokens.delete(server)
    }
}

/// Waits, for at most ten seconds, until `condition` holds.
pub async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) -> Result<()> {
    let waited = tokio::time::timeout(StdDuration::from_secs(10), async {
        while !condition() {
            tokio::time::sleep(StdDuration::from_millis(5)).await;
        }
    })
    .await;
    anyhow::ensure!(waited.is_ok(), "timed out waiting for {what}");
    Ok(())
}

type AgentRun = Pin<Box<dyn Future<Output = Result<ToolReport, ToolError>> + Send>>;

/// An agent tool for tests: what `script` does with each run plays the agent, usually by running `upheap` through an
/// [`Agent`]. Keeps what it was started with and counts the runs it was stopped in.
#[derive(Clone)]
pub struct StandIn {
    script: Arc<dyn Fn(ToolStart) -> AgentRun + Send + Sync>,
    installed: bool,
    starts: Arc<Mutex<Vec<ToolStart>>>,
    stopped: Arc<AtomicUsize>,
    /// When set, [`AgentTools::settled`] waits for a permit, as a tool still ending would hold it up.
    settle_gate: Option<Arc<tokio::sync::Semaphore>>,
    settles_asked: Arc<AtomicUsize>,
}

impl StandIn {
    /// A tool that plays `script` for each run.
    pub fn new<F, Fut>(script: F) -> Self
    where
        F: Fn(ToolStart) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolReport, ToolError>> + Send + 'static,
    {
        Self {
            script: Arc::new(move |start| Box::pin(script(start))),
            installed: true,
            starts: Arc::default(),
            stopped: Arc::default(),
            settle_gate: None,
            settles_asked: Arc::default(),
        }
    }

    /// A tool whose stopped runs take as long to be gone as `gate` has no permits: [`AgentTools::settled`] waits for
    /// one. Without a gate it is ready at once.
    pub fn with_settle_gate(mut self, gate: Arc<tokio::sync::Semaphore>) -> Self {
        self.settle_gate = Some(gate);
        self
    }

    /// How many times the loop asked whether the stopped tools are gone.
    pub fn settles_asked(&self) -> usize {
        self.settles_asked.load(Ordering::SeqCst)
    }

    /// A machine where the tool isn't installed.
    pub fn not_installed() -> Self {
        Self {
            installed: false,
            ..Self::new(|_| async { Ok(ToolReport::default()) })
        }
    }

    /// What each run started it with, oldest first.
    pub fn starts(&self) -> Vec<ToolStart> {
        self.starts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// How many runs ended by dropping the tool's future before it was done.
    pub fn stopped(&self) -> usize {
        self.stopped.load(Ordering::SeqCst)
    }
}

impl AgentTools for StandIn {
    fn check(&self, tool: Tool) -> Result<(), ToolError> {
        if self.installed {
            Ok(())
        } else {
            Err(ToolError::NotInstalled { tool })
        }
    }

    fn settled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.settles_asked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(gate) = &self.settle_gate {
                // The permit goes back when it is dropped, so an open gate stays open.
                let _ = gate.acquire().await;
            }
        })
    }

    fn run(&self, start: ToolStart) -> AgentRun {
        self.starts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(start.clone());
        let playing = (self.script)(start);
        let stopped = self.stopped.clone();
        Box::pin(async move {
            // Counts the run if the future is dropped before it is done.
            struct CountIfDropped(Arc<AtomicUsize>, bool);
            impl Drop for CountIfDropped {
                fn drop(&mut self) {
                    if !self.1 {
                        self.0.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
            let mut guard = CountIfDropped(stopped, false);
            let report = playing.await;
            guard.1 = true;
            report
        })
    }
}

/// The hands of an agent: runs `upheap` as a run does, with the run, server and heap its tool was started with.
#[derive(Clone)]
pub struct Agent {
    tokens: Arc<TestKeychain>,
    env: Environment,
}

impl Agent {
    /// Runs `upheap <args>` in the run `start` belongs to.
    pub async fn upheap(&self, start: &ToolStart, args: &[&str]) -> Outcome {
        self.upheap_with_stdin(start, args, "").await
    }

    /// [`Agent::upheap`], with `stdin` for a `-` file.
    pub async fn upheap_with_stdin(&self, start: &ToolStart, args: &[&str], stdin: &str) -> Outcome {
        let var = |name: &str| {
            start
                .env
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        let mut env = self.env.clone();
        env.run = var("UPHEAP_RUN");
        env.server = var("UPHEAP_SERVER");
        env.heap = var("UPHEAP_HEAP");
        let (stdout, stderr) = (Buffer::default(), Buffer::default());
        let (_, no_interrupts) = tokio::sync::mpsc::unbounded_channel();
        let deps = Deps {
            env,
            tokens: self.tokens.clone(),
            browser: Box::new(NoBrowser),
            stdin: Box::new(std::io::Cursor::new(stdin.as_bytes().to_vec())),
            stdout: Box::new(stdout.clone()),
            stderr: Box::new(stderr.clone()),
            clock: Arc::new(jiff::Timestamp::now),
            work: WorkDeps {
                tools: Box::new(StandIn::not_installed()),
                interrupts: Box::new(move || no_interrupts),
                idle_wait: StdDuration::from_millis(1),
                heartbeat_every: StdDuration::from_millis(1),
                heartbeat_check: StdDuration::from_millis(1),
                finish_retry: StdDuration::from_millis(1),
                sweep_every: StdDuration::ZERO,
            },
        };
        let code = cli::run(std::iter::once("upheap").chain(args.iter().copied()), deps).await;
        Outcome {
            code,
            stdout: stdout.take(),
            stderr: stderr.take(),
        }
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
    let http = local_client()?;
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
