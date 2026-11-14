//! `upheap`, the command-line client of Upheap: add, show, edit, move and list a heap's items, list, add and edit its
//! tags, add a heap, pick a default one, sign in, and print a guide to all of it.
//!
//! It is a thin client of the HTTP API, like the web app: it never ranks, checks permissions or works out labels,
//! and it reads and writes the API's own types from `api_types`.
//!
//! The binary gathers its environment once ([`Deps::system`]) and calls [`run`]; tests call [`run`] with their own
//! [`Deps`]: an in-memory token store, a browser that answers the consent page itself, and buffers for the output.

mod agent;
mod auth;
mod client;
mod commands;
mod config;
mod error;
mod lookup;
mod output;

use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use clap::{CommandFactory, Parser, Subcommand};
use reqwest::Url;

pub use crate::agent::{
    Access, AgentTools, Interrupt, ListenForInterrupts, SystemTools, Tool, ToolError, ToolReport, ToolStart, WorkDeps,
};
pub use crate::auth::{KeychainStore, MemoryTokenStore, StoredTokens, TokenStore, TokenStoreError};
pub use crate::client::HttpClients;
use crate::client::{ApiClient, ClientError};
use crate::commands::{SharedArgs, UsageError};
use crate::config::{ConfigError, ConfigFile, SettingError};
pub use crate::error::CliError;
use crate::output::{Mode, Output};

/// Tells the time now.
pub type Clock = Arc<dyn Fn() -> jiff::Timestamp + Send + Sync>;

/// Everything a run of the CLI takes from outside: the environment, where tokens are kept, the browser, stdin, and
/// the two output streams.
pub struct Deps {
    pub env: Environment,
    pub tokens: Arc<dyn TokenStore>,
    pub browser: Box<dyn Browser>,
    /// Read only by `--description -` and `--context-file -`.
    pub stdin: Box<dyn Read + Send>,
    pub stdout: Box<dyn Write + Send>,
    pub stderr: Box<dyn Write + Send>,
    /// The time now. Read whenever a token's freshness is judged or a time is printed in a command that runs long.
    pub clock: Clock,
    /// What `upheap work run` takes from outside: the agent tools, interruptions and waits.
    pub work: WorkDeps,
    /// The HTTP clients every request goes out on.
    pub http: HttpClients,
}

/// The environment, read once at startup.
#[derive(Clone, Debug)]
pub struct Environment {
    /// `UPHEAP_SERVER`.
    pub server: Option<String>,
    /// `UPHEAP_HEAP`.
    pub heap: Option<String>,
    /// `UPHEAP_RUN`: the run an agent writes in. Sent as `Upheap-Run` with every request.
    pub run: Option<String>,
    /// Whether `NO_COLOR` is set to something.
    pub no_color: bool,
    pub stdout_is_terminal: bool,
    pub stderr_is_terminal: bool,
    /// The machine's IANA timezone, sent as `Time-Zone`. `None` when it can't be told, and the server uses UTC.
    pub time_zone: Option<String>,
    /// Where the config file and the token lock are: `upheap` in the platform's config directory. `None` on a
    /// platform without one.
    pub config_dir: Option<PathBuf>,
    /// The machine's hostname, which names the sign-in. `None` when it can't be read, and the sign-in has no name.
    pub hostname: Option<String>,
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
    /// Reads the process's environment, the system timezone and hostname, the clock and the config directory.
    pub fn read() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            server: var("UPHEAP_SERVER"),
            heap: var("UPHEAP_HEAP"),
            run: var("UPHEAP_RUN"),
            no_color: var("NO_COLOR").is_some(),
            stdout_is_terminal: std::io::stdout().is_terminal(),
            stderr_is_terminal: std::io::stderr().is_terminal(),
            time_zone: jiff::tz::TimeZone::system().iana_name().map(str::to_owned),
            config_dir: dirs::config_dir().map(|dir| dir.join("upheap")),
            hostname: whoami::hostname().ok(),
            now: jiff::Timestamp::now(),
        }
    }
}

