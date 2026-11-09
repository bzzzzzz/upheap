//! `upheap`, the command-line client of Upheap: list a heap's ranked items, pick a default heap, and sign in.
//!
//! It is a thin client of the HTTP API, like the web app: it never ranks, checks permissions or works out labels,
//! and it reads and writes the API's own types from `api_types`. See design/cli.md.
//!
//! The binary gathers its environment once ([`Deps::system`]) and calls [`run`]; tests call [`run`] with their own
//! [`Deps`]: an in-memory token store, a browser that answers the consent page itself, and buffers for the output.

mod auth;
mod client;
mod commands;
mod config;
mod error;
mod lookup;
mod output;

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use reqwest::Url;

pub use crate::auth::{KeychainStore, MemoryTokenStore, StoredTokens, TokenStore, TokenStoreError};
use crate::client::{ApiClient, ClientError};
use crate::commands::SharedArgs;
use crate::config::{ConfigError, ConfigFile, SettingError};
pub use crate::error::CliError;
use crate::output::{Mode, Output};

/// Everything a run of the CLI takes from outside: the environment, where tokens are kept, the browser, and the two
/// output streams.
pub struct Deps {
    pub env: Environment,
    pub tokens: Arc<dyn TokenStore>,
    pub browser: Box<dyn Browser>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
}

/// The environment, read once at startup.
#[derive(Clone, Debug)]
pub struct Environment {
    /// `UPHEAP_SERVER`.
    pub server: Option<String>,
    /// `UPHEAP_HEAP`.
    pub heap: Option<String>,
    /// Whether `NO_COLOR` is set to something.
    pub no_color: bool,
    pub stdout_is_terminal: bool,
    pub stderr_is_terminal: bool,
    /// The machine's IANA timezone, sent as `Time-Zone`. `None` when it can't be told, and the server uses UTC.
    pub time_zone: Option<String>,
    /// Where the config file and the token lock are: `upheap` in the platform's config directory. `None` on a
    /// platform without one.
    pub config_dir: Option<PathBuf>,
    /// When the command started.
    pub now: jiff::Timestamp,
}

/// Opens a page in the user's browser.
pub trait Browser: Send + Sync {
    fn open(&self, url: &str) -> std::io::Result<()>;
}

/// The system's default browser.
pub struct SystemBrowser;

impl Browser for SystemBrowser {
    fn open(&self, url: &str) -> std::io::Result<()> {
        webbrowser::open(url)
    }
}

impl Environment {
    /// Reads the process's environment, the system timezone, the clock and the config directory.
    pub fn read() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            server: var("UPHEAP_SERVER"),
            heap: var("UPHEAP_HEAP"),
            no_color: var("NO_COLOR").is_some(),
            stdout_is_terminal: std::io::stdout().is_terminal(),
            stderr_is_terminal: std::io::stderr().is_terminal(),
            time_zone: jiff::tz::TimeZone::system().iana_name().map(str::to_owned),
            config_dir: dirs::config_dir().map(|dir| dir.join("upheap")),
            now: jiff::Timestamp::now(),
        }
    }
}