impl Deps {
    /// The real thing: the process's environment, the OS keychain, the default browser, stdin, stdout and stderr.
    pub fn system() -> Self {
        Self {
            env: Environment::read(),
            tokens: Arc::new(KeychainStore),
            browser: Box::new(SystemBrowser),
            stdin: Box::new(std::io::stdin()),
            stdout: Box::new(std::io::stdout()),
            stderr: Box::new(std::io::stderr()),
            clock: Arc::new(jiff::Timestamp::now),
            work: WorkDeps::system(),
            http: HttpClients::default(),
        }
    }
}

#[derive(Parser)]
#[command(
    name = "upheap",
    version,
    about = "Upheap from the terminal: a heap's items and tags, heaps, and signing in."
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
    /// A task's steps
    #[command(subcommand)]
    Step(StepCommand),
    /// Questions and requests on an item
    #[command(subcommand)]
    Question(QuestionCommand),
    /// Comments on an item
    #[command(subcommand)]
    Comment(CommentCommand),
    /// Tags in a heap
    #[command(subcommand)]
    Tag(TagCommand),
    /// Parameters in a heap
    #[command(subcommand)]
    Parameter(ParameterCommand),
    /// What agents work on: the queue, and a run's end
    #[command(subcommand)]
    Work(WorkCommand),
    /// Your heaps, and the default one
    #[command(subcommand)]
    Heap(HeapCommand),
    /// Signing in and out
    #[command(subcommand)]
    Auth(AuthCommand),
    /// How to use this tool, in one read
    #[command(subcommand)]
    Guide(GuideCommand),
}

#[derive(Subcommand)]
enum ItemCommand {
    /// Add an item
    Add(commands::item::add::AddArgs),
    /// The ranked list, or closed items with --closed
    List(commands::item::list::ListArgs),
    /// One item in full, and with --history what happened to it
    Show(commands::item::show::ShowArgs),
    /// Change an item's fields
    Edit(commands::item::edit::EditArgs),
    /// Move an item to in progress
    Start(commands::item::state::StateArgs),
    /// Move an item to blocked, optionally naming what it waits for
    Block(commands::item::state::BlockArgs),
    /// Close an item as done
    Done(commands::item::state::StateArgs),
    /// Close an item as won't do
    Wontdo(commands::item::state::StateArgs),
    /// Move an item back to open, from any state
    Reopen(commands::item::state::StateArgs),
    /// Make a note a task, in place: it keeps its key, age and history
    ToTask(commands::item::state::StateArgs),
    /// Close a note as became, once items are made from it with `item add --from`
    Became(commands::item::state::StateArgs),
    /// Make a note a step of an existing task in its heap, and close the note as became
    ToStep(commands::item::to_step::ToStepArgs),
    /// Move an item to another heap of its team, with its context, steps, questions and history
    Move(commands::item::move_item::MoveArgs),
}

#[derive(Subcommand)]
enum StepCommand {
    /// Add a step to a task
    Add(commands::step::add::AddArgs),
    /// Tick a step
    Tick(commands::step::state::StepArgs),
    /// Drop a step: it stays, shown as dropped
    Drop(commands::step::state::StepArgs),
    /// Change a step's title or context
    Edit(commands::step::edit::EditArgs),
}

#[derive(Subcommand)]
enum QuestionCommand {
    /// Ask a question on an item, to be answered in free text or by picking a choice
    Ask(commands::question::ask::AskArgs),
    /// Ask for something to do on an item; it is replied to with Done or Can't be done
    Request(commands::question::ask::RequestArgs),
    /// Ask the person to review work at a link, as an agent doing a task; it is replied to with Approved or Changes
    /// asked
    Review(commands::question::ask::ReviewArgs),
    /// Propose closing a task as done, as an agent doing it; the person replies Close as done or Not yet
    ProposeDone(commands::question::ask::ProposeDoneArgs),
    /// Reply to a question, a request, a review or a done proposal on an item
    Reply(commands::question::reply::ReplyArgs),
}

#[derive(Subcommand)]
enum CommentCommand {
    /// Add a comment to an item: what you learn, decide or did, without touching its context
    Add(commands::comment::add::AddArgs),
    /// The comments on an item, oldest first
    List(commands::comment::list::ListArgs),
}

#[derive(Subcommand)]
enum TagCommand {
    /// The heap's tags
    List(commands::tag::list::ListArgs),
    /// Add a tag
    Add(commands::tag::add::AddArgs),
    /// Rename a tag or change its description
    Edit(commands::tag::edit::EditArgs),
}

#[derive(Subcommand)]
enum ParameterCommand {
    /// The heap's parameters and their options
    List(commands::parameter::list::ListArgs),
    /// Add a parameter to the heap
    Add(commands::parameter::add::AddArgs),
    /// Change a parameter's name, weight, tags or option order
    Edit(commands::parameter::edit::EditArgs),
    /// Archive a parameter no open item has a value for
    Archive(commands::parameter::archive::ArchiveArgs),
    /// A parameter's options
    #[command(subcommand)]
    Option(ParameterOptionCommand),
}

#[derive(Subcommand)]
enum ParameterOptionCommand {
    /// Add an option as the lowest priority
    Add(commands::parameter::option::add::AddArgs),
    /// Relabel an option
    Edit(commands::parameter::option::edit::EditArgs),
    /// Archive an option, moving open items to another
    Archive(commands::parameter::option::archive::ArchiveArgs),
}

#[derive(Subcommand)]
enum WorkCommand {
    /// The item an agent would work on next, without taking it
    Next(commands::work::next::NextArgs),
    /// Take the next item and start a run on it
    Claim(commands::work::claim::ClaimArgs),
    /// Work the queue: claim an item, start an agent tool on it, record how the run ended, and go on
    Run(commands::work::run::RunArgs),
    /// Say the run reached the turn's goal (needs UPHEAP_RUN)
    Reached(commands::work::reached::ReachedArgs),
    /// Give the turn back: there was nothing to do for its goal (needs UPHEAP_RUN)
    HandBack(commands::work::hand_back::HandBackArgs),
    /// End the run and record what it cost (needs UPHEAP_RUN)
    Finish(commands::work::finish::FinishArgs),
}

#[derive(Subcommand)]
enum HeapCommand {
    /// Add a heap to a team
    Add(commands::heap::add::AddArgs),
    /// Every heap you can use, by team
    List(commands::heap::list::ListArgs),
    /// Make a heap the default for commands that work in one
    Use(commands::heap::use_heap::UseArgs),
    /// A heap's name, weights and context
    Show(commands::heap::show::ShowArgs),
    /// Write or clear a heap's context
    Edit(commands::heap::edit::EditArgs),
    /// Change a heap's ranking weights, its parameters' included, in one save
    Weights(commands::heap::weights::WeightsArgs),
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Sign in through the browser
    Login(commands::auth::login::LoginArgs),
    /// Sign out, ending the sign-in on the server too
    Logout(commands::auth::logout::LogoutArgs),
}

#[derive(Subcommand)]
enum GuideCommand {
    /// Every command with its arguments and flags, and instructions for agents
    Show(commands::guide::show::ShowArgs),
}