impl Deps {
    /// The real thing: the process's environment, the OS keychain, the default browser, stdout and stderr.
    pub fn system() -> Self {
        Self {
            env: Environment::read(),
            tokens: Arc::new(KeychainStore),
            browser: Box::new(SystemBrowser),
            stdout: Box::new(std::io::stdout()),
            stderr: Box::new(std::io::stderr()),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "upheap",
    version,
    about = "Upheap from the terminal: a heap's ranked items, heaps, and signing in."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Items in a heap
    #[command(subcommand)]
    Item(ItemCommand),
    /// Your heaps, and the default one
    #[command(subcommand)]
    Heap(HeapCommand),
    /// Signing in and out
    #[command(subcommand)]
    Auth(AuthCommand),
}

#[derive(Subcommand)]
enum ItemCommand {
    /// The ranked list, or closed items with --closed
    List(commands::item::list::ListArgs),
}

#[derive(Subcommand)]
enum HeapCommand {
    /// Every heap you can use, by team
    List(commands::heap::list::ListArgs),
    /// Make a heap the default for commands that work in one
    Use(commands::heap::use_heap::UseArgs),
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Sign in through the browser
    Login(commands::auth::login::LoginArgs),
    /// Sign out, ending the sign-in on the server too
    Logout(commands::auth::logout::LogoutArgs),
}

impl Command {
    fn shared(&self) -> &SharedArgs {
        match self {
            Self::Item(ItemCommand::List(args)) => &args.shared,
            Self::Heap(HeapCommand::List(args)) => &args.shared,
            Self::Heap(HeapCommand::Use(args)) => &args.shared,
            Self::Auth(AuthCommand::Login(args)) => &args.shared,
            Self::Auth(AuthCommand::Logout(args)) => &args.shared,
        }
    }
}

/// Runs the command `args` (the program name first) with `deps`, and returns its exit code.
pub async fn run<I, T>(args: I, deps: Deps) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let Deps {
        env,
        tokens,
        browser,
        mut stdout,
        mut stderr,
    } = deps;
    let color = env.stdout_is_terminal && !env.no_color;
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            let text = if color {
                err.render().ansi().to_string()
            } else {
                err.render().to_string()
            };
            let stream = if err.use_stderr() { &mut stderr } else { &mut stdout };
            // Nowhere left to report a failure to write the usage.
            let _ = write!(stream, "{text}");
            return ExitCode::from(u8::try_from(err.exit_code()).unwrap_or(2));
        }
    };
    let shared = cli.command.shared();
    let mode = if shared.json {
        Mode::Json
    } else if shared.quiet {
        Mode::Quiet
    } else {
        Mode::Human
    };
    let mut out = Output::new(stdout, stderr, mode, color).on_terminals(env.stdout_is_terminal, env.stderr_is_terminal);
    let result = match load_app(env, tokens, browser) {
        Ok(app) => dispatch(cli.command, &app, &mut out).await,
        Err(err) => Err(err.into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // The reader went away, as `| head` does once it has its lines: nothing more to say, and nobody to say it to.
        Err(CliError::Output(err)) if err.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(err) => {
            report(&err, &mut out);
            ExitCode::from(err.exit_code())
        }
    }
}

fn load_app(env: Environment, tokens: Arc<dyn TokenStore>, browser: Box<dyn Browser>) -> Result<App, ConfigError> {
    let config = match &env.config_dir {
        Some(dir) => config::load_config(dir)?,
        None => ConfigFile::default(),
    };
    Ok(App {
        env,
        tokens,
        browser,
        config,
    })
}

async fn dispatch(command: Command, app: &App, out: &mut Output) -> Result<(), CliError> {
    match command {
        Command::Item(ItemCommand::List(args)) => commands::item::list::run(args, app, out).await,
        Command::Heap(HeapCommand::List(args)) => commands::heap::list::run(args, app, out).await,
        Command::Heap(HeapCommand::Use(args)) => commands::heap::use_heap::run(args, app, out).await,
        Command::Auth(AuthCommand::Login(args)) => commands::auth::login::run(args, app, out).await,
        Command::Auth(AuthCommand::Logout(args)) => commands::auth::logout::run(args, app, out).await,
    }
}

/// Writes `err` to stderr: the server's problem body as it came with `--json`, a message otherwise.
fn report(err: &CliError, out: &mut Output) {
    if out.mode() == Mode::Json
        && let Some(body) = err.problem_body()
    {
        let _ = out.json_note(body);
        return;
    }
    for line in err.to_message() {
        // Nowhere left to report a failure to write to stderr.
        let _ = out.note(&line);
    }
}

/// What every command works with: the environment, the config file as it was at startup, the token store and the
/// browser.
pub(crate) struct App {
    pub env: Environment,
    pub tokens: Arc<dyn TokenStore>,
    pub browser: Box<dyn Browser>,
    pub config: ConfigFile,
}

impl App {
    pub fn config_dir(&self) -> Result<&Path, ConfigError> {
        self.env.config_dir.as_deref().ok_or(ConfigError::NoConfigDir)
    }

    /// The server `shared` names, or the environment or config file does.
    pub fn server(&self, shared: &SharedArgs) -> Result<Url, SettingError> {
        config::resolve_server(
            shared.server.as_deref(),
            self.env.server.as_deref(),
            self.config.server.as_deref(),
        )
    }

    /// The heap `flag` names, or the environment or config file does, as typed.
    pub fn heap_name<'a>(&'a self, flag: Option<&'a str>) -> Option<&'a str> {
        config::resolve_heap(flag, self.env.heap.as_deref(), self.config.heap.as_deref()).ok()
    }

    pub fn client(&self, server: Url) -> Result<ApiClient, ClientError> {
        ApiClient::new(server, self.env.time_zone.clone())
    }

    /// An access token for `client`'s server, refreshed when the stored one is stale.
    pub async fn access_token(&self, client: &ApiClient) -> Result<String, CliError> {
        let server = config::server_key(client.server());
        let token = auth::access_token(
            &self.tokens,
            self.config_dir()?,
            client,
            &server,
            self.env.now.as_second(),
        )
        .await?;
        Ok(token)
    }
}