impl Command {
    /// The flags every command that talks to the server takes. `None` for one that doesn't.
    fn shared(&self) -> Option<&SharedArgs> {
        Some(match self {
            Self::Item(ItemCommand::Add(args)) => &args.shared,
            Self::Item(ItemCommand::List(args)) => &args.shared,
            Self::Item(ItemCommand::Show(args)) => &args.shared,
            Self::Item(ItemCommand::Edit(args)) => &args.shared,
            Self::Item(
                ItemCommand::Start(args)
                | ItemCommand::Done(args)
                | ItemCommand::Wontdo(args)
                | ItemCommand::Reopen(args)
                | ItemCommand::ToTask(args)
                | ItemCommand::Became(args),
            ) => &args.shared,
            Self::Item(ItemCommand::Block(args)) => &args.state.shared,
            Self::Item(ItemCommand::ToStep(args)) => &args.state.shared,
            Self::Item(ItemCommand::Move(args)) => &args.shared,
            Self::Step(StepCommand::Add(args)) => &args.shared,
            Self::Step(StepCommand::Tick(args) | StepCommand::Drop(args)) => &args.shared,
            Self::Step(StepCommand::Edit(args)) => &args.shared,
            Self::Question(QuestionCommand::Ask(args)) => &args.shared,
            Self::Question(QuestionCommand::Request(args)) => &args.shared,
            Self::Question(QuestionCommand::Review(args)) => &args.shared,
            Self::Question(QuestionCommand::ProposeDone(args)) => &args.shared,
            Self::Comment(CommentCommand::Add(args)) => &args.shared,
            Self::Comment(CommentCommand::List(args)) => &args.shared,
            Self::Question(QuestionCommand::Reply(args)) => &args.shared,
            Self::Tag(TagCommand::List(args)) => &args.shared,
            Self::Tag(TagCommand::Add(args)) => &args.shared,
            Self::Tag(TagCommand::Edit(args)) => &args.shared,
            Self::Parameter(ParameterCommand::List(args)) => &args.shared,
            Self::Parameter(ParameterCommand::Add(args)) => &args.shared,
            Self::Parameter(ParameterCommand::Edit(args)) => &args.shared,
            Self::Parameter(ParameterCommand::Archive(args)) => &args.shared,
            Self::Parameter(ParameterCommand::Option(ParameterOptionCommand::Add(args))) => &args.shared,
            Self::Parameter(ParameterCommand::Option(ParameterOptionCommand::Edit(args))) => &args.shared,
            Self::Parameter(ParameterCommand::Option(ParameterOptionCommand::Archive(args))) => &args.shared,
            Self::Heap(HeapCommand::Add(args)) => &args.shared,
            Self::Work(WorkCommand::Next(args)) => &args.shared,
            Self::Work(WorkCommand::Claim(args)) => &args.shared,
            Self::Work(WorkCommand::Run(args)) => &args.shared,
            Self::Work(WorkCommand::Reached(args)) => &args.shared,
            Self::Work(WorkCommand::HandBack(args)) => &args.shared,
            Self::Work(WorkCommand::Finish(args)) => &args.shared,
            Self::Heap(HeapCommand::List(args)) => &args.shared,
            Self::Heap(HeapCommand::Use(args)) => &args.shared,
            Self::Heap(HeapCommand::Show(args)) => &args.shared,
            Self::Heap(HeapCommand::Edit(args)) => &args.shared,
            Self::Heap(HeapCommand::Weights(args)) => &args.shared,
            Self::Auth(AuthCommand::Login(args)) => &args.shared,
            Self::Auth(AuthCommand::Logout(args)) => &args.shared,
            Self::Guide(GuideCommand::Show(_)) => return None,
        })
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
        stdin,
        mut stdout,
        mut stderr,
        clock,
        work,
        http,
    } = deps;
    let color = env.stdout_is_terminal && !env.no_color;
    let in_run = env.run.is_some();
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
    let mode = match cli.command.shared() {
        Some(shared) if shared.json => Mode::Json,
        Some(shared) if shared.quiet => Mode::Quiet,
        _ => Mode::Human,
    };
    let mut out = Output::new(stdout, stderr, mode, color).on_terminals(env.stdout_is_terminal, env.stderr_is_terminal);
    let result = match cli.command {
        // Before the config file is read, so the guide can be read on a machine where nothing is set up yet.
        Command::Guide(GuideCommand::Show(_)) => {
            commands::guide::show::run(&Cli::command(), &mut out).map_err(CliError::from)
        }
        command => match load_app(env, tokens, browser, stdin, clock, http) {
            // Before any request, so a bad run ID is told of once, not as a refusal.
            Ok(app) => match check_run_env(&app.env) {
                Ok(()) => dispatch(command, &app, work, &mut out).await,
                Err(err) => Err(err.into()),
            },
            Err(err) => Err(err.into()),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // The reader went away, as `| head` does once it has its lines: nothing more to say, and nobody to say it to.
        Err(CliError::Output(err)) if err.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(err) => {
            report(&err, &mut out, in_run);
            ExitCode::from(err.exit_code())
        }
    }
}

/// `UPHEAP_RUN`, when set, must be a run ID.
fn check_run_env(env: &Environment) -> Result<(), UsageError> {
    match &env.run {
        Some(run) if uuid::Uuid::parse_str(run).is_err() => Err(UsageError::BadRunEnv),
        _ => Ok(()),
    }
}

fn load_app(
    env: Environment,
    tokens: Arc<dyn TokenStore>,
    browser: Box<dyn Browser>,
    stdin: Box<dyn Read + Send>,
    clock: Clock,
    http: HttpClients,
) -> Result<App, ConfigError> {
    let config = match &env.config_dir {
        Some(dir) => config::load_config(dir)?,
        None => ConfigFile::default(),
    };
    Ok(App {
        env,
        tokens,
        browser,
        stdin: Arc::new(Mutex::new(stdin)),
        config,
        clock,
        http,
    })
}

async fn dispatch(command: Command, app: &App, work: WorkDeps, out: &mut Output) -> Result<(), CliError> {
    use commands::item::state::{StateVerb, run_block, run_verb};
    use commands::step::state::{StepVerb, run_verb as run_step_verb};
    match command {
        Command::Item(ItemCommand::Add(args)) => commands::item::add::run(args, app, out).await,
        Command::Item(ItemCommand::List(args)) => commands::item::list::run(args, app, out).await,
        Command::Item(ItemCommand::Show(args)) => commands::item::show::run(args, app, out).await,
        Command::Item(ItemCommand::Edit(args)) => commands::item::edit::run(args, app, out).await,
        Command::Item(ItemCommand::Start(args)) => run_verb(StateVerb::Start, args, app, out).await,
        Command::Item(ItemCommand::Block(args)) => run_block(args, app, out).await,
        Command::Item(ItemCommand::Done(args)) => run_verb(StateVerb::Done, args, app, out).await,
        Command::Item(ItemCommand::Wontdo(args)) => run_verb(StateVerb::WontDo, args, app, out).await,
        Command::Item(ItemCommand::Reopen(args)) => run_verb(StateVerb::Reopen, args, app, out).await,
        Command::Item(ItemCommand::ToTask(args)) => commands::item::to_task::run(args, app, out).await,
        Command::Item(ItemCommand::Became(args)) => commands::item::became::run(args, app, out).await,
        Command::Item(ItemCommand::ToStep(args)) => commands::item::to_step::run(args, app, out).await,
        Command::Item(ItemCommand::Move(args)) => commands::item::move_item::run(args, app, out).await,
        Command::Step(StepCommand::Add(args)) => commands::step::add::run(args, app, out).await,
        Command::Step(StepCommand::Tick(args)) => run_step_verb(StepVerb::Tick, args, app, out).await,
        Command::Step(StepCommand::Drop(args)) => run_step_verb(StepVerb::Drop, args, app, out).await,
        Command::Step(StepCommand::Edit(args)) => commands::step::edit::run(args, app, out).await,
        Command::Question(QuestionCommand::Ask(args)) => commands::question::ask::run_ask(args, app, out).await,
        Command::Question(QuestionCommand::Request(args)) => commands::question::ask::run_request(args, app, out).await,
        Command::Question(QuestionCommand::Review(args)) => commands::question::ask::run_review(args, app, out).await,
        Command::Question(QuestionCommand::ProposeDone(args)) => {
            commands::question::ask::run_propose_done(args, app, out).await
        }
        Command::Comment(CommentCommand::Add(args)) => commands::comment::add::run(args, app, out).await,
        Command::Comment(CommentCommand::List(args)) => commands::comment::list::run(args, app, out).await,
        Command::Question(QuestionCommand::Reply(args)) => commands::question::reply::run_reply(args, app, out).await,
        Command::Tag(TagCommand::List(args)) => commands::tag::list::run(args, app, out).await,
        Command::Tag(TagCommand::Add(args)) => commands::tag::add::run(args, app, out).await,
        Command::Tag(TagCommand::Edit(args)) => commands::tag::edit::run(args, app, out).await,
        Command::Parameter(ParameterCommand::List(args)) => commands::parameter::list::run(args, app, out).await,
        Command::Parameter(ParameterCommand::Add(args)) => commands::parameter::add::run(args, app, out).await,
        Command::Parameter(ParameterCommand::Edit(args)) => commands::parameter::edit::run(args, app, out).await,
        Command::Parameter(ParameterCommand::Archive(args)) => commands::parameter::archive::run(args, app, out).await,
        Command::Parameter(ParameterCommand::Option(ParameterOptionCommand::Add(args))) => {
            commands::parameter::option::add::run(args, app, out).await
        }
        Command::Parameter(ParameterCommand::Option(ParameterOptionCommand::Edit(args))) => {
            commands::parameter::option::edit::run(args, app, out).await
        }
        Command::Parameter(ParameterCommand::Option(ParameterOptionCommand::Archive(args))) => {
            commands::parameter::option::archive::run(args, app, out).await
        }
        Command::Heap(HeapCommand::Add(args)) => commands::heap::add::run(args, app, out).await,
        Command::Work(WorkCommand::Next(args)) => commands::work::next::run(args, app, out).await,
        Command::Work(WorkCommand::Claim(args)) => commands::work::claim::run(args, app, out).await,
        Command::Work(WorkCommand::Run(args)) => commands::work::run::run(args, app, work, out).await,
        Command::Work(WorkCommand::Reached(args)) => commands::work::reached::run(args, app, out).await,
        Command::Work(WorkCommand::HandBack(args)) => commands::work::hand_back::run(args, app, out).await,
        Command::Work(WorkCommand::Finish(args)) => commands::work::finish::run(args, app, out).await,
        Command::Heap(HeapCommand::List(args)) => commands::heap::list::run(args, app, out).await,
        Command::Heap(HeapCommand::Use(args)) => commands::heap::use_heap::run(args, app, out).await,
        Command::Heap(HeapCommand::Show(args)) => commands::heap::show::run(args, app, out).await,
        Command::Heap(HeapCommand::Edit(args)) => commands::heap::edit::run(args, app, out).await,
        Command::Heap(HeapCommand::Weights(args)) => commands::heap::weights::run(args, app, out).await,
        Command::Auth(AuthCommand::Login(args)) => commands::auth::login::run(args, app, out).await,
        Command::Auth(AuthCommand::Logout(args)) => commands::auth::logout::run(args, app, out).await,
        Command::Guide(GuideCommand::Show(_)) => Ok(commands::guide::show::run(&Cli::command(), out)?),
    }
}

/// Writes `err` to stderr: the server's problem body as it came with `--json`, a message otherwise.
fn report(err: &CliError, out: &mut Output, in_run: bool) {
    if out.mode() == Mode::Json
        && let Some(body) = err.problem_body()
    {
        let _ = out.json_note(body);
        return;
    }
    for line in err.to_message_in(in_run) {
        // Nowhere left to report a failure to write to stderr.
        let _ = out.note(&line);
    }
}

/// What every command works with: the environment, the config file as it was at startup, the token store, the
/// browser and stdin.
pub(crate) struct App {
    pub env: Environment,
    pub tokens: Arc<dyn TokenStore>,
    pub browser: Box<dyn Browser>,
    /// Shared so a read can run on a blocking thread, as stdin may be a terminal that never ends.
    stdin: Arc<Mutex<Box<dyn Read + Send>>>,
    pub config: ConfigFile,
    /// The time now, as opposed to `env.now`, when the command started: a loop that lives for hours must see its
    /// access token go stale. The wall clock, not time elapsed, which stands still while the machine sleeps.
    clock: Clock,
    http: HttpClients,
}

impl App {
    /// The time now.
    pub fn now(&self) -> jiff::Timestamp {
        (self.clock)()
    }

    /// All of stdin, read to its end as UTF-8.
    pub async fn read_stdin(&self, flag: &'static str) -> Result<String, UsageError> {
        let stdin = Arc::clone(&self.stdin);
        tokio::task::spawn_blocking(move || {
            let mut text = String::new();
            let mut reader = stdin.lock().unwrap_or_else(PoisonError::into_inner);
            reader.read_to_string(&mut text).map(|_| text)
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|read| read)
        .map_err(|source| UsageError::UnreadableStdin { flag, source })
    }

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
        ApiClient::new(&self.http, server, self.env.time_zone.clone()).map(|client| client.in_run(self.env.run.clone()))
    }

    /// An access token for `client`'s server, refreshed when the stored one is stale.
    pub async fn access_token(&self, client: &ApiClient) -> Result<String, CliError> {
        let server = config::server_key(client.server());
        let token = auth::access_token(
            &self.tokens,
            self.config_dir()?,
            client,
            &server,
            self.now().as_second(),
        )
        .await?;
        Ok(token)
    }
}
