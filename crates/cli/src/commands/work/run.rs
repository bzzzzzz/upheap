//! `upheap work run --tool <tool> [--heap <heap>]... [--once] [--workers <N>] [--allow <RULE>]...`: the agent loop.
//!
//! It claims the next item of the queue, starts the agent tool on it with a brief, keeps the claim alive, records
//! how the run ended and what it cost, and waits when there is nothing to do. The server decides how a run ended
//! (it reached its goal, it handed the turn back, it asked, or it failed); the loop only reports what the tool did.
//! When the tool stopped at the account's usage limit, the run is recorded as limited and every worker holds until the
//! limit resets, then claims again; the queue gives that item first place.
//! The tool sits behind [`AgentTools`], and the waits and interruptions come in through [`WorkDeps`], so tests pass
//! their own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use api_types::common::ItemDto;
use api_types::heaps::HeapDto;
use api_types::work::{ClaimDto, ClaimRequestDto, FinishRequestDto, RunDto, RunEndDto};
use clap::Args;
use futures_util::future::join_all;
use reqwest::Url;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use tokio::time::{Instant, MissedTickBehavior};

use super::brief::{BriefParts, KnownGoal, build_brief};
use super::kept::{self, Kept, LiveDirs, Verdict};
use super::logs;
use super::{
    HeapsArgs, NamedHeap, end_word, ended_journey, journey, missing_agents, resolve_named_heaps, waiting_by_heap_text,
    waiting_text,
};
use crate::agent::leftovers;
use crate::agent::{
    Access, AgentTools, Interrupt, Tool, ToolError, ToolReport, ToolStart, UsageLimit, WorkDeps, make_root,
};
use crate::client::{ApiClient, ClientError, Fetched, NO_QUERY};
use crate::commands::heap_api::HeapApi;
use crate::commands::item::show::item_text;
use crate::commands::{SharedArgs, UsageError};
use crate::config::server_key;
use crate::lookup::HeapHandle;
use crate::output::{Cell, Mode, Output, Tone, shorten, spoken_duration};
use crate::{App, CliError};

/// How many times the end of a run is sent before giving up on the server being out of reach.
const FINISH_TRIES: u32 = 3;

/// How long past the reset the loop holds, so a run doesn't start in the second the limit lifts.
const LIMIT_MARGIN: Duration = Duration::from_secs(60);

/// How long the loop holds when the tool didn't say when its limit resets.
const LIMIT_FALLBACK: Duration = Duration::from_secs(30 * 60);

/// The longest title a run's line shows, in characters.
const TITLE_WIDTH: usize = 40;

/// The help of `--tool`, with the oldest version of each tool `work run` starts.
fn tool_help() -> String {
    format!(
        "The agent tool to start on each item: claude {} or newer, or copilot {} or newer",
        Tool::Claude.min_version(),
        Tool::Copilot.min_version()
    )
}

#[derive(Args, Debug)]
pub struct RunArgs {
    #[arg(long, value_enum, help = tool_help())]
    pub tool: Tool,
    /// The model every run's agent tool uses, passed to the tool as it is. Left out, the tool's own default
    #[arg(long, value_name = "MODEL", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub model: Option<String>,
    #[command(flatten)]
    pub heaps: HeapsArgs,
    /// Do one run, or nothing when there is nothing to do, and exit. With --workers, one run for each worker
    #[arg(long)]
    pub once: bool,
    /// Run each item in a directory of its own under this one, `<team>/<heap>/<key>`, kept for its next run and made
    /// with the tool's instructions and settings for working there. Left out, every run starts in this directory, as
    /// it is
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,
    /// How many runs to keep going at once, each on its own item. Above 1, needs --root
    #[arg(long, value_name = "N", default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=MAX_WORKERS as i64))]
    pub workers: u8,
    /// A file with your own instructions for every run: how you want work done on this machine. Each run is told
    /// them after Upheap's own rules and before the heap's context. Repeat it for several, such as a shared file and
    /// one for this machine: they are joined in the order given. Read once, when the loop starts
    #[arg(long, value_name = "FILE")]
    pub instructions: Vec<PathBuf>,
    /// An extra allow rule for the agent tool in every run, in the tool's own syntax, such as
    /// mcp__claude_ai_Claude_Docs or Bash(npm:*) for claude, shell(npm:*) for copilot. Repeat it for several
    #[arg(long, value_name = "RULE")]
    pub allow: Vec<String>,
    /// An extra allow rule only in runs that prepare an item (to with context or defined). Repeat it for several
    #[arg(long, value_name = "RULE")]
    pub allow_prepare: Vec<String>,
    /// An extra allow rule only in runs that do a task (to done). Repeat it for several
    #[arg(long, value_name = "RULE")]
    pub allow_do: Vec<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Checks one allow rule of `flag` for `tool`, before any request. Claude's `--allowedTools` splits a value on commas
/// and spaces, so a rule holding either would become several rules, or none: each rule gets its own flag instead. The
/// rule is not trimmed. A rule starting with `-` would reach `claude` as a flag of its own, not as a rule, so it is
/// refused too. Copilot gets each rule as one `--allow-tool=<rule>` argument, so a leading `-` or a space can't split
/// it, and its rules hold spaces (`shell(gh pr)`): only an empty one is refused.
fn check_allow_rule(tool: Tool, flag: &'static str, rule: &str) -> Result<(), UsageError> {
    let reason = if rule.is_empty() {
        "an allow rule can't be empty"
    } else if tool == Tool::Copilot {
        return Ok(());
    } else if rule.starts_with('-') {
        "an allow rule can't start with -"
    } else if rule.contains(',') || rule.contains(char::is_whitespace) {
        "an allow rule can't hold a comma or whitespace: give each rule its own flag"
    } else {
        return Ok(());
    };
    Err(UsageError::BadAllowRule {
        flag,
        rule: rule.to_owned(),
        reason,
    })
}

/// The runner's extra allow rules, from `--allow`, `--allow-prepare` and `--allow-do`.
#[derive(Debug, Default)]
struct AllowRules {
    /// For every run.
    every: Vec<String>,
    /// For runs that prepare an item only.
    prepare: Vec<String>,
    /// For runs that do a task only.
    doing: Vec<String>,
}

impl AllowRules {
    /// The rules for a run with `access`: those for every run, then those for its access, each in the order given.
    fn for_access(&self, access: Access) -> Vec<String> {
        let own = match access {
            Access::Prepare => &self.prepare,
            Access::Do => &self.doing,
        };
        self.every.iter().chain(own).cloned().collect()
    }
}

/// Works the queue until stopped: claims an item, starts the tool on it, and records how the run ended. Prints a line
/// for each run that ends, `14:02  k3x9a  Rotate the certs  Work  note → with context  reached  2m10s  $0.31`, and
/// one when nothing is left to do. Ctrl-C once stops the loop after the current run, twice stops the run too.
/// `--quiet` prints the key of each item worked, or `team/heap/key` with several heaps, `--json` each run as the
/// server records its end. Exit 0 when stopped or `--once` is done, and 5 when the server can't be reached with
/// `--once`. Refused inside a run.
///
/// With `--workers N`, up to N runs go at once, each on its own item, and a line is printed as each ends. One Ctrl-C
/// reaches them all: no run starts, the live ones end, and a second Ctrl-C stops them now. With `--once`, each worker
/// does one pass, so up to N runs. Above 1 it needs `--root`, since runs in one directory would trample each other.
///
/// With `--root`, each item gets a directory of its own under it, `<team>/<heap>/<key>`, which the tool sets up and
/// starts in. It is the same one for every run on the item, as the last run left it. The loop removes it once the
/// item is closed or can't be found, or when no run has been in it for 7 days. Directories from before, named by
/// an item's key and a run's ID, are never touched.
///
/// With `--model`, every run's tool is started with `--model <model>`, the value as given. Each run records the tool
/// and the model: the one the tool's own output names, else the one given, else none.
///
/// A run whose tool stopped at the account's usage limit ends as `limited`, not failed. The loop then claims nothing
/// until the limit resets and a minute more (30 minutes when the tool gave no time), by the wall clock, and says so;
/// live runs end as usual. With `--once` it says when to run again and exits at once.
pub async fn run(args: RunArgs, app: &App, work: WorkDeps, out: &mut Output) -> Result<(), CliError> {
    if app.env.run.is_some() {
        return Err(UsageError::RunInRun.into());
    }
    if args.workers > 1 && args.root.is_none() {
        return Err(UsageError::WorkersWithoutRoot { workers: args.workers }.into());
    }
    for (flag, rules) in [
        ("--allow", &args.allow),
        ("--allow-prepare", &args.allow_prepare),
        ("--allow-do", &args.allow_do),
    ] {
        for rule in rules {
            check_allow_rule(args.tool, flag, rule)?;
        }
    }
    let server = app.server(&args.shared)?;
    let client = app.client(server.clone())?;
    let token = app.access_token(&client).await?;
    let heaps = resolve_named_heaps(&args.heaps, app, &client, &token).await?;
    work.tools.check(args.tool).await?;
    let root = args.root.as_deref().map(make_root).transpose()?;
    let instructions = read_instructions(args.instructions).await?;
    let mode = out.mode();
    if mode == Mode::Human {
        out.line(&header(&heaps, args.tool, args.once, args.workers))?;
        if let Some(config_dir) = &app.env.config_dir {
            let note = format!(
                "Each run's {} output is saved in {}, as <run id>.jsonl.",
                args.tool,
                logs::runs_dir(config_dir).display()
            );
            out.note_toned(&note, Tone::Dim)?;
        }
    }
    let WorkDeps {
        tools,
        interrupts,
        idle_wait,
        heartbeat_every,
        heartbeat_check,
        finish_retry,
        sweep_every,
    } = work;
    let (stop, _) = watch::channel(Stop::No);
    let shared = Shared {
        app,
        server,
        tool: args.tool,
        model: args.model,
        heaps: heaps.into_iter().map(|heap| heap.handle).collect(),
        once: args.once,
        workers: args.workers,
        root,
        instructions,
        allow: AllowRules {
            every: args.allow,
            prepare: args.allow_prepare,
            doing: args.allow_do,
        },
        tools,
        idle_wait,
        heartbeat_every,
        heartbeat_check,
        finish_retry,
        sweep_every,
        last_sweep: Mutex::new(None),
        last_log_prune: Mutex::new(None),
        live_dirs: LiveDirs::default(),
        mode,
        out: Mutex::new(out),
        noted: Mutex::new(Noted::default()),
        live: Mutex::new(Vec::new()),
        pressed: AtomicBool::new(false),
        stopping_noted: AtomicBool::new(false),
        first_error: Mutex::new(None),
        hold: Mutex::new(None),
        stop,
    };
    shared.work_the_queue(interrupts()).await
}

/// The most runs `--workers` allows at once.
const MAX_WORKERS: u8 = 8;

/// `Working in Work and House with claude, 3 runs at a time. Ctrl-C stops after the current runs.`
fn header(heaps: &[NamedHeap], tool: Tool, once: bool, workers: u8) -> String {
    let names: Vec<&str> = heaps.iter().map(|heap| heap.name.as_str()).collect();
    let mut text = format!("Working in {} with {tool}", join_names(&names));
    if workers > 1 {
        text.push_str(&format!(", {workers} runs at a time"));
    }
    text.push('.');
    if !once {
        text.push_str(if workers > 1 {
            " Ctrl-C stops after the current runs."
        } else {
            " Ctrl-C stops after the current run."
        });
    }
    text
}

/// `A`, `A and B`, `A, B and C`.
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// What a worker does next.
enum Next {
    /// Claim again at once.
    Again,
    /// Wait before claiming again.
    Wait,
    /// The tool stopped at the account's usage limit, and the hold is set: claim nothing until it ends.
    Hold(UsageLimit),
}

/// What the loop does when a claim, or the reads that follow it, fail.
#[derive(Debug, PartialEq, Eq)]
enum AfterFailure {
    /// Say the server can't be reached, wait, and ask again.
    TryAgain,
    /// End the worker with the error.
    Stop,
}

/// A server that can't be reached is waited for, unless the person asked for one run; any other failure ends the
/// loop, since asking again would only fail again.
fn after_failure(err: &CliError, once: bool) -> AfterFailure {
    if is_unreachable(err) && !once {
        AfterFailure::TryAgain
    } else {
        AfterFailure::Stop
    }
}

/// Whether a failure to read a claimed item is about that item alone: the server answered that it, or its heap, is
/// not there or not the caller's. Every other failure (the server out of reach or broken, the sign-in gone, an
/// answer that can't be read) would meet the next item too, and going on would fail run after run until every turn
/// in the queue is stuck.
fn is_about_the_item(err: &CliError) -> bool {
    matches!(err.problem_status(), Some(403 | 404))
}

/// Whether the failure is the server being out of reach or unavailable (exit code 5).
pub(super) fn is_unreachable(err: &CliError) -> bool {
    err.exit_code() == 5
}

/// How many heartbeats in a row must fail before the loop says so on stderr. Every failure is logged.
const FAILURES_TO_SAY: u32 = 2;

/// How a heartbeat went.
enum Heartbeat {
    /// The server heard it.
    Heard,
    /// The server says the run is over.
    Over,
    /// It didn't get through, or the server refused it for another reason.
    Failed(CliError),
}

/// How the tool's time ended, as far as the loop can tell.
enum Driven {
    /// The tool ended on its own, or could not be started.
    Ended(Result<ToolReport, ToolError>),
    /// The server says the run is over: the person took the turn back, or the run lapsed. The tool was stopped.
    Over,
    /// The person stopped the run. The tool was stopped.
    Stopped,
}

/// The text of each file `--instructions` names, in the order given. Read before the first claim, so a file that
/// can't be read fails before any item is taken.
async fn read_instructions(paths: Vec<PathBuf>) -> Result<Vec<String>, CliError> {
    let mut texts = Vec::with_capacity(paths.len());
    for path in paths {
        texts.push(read_instructions_file(path).await?);
    }
    Ok(texts)
}

/// The text of one file `--instructions` names.
async fn read_instructions_file(path: PathBuf) -> Result<String, CliError> {
    let read = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await
    };
    match read {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(source)) => Err(CliError::NoInstructions { path, source }),
        Err(join) => Err(CliError::NoInstructions {
            path,
            source: std::io::Error::other(join),
        }),
    }
}

/// How far the loop has been told to stop. It only moves forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Stop {
    /// Keep working.
    No,
    /// Claim nothing new; let the live runs end and be recorded.
    AfterRuns,
    /// Stop every live tool now.
    Now,
}

/// What has been said while the loop stays idle or out of reach, shared by every worker so a stretch is said once.
#[derive(Default)]
struct Noted {
    /// What the idle line said about the items that wait for the person (the text after `Nothing to do.`) when it was
    /// last printed, while the loop stays idle.
    idle: Option<String>,
    /// Whether the server being out of reach has been said, while it stays so.
    unreachable: bool,
}

/// A live run, as the note that the loop is stopping lists it.
struct LiveRun {
    id: uuid::Uuid,
    key: String,
    title: String,
    /// UTC milliseconds: when the run was claimed.
    started_at: i64,
}

/// The loop's context: what it works on, how, and what it has said so far. One, borrowed by every worker.
///
/// The workers and the interrupt listener run on the task that runs the command, joined together, not as spawned
/// tasks: a spawned task must be `'static`, and they all borrow the [`App`]. The binary runs on one thread anyway, so
/// nothing is lost, and nothing here is held across an `.await` but the stop signal.
struct Shared<'a> {
    app: &'a App,
    server: Url,
    tool: Tool,
    /// The `--model` the tool is started with; `None` for the tool's own default.
    model: Option<String>,
    /// The heaps the queue is over, each as `team/heap`.
    heaps: Vec<String>,
    once: bool,
    /// How many workers there are.
    workers: u8,
    /// Where each item's directory is kept; `None` when runs start in this directory.
    root: Option<PathBuf>,
    /// The runner's own instructions, one text per file, for every run's brief.
    instructions: Vec<String>,
    /// The runner's extra allow rules, which each run's tool is started with.
    allow: AllowRules,
    tools: Box<dyn AgentTools>,
    idle_wait: Duration,
    heartbeat_every: Duration,
    heartbeat_check: Duration,
    finish_retry: Duration,
    sweep_every: Duration,
    /// When the root was last swept, or began to be; `None` before the first sweep. Taken by the worker that finds
    /// a sweep due, so the others go on claiming.
    last_sweep: Mutex<Option<Instant>>,
    /// When the old run logs were last removed; `None` before the first time.
    last_log_prune: Mutex<Option<Instant>>,
    /// The directories runs are in, which a sweep leaves alone.
    live_dirs: LiveDirs,
    mode: Mode,
    /// Locked only to write, never across an `.await`, so lines from two workers never tear or interleave.
    out: Mutex<&'a mut Output>,
    noted: Mutex<Noted>,
    /// The runs that are live: claimed and not yet recorded and printed, in the order they were claimed.
    live: Mutex<Vec<LiveRun>>,
    /// Whether the person has pressed Ctrl-C, so a run claimed after that can still be told about it.
    pressed: AtomicBool,
    /// Whether the note that the loop is stopping has been printed, so it is printed once.
    stopping_noted: AtomicBool,
    /// The error that ended the first worker that failed; the command fails with it once every worker has ended.
    first_error: Mutex<Option<CliError>>,
    /// When claiming may start again after the tool's usage limit was reached, while it is held. One for every worker.
    hold: Mutex<Option<jiff::Timestamp>>,
    stop: watch::Sender<Stop>,
}

impl Shared<'_> {
    /// Runs every worker, and the listener for interrupts until they have all ended.
    async fn work_the_queue(&self, interrupts: UnboundedReceiver<Interrupt>) -> Result<(), CliError> {
        let workers = join_all((0..self.workers).map(|_| self.work()));
        // The listener never ends by itself, so the workers ending is what ends this.
        tokio::select! {
            _ = workers => {}
            () = self.listen(interrupts) => {}
        }
        match lock(&self.first_error).take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Turns the person's interrupts into the stop signal. Once nobody can interrupt any more, waits for ever: the
    /// workers carry on.
    async fn listen(&self, mut interrupts: UnboundedReceiver<Interrupt>) {
        // Counted here, not read off the stop signal: a worker's failure raises that too, and the person's first
        // Ctrl-C is the gentle one whatever came before it.
        let mut presses = 0_u32;
        while let Some(interrupt) = interrupts.recv().await {
            match interrupt {
                Interrupt::CtrlC => {
                    presses += 1;
                    if presses == 1 {
                        self.raise(Stop::AfterRuns);
                        self.pressed.store(true, Ordering::SeqCst);
                        if self.has_live_runs() {
                            self.note_stopping();
                        }
                    } else {
                        self.raise(Stop::Now);
                    }
                }
                Interrupt::Now => self.raise(Stop::Now),
            }
        }
        std::future::pending::<()>().await;
    }

    /// Says once for the loop that it stops after the live runs, and how to stop them now, then lists each live run
    /// with how long it has been going, so the person can tell whether to wait.
    fn note_stopping(&self) {
        if self.stopping_noted.swap(true, Ordering::SeqCst) {
            return;
        }
        let text = if self.workers > 1 {
            "Stopping after the current runs. Ctrl-C again stops them now."
        } else {
            "Stopping after this run. Ctrl-C again stops it now."
        };
        let now = self.app.now().as_millisecond();
        let rows: Vec<Vec<Cell>> = lock(&self.live)
            .iter()
            .map(|run| {
                vec![
                    Cell::toned(run.key.clone(), Tone::Strong),
                    Cell::new(shorten(&run.title, TITLE_WIDTH)),
                    Cell::new(format_duration(now - run.started_at)),
                ]
            })
            .collect();
        // Nowhere left to say it if stderr is gone.
        let _ = self.write(|out| {
            out.note(text)?;
            for line in out.render_table(&rows) {
                out.note_table_line("  ", &line)?;
            }
            Ok::<_, std::io::Error>(())
        });
    }

    fn has_live_runs(&self) -> bool {
        !lock(&self.live).is_empty()
    }

    /// Moves the stop signal forward to `to`, never back.
    fn raise(&self, to: Stop) {
        self.stop.send_if_modified(|stop| {
            let moved = to > *stop;
            if moved {
                *stop = to;
            }
            moved
        });
    }

    /// Whether the loop was told to stop in any way: nothing new is claimed.
    fn is_stopping(&self) -> bool {
        *self.stop.borrow() != Stop::No
    }

    /// Waits until the stop signal is at least `level`.
    async fn stop_reaches(&self, level: Stop) {
        let mut stop = self.stop.subscribe();
        // The sender lives as long as `self`, so this can't fail.
        let _ = stop.wait_for(|stop| *stop >= level).await;
    }

    /// Writes with the output, for as long as `write` takes and no longer.
    fn write<T>(&self, write: impl FnOnce(&mut Output) -> T) -> T {
        write(&mut lock(&self.out))
    }

    /// A note on stderr. Nowhere left to say it if stderr is gone.
    fn note(&self, text: &str) {
        let _ = self.write(|out| out.note(text));
    }

    /// One worker: claims, works the run, and asks again, until it is told to stop, or fails.
    async fn work(&self) {
        loop {
            if self.is_stopping() {
                return;
            }
            self.prune_logs_if_due().await;
            self.sweep_if_due().await;
            // A sweep can take a while: a Ctrl-C during it claims nothing more, so the runs the note listed are all.
            if self.is_stopping() {
                return;
            }
            if !self.wait_for_hold().await {
                return;
            }
            let next = match self.work_a_pass().await {
                Ok(next) => next,
                Err(err) if after_failure(&err, self.once) == AfterFailure::TryAgain => {
                    self.note_unreachable();
                    Next::Wait
                }
                Err(err) => return self.end_with(err),
            };
            match next {
                Next::Hold(limit) if self.once => return self.note_limit_once(&limit),
                Next::Hold(_) => {}
                _ if self.once => return,
                Next::Again => {}
                Next::Wait => {
                    if !self.wait_to_ask_again().await {
                        return;
                    }
                }
            }
        }
    }

    /// Ends the worker with `err`. The others claim nothing new and let their live runs end. The first error is the
    /// one the command fails with; the rest are logged.
    fn end_with(&self, err: CliError) {
        let mut first = lock(&self.first_error);
        if first.is_some() {
            tracing::warn!(error = %err, "another worker ended with an error");
            return;
        }
        self.raise(Stop::AfterRuns);
        if self.has_live_runs() {
            self.note(&format!(
                "Stopping after the current runs: {err}. Ctrl-C twice stops them now."
            ));
        }
        *first = Some(err);
    }

    /// Claims once, and works the run that came of it.
    async fn work_a_pass(&self) -> Result<Next, CliError> {
        let claimed = self.claim().await?;
        lock(&self.noted).unreachable = false;
        let Some(run) = claimed.value.run else {
            self.note_idle(&claimed.value)?;
            return Ok(Next::Wait);
        };
        lock(&self.noted).idle = None;
        // A run claimed after the stop signal is still worked to its end: abandoning it would leave it to lapse.
        let id = run.id;
        lock(&self.live).push(LiveRun {
            id,
            key: run.item.key.clone(),
            title: run.item.title.clone(),
            started_at: run.started_at,
        });
        // A Ctrl-C that came while this was being claimed found nothing live to say it to.
        if self.pressed.load(Ordering::SeqCst) {
            self.note_stopping();
        }
        let result = self.work_claimed(run).await;
        lock(&self.live).retain(|live| live.id != id);
        result
    }

    async fn work_claimed(&self, run: RunDto) -> Result<Next, CliError> {
        let Some(goal) = KnownGoal::from_dto(run.item.goal) else {
            let failure = "This version of upheap doesn't know this turn's goal. Update upheap.";
            let request = FinishRequestDto {
                cost_usd: None,
                failure: Some(failure.to_owned()),
                ..Default::default()
            };
            self.finish_and_print(&run, &request, None).await?;
            return Ok(Next::Wait);
        };
        self.work_one(run, goal).await
    }

    async fn claim(&self) -> Result<Fetched<ClaimDto>, CliError> {
        let client = self.app.client(self.server.clone())?;
        let token = self.app.access_token(&client).await?;
        let body = ClaimRequestDto {
            heaps: self.heaps.clone(),
        };
        client
            .post::<ClaimDto>(&["work", "runs"], &body, &token)
            .await
            .map_err(|err| missing_agents(err, None))
    }

    /// Runs the tool on the claimed `run`, keeps the claim alive meanwhile, and records how it ended.
    async fn work_one(&self, run: RunDto, goal: KnownGoal) -> Result<Next, CliError> {
        self.note_started(&run);
        let handle = HeapHandle {
            team: run.item.heap.team_slug.clone(),
            heap: run.item.heap.slug.clone(),
        };
        let (heap, item) = match self.read_heap_and_item(&run, &handle).await {
            Ok(read) => read,
            Err(err) => {
                let request = FinishRequestDto {
                    cost_usd: None,
                    failure: Some(format!("Couldn't read the item before starting: {err}")),
                    ..Default::default()
                };
                self.finish_and_print(&run, &request, None).await?;
                // An item that is gone, such as one moved to another heap since the claim, is that item's trouble:
                // the next may be fine. Anything else would fail every run, so it ends the worker.
                return if is_about_the_item(&err) {
                    Ok(Next::Again)
                } else {
                    Err(err)
                };
            }
        };
        let brief = build_brief(&BriefParts {
            goal,
            runner_instructions: &self.instructions,
            heap_name: &run.item.heap.name,
            heap_handle: &handle.to_string(),
            heap_context: heap.context.as_deref(),
            item_text: &item_text(&item, self.app.now()),
            line: run.item.line.as_deref(),
            runs: run.item.runs,
            failed_runs: run.item.failed_runs,
        });
        let log = self
            .app
            .env
            .config_dir
            .as_deref()
            .map(|config_dir| logs::log_path(config_dir, &run.id.to_string()));
        let start = ToolStart {
            tool: self.tool,
            access: goal.access(),
            brief,
            env: vec![
                ("UPHEAP_RUN".to_owned(), run.id.to_string()),
                ("UPHEAP_SERVER".to_owned(), self.server.to_string()),
                ("UPHEAP_HEAP".to_owned(), handle.to_string()),
            ],
            dir: None,
            allow: self.allow.for_access(goal.access()),
            log: log.clone(),
            model: self.model.clone(),
        };
        tracing::info!(run_id = %run.id, item = %run.item.key, goal = ?goal, "starting the tool");
        // Registered before the directory is made or touched, and kept until the run is recorded below, so a sweep
        // never takes the directory of a run that is starting or running.
        let dir = self.root.as_deref().map(|root| {
            kept::item_directory(root, &handle.team, &handle.heap, &run.item.key).map_err(|source| {
                ToolError::NoDirectory {
                    dir: root.to_owned(),
                    source,
                }
            })
        });
        let _live = dir
            .as_ref()
            .and_then(|dir| dir.as_ref().ok())
            .map(|dir| self.live_dirs.register(dir.clone()));
        // The marker stays open and locked until the tool is gone, so no sweep, in this loop or another, takes the
        // directory meanwhile.
        let mut marker = None;
        let (driven, tool_started) = match dir {
            None => {
                // Taken just before the tool is launched, to be sent with `finish` as when the tool started.
                let started = self.app.now();
                (self.drive_tool(&run, start).await, Some(started))
            }
            Some(Ok(dir)) => match self.prepare_directory(&run, &dir).await {
                Ok(locked) => {
                    marker = Some(locked);
                    let start = ToolStart {
                        dir: Some(dir),
                        ..start
                    };
                    let started = self.app.now();
                    (self.drive_tool(&run, start).await, Some(started))
                }
                Err(err) => (Driven::Ended(Err(err)), None),
            },
            Some(Err(err)) => (Driven::Ended(Err(err)), None),
        };
        // A tool that was stopped is still ending, and in a directory shared with the next run (no `--root`) two agents
        // would work at once. So the run is recorded, and the next one claimed, only once the tool is gone.
        if matches!(driven, Driven::Over | Driven::Stopped) {
            self.tools.settled().await;
        }
        // The tool is gone: its own exit time, which a tool that never started doesn't have.
        let tool_ended = self.app.now();
        // What the tool started and left running would keep its CPU and ports and go on writing into the directory, so
        // it is ended before the marker lock is dropped and the run is recorded. The blocking scan runs off the loop.
        if tool_started.is_some() {
            let id = run.id.to_string();
            if let Err(err) = tokio::task::spawn_blocking(move || leftovers::end_run(&id)).await {
                tracing::warn!(run_id = %run.id, error = %err, "ending what the run left running failed");
            }
        }
        let tool_times = |request: FinishRequestDto| FinishRequestDto {
            tool_started_at: tool_started.map(|at| at.as_millisecond()),
            tool_ended_at: tool_started.map(|_| tool_ended.as_millisecond()),
            tool: tool_started.map(|_| self.tool.name().to_owned()),
            model: request.model.or_else(|| self.model.clone()),
            ..request
        };
        // Used until now, so a run longer than the unused period doesn't lose its directory the moment it ends.
        if let Some(marker) = marker
            && let Err(err) = marker.set_modified(SystemTime::from(self.app.now()))
        {
            tracing::warn!(run_id = %run.id, error = %err, "couldn't mark the run's directory as used");
        }
        let mut limit = None;
        let (request, not_started) = match driven {
            Driven::Ended(Ok(report)) => {
                tracing::debug!(run_id = %run.id, last_message = ?report.last_message, "the tool ended");
                limit = report.limit;
                // The server keeps no failure text for a limited run.
                let request = tool_times(FinishRequestDto {
                    cost_usd: report.cost_usd,
                    limited: limit.is_some(),
                    failure: report.failure,
                    model: report.model,
                    ..Default::default()
                });
                (request, None)
            }
            Driven::Ended(Err(err)) => {
                let request = FinishRequestDto {
                    cost_usd: None,
                    failure: Some(err.failure_text()),
                    ..Default::default()
                };
                (request, Some(err))
            }
            Driven::Over => (tool_times(FinishRequestDto::default()), None),
            Driven::Stopped => {
                let request = tool_times(FinishRequestDto {
                    failure: Some("Stopped by the person.".to_owned()),
                    ..Default::default()
                });
                (request, None)
            }
        };
        // Held before the end is sent, so no other worker claims while it is on its way, and said after the run's line.
        let held = limit.as_ref().and_then(|limit| self.hold_for(limit));
        self.finish_and_print(&run, &request, log.as_deref()).await?;
        // With `--once` the loop doesn't wait: it says when to run again instead.
        if let (Some(limit), Some(until)) = (&limit, held)
            && !self.once
        {
            self.note_hold(limit, until);
        }
        // Every run would fail the same way, so the worker does not go on.
        if let Some(err) = not_started {
            return Err(err.into());
        }
        Ok(limit.map_or(Next::Again, Next::Hold))
    }

    /// Makes `dir` if it isn't there and writes the marker that says a run started in it now. Returns the marker, which
    /// holds a lock for as long as it is kept.
    async fn prepare_directory(&self, run: &RunDto, dir: &Path) -> Result<std::fs::File, ToolError> {
        let (id, started) = (
            run.id.to_string(),
            jiff::Timestamp::from_second(self.app.now().as_second()),
        );
        let made = {
            let dir = dir.to_owned();
            let started = started.map_or_else(|_| String::new(), |at| at.to_string());
            let server = server_key(&self.server);
            tokio::task::spawn_blocking(move || kept::prepare(&dir, &id, &started, &server)).await
        };
        made.unwrap_or_else(|panicked| Err(std::io::Error::other(panicked)))
            .map_err(|source| ToolError::NoDirectory {
                dir: dir.to_owned(),
                source,
            })
    }

    /// Removes the run logs more than 7 days old if it has been `sweep_every` since the last time, or never has been:
    /// before the first claim, then now and then. A worker that finds it due marks it started and does it; the others
    /// carry on. Needs no `--root`.
    async fn prune_logs_if_due(&self) {
        let Some(config_dir) = &self.app.env.config_dir else {
            return;
        };
        {
            let mut last = lock(&self.last_log_prune);
            if last.is_some_and(|at| at.elapsed() < self.sweep_every) {
                return;
            }
            *last = Some(Instant::now());
        }
        let (runs_dir, now) = (logs::runs_dir(config_dir), SystemTime::from(self.app.now()));
        // A failed task is a log left for the next time.
        let _ = tokio::task::spawn_blocking(move || logs::prune(&runs_dir, now)).await;
    }

    /// Sweeps the root if it has been `sweep_every` since the last sweep, or never has been. A worker that finds one
    /// due marks it started and sweeps; the others find it done and carry on.
    async fn sweep_if_due(&self) {
        let Some(root) = &self.root else {
            return;
        };
        {
            let mut last = lock(&self.last_sweep);
            if last.is_some_and(|at| at.elapsed() < self.sweep_every) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.sweep(root).await;
    }

    /// Removes the kept directories of closed and missing items, and those unused for 7 days; see [`kept`]. Never
    /// fails: whatever goes wrong leaves a directory in place, or is a warning.
    async fn sweep(&self, root: &Path) {
        let now = SystemTime::from(self.app.now());
        let found = {
            let root = root.to_owned();
            tokio::task::spawn_blocking(move || {
                // What an interrupted sweep renamed and did not finish deleting.
                for left in kept::leftovers(&root) {
                    let removed = if left.is_dir() {
                        // Its delete most likely failed because something still wrote into it.
                        leftovers::end_inside(&left);
                        kept::delete(&left)
                    } else {
                        std::fs::remove_file(&left)
                    };
                    match removed {
                        Ok(()) => tracing::info!(dir = %left.display(), "cleared a half-removed directory"),
                        Err(err) => {
                            tracing::warn!(dir = %left.display(), error = %err, "couldn't clear a half-removed directory");
                        }
                    }
                }
                kept::list(&root, now)
            })
            .await
        };
        let Ok(found) = found else {
            return;
        };
        let server = server_key(&self.server);
        for directory in found {
            if self.is_stopping() {
                return;
            }
            if self.live_dirs.is_live(&directory.dir) {
                continue;
            }
            let reason = if kept::is_unused(directory.age) {
                kept::Reason::Unused
            } else if directory.server.as_deref() != Some(server.as_str()) {
                // Another server's item: this one can't say whether it is closed.
                continue;
            } else {
                let read = self.read_closed_at(&directory).await;
                match kept::verdict(read.as_ref().copied()) {
                    Verdict::Keep => {
                        if let Err(err) = &read {
                            tracing::debug!(dir = %directory.dir.display(), error = %err, "kept a directory: its item couldn't be read");
                        }
                        continue;
                    }
                    Verdict::KeepAndStop => return,
                    Verdict::Remove(kept::Reason::NotFound) => {
                        // The server answers 404 for a heap the caller can't see as well, such as another account's
                        // on a shared root. Only a heap that reads fine says the item itself is gone.
                        match self.read_heap_of(&directory).await {
                            Ok(()) => kept::Reason::NotFound,
                            Err(err) if is_unreachable(&err) => return,
                            Err(err) => {
                                tracing::debug!(dir = %directory.dir.display(), error = %err, "kept a directory: its heap couldn't be read");
                                continue;
                            }
                        }
                    }
                    Verdict::Remove(reason) => reason,
                }
            };
            self.remove_kept(root, &directory, reason).await;
        }
    }

    /// When the item of `directory` was closed, or `None` while it is open.
    async fn read_closed_at(&self, directory: &Kept) -> Result<Option<i64>, CliError> {
        let client = self.app.client(self.server.clone())?;
        // A token for each request, so a long sweep never keeps one past its expiry.
        let token = self.app.access_token(&client).await?;
        let handle = HeapHandle {
            team: directory.team.clone(),
            heap: directory.heap.clone(),
        };
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: &handle,
        };
        let item = api.get_item(&directory.key, &directory.key).await?;
        Ok(item.value.closed_at)
    }

    /// Reads the heap of `directory`, to tell an item that is gone from a heap the caller can't see.
    async fn read_heap_of(&self, directory: &Kept) -> Result<(), CliError> {
        let client = self.app.client(self.server.clone())?;
        let token = self.app.access_token(&client).await?;
        let handle = HeapHandle {
            team: directory.team.clone(),
            heap: directory.heap.clone(),
        };
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: &handle,
        };
        api.get::<HeapDto>(&[], NO_QUERY).await.map(|_| ())
    }

    /// Renames `directory` out of the way and deletes it, then its heap and team directories if that left them empty.
    /// The rename is what the lock covers: deleting a clone and its caches takes long.
    async fn remove_kept(&self, root: &Path, directory: &Kept, reason: kept::Reason) {
        let warn = |err: &dyn std::fmt::Display| {
            self.note(&format!("warning: couldn't remove {}: {err}", directory.dir.display()));
        };
        let away = match self.live_dirs.rename_away(root, &directory.dir, directory.modified) {
            // A run is in it, or started in it since the sweep listed it.
            None => return,
            Some(Ok(away)) => away,
            Some(Err(err)) => return warn(&err),
        };
        let deleted = {
            let away = away.clone();
            // What still works in the directory is ended first, or it keeps writing into it and the delete fails with
            // "Directory not empty". Done after the rename, which is what the lock covers.
            tokio::task::spawn_blocking(move || {
                leftovers::end_inside(&away);
                kept::delete(&away)
            })
            .await
        };
        // A directory left half-deleted under its new name is cleared by the next sweep.
        if let Err(err) = deleted.unwrap_or_else(|panicked| Err(std::io::Error::other(panicked))) {
            return self.note(&format!(
                "warning: couldn't remove {}: {err}. What's left is in {}; the next sweep tries again.",
                directory.dir.display(),
                away.display()
            ));
        }
        self.live_dirs.remove_emptied_parents(&directory.dir);
        tracing::info!(dir = %directory.dir.display(), reason = reason.text(), "removed a kept directory");
        if self.mode == Mode::Human {
            let now = self.local_time(self.app.now().as_millisecond());
            self.note(&format!("{now}  Removed {}: {}", directory.name(), reason.text()));
        }
    }

    /// The heap, for its context, and the item, as they are now.
    async fn read_heap_and_item(&self, run: &RunDto, handle: &HeapHandle) -> Result<(HeapDto, ItemDto), CliError> {
        let client = self.app.client(self.server.clone())?;
        let token = self.app.access_token(&client).await?;
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: handle,
        };
        let heap = api.get::<HeapDto>(&[], NO_QUERY).await?;
        // A token for each request, so a long loop never keeps one past its expiry.
        let token = self.app.access_token(&client).await?;
        let api = HeapApi {
            client: &client,
            token: &token,
            heap: handle,
        };
        let item = api.get_item(&run.item.key, &run.item.key).await?;
        Ok((heap.value, item.value))
    }

    /// Waits for the tool, and while it runs says every `heartbeat_every` that the run is alive. The time is kept by
    /// the wall clock and looked at every `heartbeat_check`, since the clock tokio keeps stands still while the
    /// machine sleeps: a heartbeat then goes out soon after it wakes. A heartbeat that fails is tried again at the
    /// next check until one is heard, and said once it has failed [`FAILURES_TO_SAY`] times in a row, so the one that
    /// goes out before the network is back after a wake isn't a false alarm. While a heartbeat's request is on its
    /// way, the tool ending and the stop signal are still watched. Gives up the tool (which stops it) when the server
    /// says the run is over, or when the stop signal says to stop now: the person's second Ctrl-C or a request to stop
    /// now. The first Ctrl-C only keeps the loop from claiming again.
    async fn drive_tool(&self, run: &RunDto, start: ToolStart) -> Driven {
        let mut tool = self.tools.run(start);
        let mut check = tokio::time::interval_at(Instant::now() + self.heartbeat_check, self.heartbeat_check);
        check.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut alive = true;
        // When the last heartbeat was tried, or the tool started.
        let mut last = self.app.now();
        // How many heartbeats in a row have failed. While any has, the next is due at the next check.
        let mut failures = 0_u32;
        loop {
            tokio::select! {
                result = &mut tool => return Driven::Ended(result),
                _ = check.tick(), if alive => {
                    if failures == 0 && !self.heartbeat_is_due(last) {
                        continue;
                    }
                    // The token is got first, whole (see `heartbeat_client`). The heartbeat can take as long as the
                    // client's timeout, so the tool ending and the stop signal are watched meanwhile.
                    let outcome = match self.heartbeat_client(run).await {
                        Ok((client, token)) => tokio::select! {
                            outcome = Self::send_heartbeat(&client, &token, run) => outcome,
                            result = &mut tool => return Driven::Ended(result),
                            () = self.stop_reaches(Stop::Now) => return Driven::Stopped,
                        },
                        Err(err) => Heartbeat::Failed(err),
                    };
                    last = self.app.now();
                    match outcome {
                        Heartbeat::Heard => {
                            if failures >= FAILURES_TO_SAY {
                                self.note(&format!(
                                    "{}  {}  The server hears the run again.",
                                    self.local_time(last.as_millisecond()),
                                    run.item.key
                                ));
                            }
                            failures = 0;
                        }
                        Heartbeat::Failed(err) => {
                            tracing::warn!(run_id = %run.id, item = %run.item.key, error = %err, "heartbeat failed");
                            failures = failures.saturating_add(1);
                            if failures == FAILURES_TO_SAY {
                                self.note(&format!(
                                    "{}  {}  warning: couldn't tell the server the run is still going: {}. \
                                     Trying again every {}.",
                                    self.local_time(last.as_millisecond()),
                                    run.item.key,
                                    one_line(&err.to_string()),
                                    spoken_duration(self.heartbeat_check),
                                ));
                            }
                        }
                        Heartbeat::Over => {
                            // A run that said reached or handed back is over too, with its tool still on its way
                            // out: that one is left to end, so its cost is recorded. Any other run that is over was
                            // taken back or lapsed.
                            if !self.has_ended_itself(run).await {
                                return Driven::Over;
                            }
                            alive = false;
                        }
                    }
                }
                () = self.stop_reaches(Stop::Now) => return Driven::Stopped,
            }
        }
    }

    /// Whether `heartbeat_every` has passed, by the wall clock, since `last`. A clock that has moved back counts as
    /// passed, so a clock change never stops the heartbeats.
    fn heartbeat_is_due(&self, last: jiff::Timestamp) -> bool {
        let passed = self.app.now().as_millisecond() - last.as_millisecond();
        let every = i64::try_from(self.heartbeat_every.as_millis()).unwrap_or(i64::MAX);
        passed < 0 || passed >= every
    }

    /// A client in `run` and a token for its heartbeat. Kept apart from sending it: getting the token may refresh the
    /// sign-in, which spends the old refresh token, so it must never be cut off halfway, while the heartbeat itself
    /// can be dropped at any point and simply sent again.
    async fn heartbeat_client(&self, run: &RunDto) -> Result<(ApiClient, String), CliError> {
        let client = self.app.client(self.server.clone())?.in_run(Some(run.id.to_string()));
        let token = self.app.access_token(&client).await?;
        Ok((client, token))
    }

    /// Sends a heartbeat for `run` and says how it went. Any failure but the run being over is returned for the
    /// caller to report, since the agent's own writes keep the claim alive too.
    async fn send_heartbeat(client: &ApiClient, token: &str, run: &RunDto) -> Heartbeat {
        let id = run.id.to_string();
        match client.post_empty(&["work", "runs", &id, "heartbeat"], token).await {
            Ok(()) => Heartbeat::Heard,
            Err(ClientError::Problem { problem, .. }) if problem.code == "run_over" => Heartbeat::Over,
            Err(err) => Heartbeat::Failed(err.into()),
        }
    }

    /// Whether `run` ended by the agent's own say: as reached, or handed back. Asked through `finish` with nothing to
    /// record, which leaves a run that has ended as it is and answers with it.
    async fn has_ended_itself(&self, run: &RunDto) -> bool {
        match self.finish(run, &FinishRequestDto::default()).await {
            Ok(finished) => matches!(finished.value.end, Some(RunEndDto::Reached | RunEndDto::HandedBack)),
            Err(_) => false,
        }
    }

    /// Records the end of `run` and prints its line. When the end can't be recorded: with `--once` and the server out
    /// of reach, the error ends the worker; otherwise a warning says so, and the run lapses on the server. `log` is the
    /// run's log, which a failed run's line points to when the file is there.
    async fn finish_and_print(
        &self,
        run: &RunDto,
        request: &FinishRequestDto,
        log: Option<&Path>,
    ) -> Result<(), CliError> {
        match self.finish_with_retry(run, request).await {
            Ok(finished) => {
                tracing::info!(run_id = %run.id, end = ?finished.value.end, "run ended");
                self.print_run(&finished, log).map_err(CliError::from)
            }
            Err(err) if self.once && is_unreachable(&err) => Err(err),
            Err(err) => {
                self.note(&format!("warning: couldn't record the end of run {}: {err}", run.id));
                Ok(())
            }
        }
    }

    /// Sends the end of the run, trying again `finish_retry` later when the server can't be reached.
    async fn finish_with_retry(&self, run: &RunDto, request: &FinishRequestDto) -> Result<Fetched<RunDto>, CliError> {
        let mut tries = 1;
        loop {
            match self.finish(run, request).await {
                Err(err) if is_unreachable(&err) && tries < FINISH_TRIES => {
                    tries += 1;
                    tokio::time::sleep(self.finish_retry).await;
                }
                result => return result,
            }
        }
    }

    async fn finish(&self, run: &RunDto, request: &FinishRequestDto) -> Result<Fetched<RunDto>, CliError> {
        let id = run.id.to_string();
        let client = self.app.client(self.server.clone())?.in_run(Some(id.clone()));
        let token = self.app.access_token(&client).await?;
        client
            .post::<RunDto>(&["work", "runs", &id, "finish"], request, &token)
            .await
            .map_err(|err| missing_agents(err, Some(&id)))
    }

    /// What a run that ended prints on stdout: its line, the key with `--quiet` (`team/heap/key` with several heaps),
    /// or the server's body with `--json`. The line, the failure under it and the log under that are written together,
    /// under one lock.
    fn print_run(&self, finished: &Fetched<RunDto>, log: Option<&Path>) -> std::io::Result<()> {
        let run = &finished.value;
        self.write(|out| match self.mode {
            Mode::Json => out.json(&finished.body),
            Mode::Quiet if self.heaps.len() > 1 => {
                let heap = &run.item.heap;
                out.line(&format!("{}/{}/{}", heap.team_slug, heap.slug, run.item.key))
            }
            Mode::Quiet => out.line(&run.item.key),
            Mode::Human => {
                let tone = if run.end == Some(RunEndDto::Failed) {
                    Tone::Alert
                } else {
                    Tone::Plain
                };
                let mut row = vec![
                    Cell::toned(self.local_time(run.started_at), Tone::Dim),
                    Cell::toned(run.item.key.clone(), Tone::Strong),
                    Cell::new(shorten(&run.item.title, TITLE_WIDTH)),
                    Cell::new(run.item.heap.name.clone()),
                    Cell::new(ended_journey(&run.item, run.end)),
                    Cell::toned(end_word(run.end).unwrap_or("ended"), tone),
                ];
                if let Some(ended_at) = run.ended_at {
                    row.push(Cell::new(format_duration(ended_at - run.started_at)));
                }
                if let Some(cost) = run.cost_usd {
                    row.push(Cell::new(format!("${cost:.2}")));
                }
                for line in out.render_table(&[row]) {
                    out.table_line("", &line)?;
                }
                if run.end == Some(RunEndDto::Failed)
                    && let Some(failure) = run.failure.as_deref()
                {
                    out.line(&format!("       {}", one_line(failure)))?;
                    if let Some(log) = log.filter(|log| log.exists()) {
                        out.line(&format!("       Log: {}", log.display()))?;
                    }
                }
                Ok(())
            }
        })
    }

    /// `14:02  k3x9a  Rotate the certs  Work  note → with context  started`, on stderr.
    fn note_started(&self, run: &RunDto) {
        if self.mode != Mode::Human {
            return;
        }
        let line = format!(
            "{}  {}  {}  {}  {}  started",
            self.local_time(run.started_at),
            run.item.key,
            shorten(&run.item.title, TITLE_WIDTH),
            run.item.heap.name,
            journey(&run.item)
        );
        // Nowhere left to say it if stderr is gone.
        let _ = self.write(|out| out.note_toned(&line, Tone::Dim));
    }

    /// `14:06  Nothing to do. 3 items wait for you: 2 in Work, 1 in House. Asking again in a minute.`, once for each
    /// stretch of idling across all workers, and not while a run is live: again only when what it says about the items
    /// that wait for the person changes. With one heap it says how many wait, not where.
    fn note_idle(&self, claim: &ClaimDto) -> std::io::Result<()> {
        // Idle is only news when nothing is going on: another worker's live run is not "nothing to do".
        if self.has_live_runs() {
            return Ok(());
        }
        let waiting = if self.heaps.len() > 1 {
            let by_heap: Vec<(&str, u32)> = claim
                .waiting
                .iter()
                .map(|in_heap| (in_heap.heap.name.as_str(), in_heap.count))
                .collect();
            waiting_by_heap_text(claim.waiting_count, &by_heap)
        } else {
            waiting_text(claim.waiting_count)
        };
        let said = waiting.unwrap_or_default();
        {
            let mut noted = lock(&self.noted);
            if noted.idle.as_ref() == Some(&said) {
                return Ok(());
            }
            noted.idle = Some(said.clone());
        }
        if self.mode != Mode::Human {
            return Ok(());
        }
        let mut text = String::from("Nothing to do.");
        if !said.is_empty() {
            text.push(' ');
            text.push_str(&said);
        }
        if !self.once {
            text.push_str(" Asking again in a minute.");
        }
        let now = self.local_time(self.app.now().as_millisecond());
        self.write(|out| {
            out.line_parts(&[
                (now.as_str(), Tone::Dim),
                ("  ", Tone::Plain),
                (text.as_str(), Tone::Plain),
            ])
        })
    }

    /// `14:07  Can't reach the server. Trying again in a minute.`, on stderr, once for each stretch of it.
    fn note_unreachable(&self) {
        {
            let mut noted = lock(&self.noted);
            if std::mem::replace(&mut noted.unreachable, true) {
                return;
            }
            // So the idle line is said again once the server is back, and this note isn't the last word.
            noted.idle = None;
        }
        let now = self.local_time(self.app.now().as_millisecond());
        self.note(&format!("{now}  Can't reach the server. Trying again in a minute."));
    }

    /// Starts the hold for `limit`, or moves its end later: until the limit resets and [`LIMIT_MARGIN`] more, or
    /// [`LIMIT_FALLBACK`] from now without a time. Never moves an end earlier. Returns the new end when the hold
    /// started or its end moved, for [`Self::note_hold`]; `None` when an existing hold already lasts as long.
    fn hold_for(&self, limit: &UsageLimit) -> Option<jiff::Timestamp> {
        let now = self.app.now();
        let until = hold_end(limit, now);
        let mut hold = lock(&self.hold);
        if hold.is_some_and(|held| held > now && held >= until) {
            return None;
        }
        *hold = Some(until);
        Some(until)
    }

    /// `12:07  Claude limit reached (five_hour), waiting until 12:21.`, in human mode.
    fn note_hold(&self, limit: &UsageLimit, until: jiff::Timestamp) {
        if self.mode != Mode::Human {
            return;
        }
        let now = self.app.now();
        let window = limit
            .window
            .as_ref()
            .map_or(String::new(), |window| format!(" ({window})"));
        let text = format!(
            "Claude limit reached{window}, waiting until {}.",
            self.local_time_or_date(until.as_millisecond())
        );
        let at = self.local_time(now.as_millisecond());
        // Nowhere left to say it if stdout is gone.
        let _ = self.write(|out| {
            out.line_parts(&[
                (at.as_str(), Tone::Dim),
                ("  ", Tone::Plain),
                (text.as_str(), Tone::Plain),
            ])
        });
    }

    /// `--once` after a run hit the limit: when to run again, on stderr. Nothing is waited for.
    fn note_limit_once(&self, limit: &UsageLimit) {
        if self.mode != Mode::Human {
            return;
        }
        let window = limit
            .window
            .as_ref()
            .map_or(String::new(), |window| format!(" ({window})"));
        let again = match limit.resets_at {
            Some(resets_at) => format!(
                "It resets at {}; run again after {}.",
                self.local_time_or_date(resets_at.as_millisecond()),
                self.local_time_or_date(hold_end(limit, self.app.now()).as_millisecond())
            ),
            None => format!("Try again in {}.", spoken_duration(LIMIT_FALLBACK)),
        };
        self.note(&format!("Claude limit reached{window}. {again}"));
    }

    /// Waits while a hold lasts, by the wall clock, looked at every `heartbeat_check` so a machine that slept through
    /// the reset carries on soon after it wakes. Clears the hold once it is over. False when the loop was told to stop,
    /// which ends the worker at once.
    async fn wait_for_hold(&self) -> bool {
        loop {
            let now = self.app.now();
            let remaining = {
                let mut hold = lock(&self.hold);
                match *hold {
                    None => return true,
                    Some(until) if until <= now => {
                        *hold = None;
                        return true;
                    }
                    Some(until) => {
                        Duration::from_millis(u64::try_from(until.as_millisecond() - now.as_millisecond()).unwrap_or(0))
                    }
                }
            };
            let wait = remaining.min(self.heartbeat_check);
            if tokio::time::timeout(wait, self.stop_reaches(Stop::AfterRuns))
                .await
                .is_ok()
            {
                return false;
            }
        }
    }

    /// Waits `idle_wait` before asking again. False when the loop was told to stop, which ends the worker at once.
    async fn wait_to_ask_again(&self) -> bool {
        tokio::time::timeout(self.idle_wait, self.stop_reaches(Stop::AfterRuns))
            .await
            .is_err()
    }

    /// [`Self::local_time`], or `Nov 15 12:21` when `ms` is on another day than now.
    fn local_time_or_date(&self, ms: i64) -> String {
        let zone = zone_of(self.app.env.time_zone.as_deref());
        let at = |ms| jiff::Timestamp::from_millisecond(ms).map(|at| at.to_zoned(zone.clone()));
        match (at(ms), at(self.app.now().as_millisecond())) {
            (Ok(there), Ok(today)) if there.date() != today.date() => there.strftime("%b %-d %H:%M").to_string(),
            _ => self.local_time(ms),
        }
    }

    /// `ms`, UTC milliseconds, as `14:02` in the machine's time zone, or UTC without one.
    fn local_time(&self, ms: i64) -> String {
        local_time(ms, self.app.env.time_zone.as_deref())
    }
}

/// When claiming may start again after `limit`: a minute past its reset, or half an hour from `now` without one. Never
/// less than a minute from `now`: a reset that has passed by this machine's clock (a skewed clock, a stale time) would
/// otherwise start run after run at the limit, and a limited run never makes its turn stuck.
fn hold_end(limit: &UsageLimit, now: jiff::Timestamp) -> jiff::Timestamp {
    let after = |from: jiff::Timestamp, wait: Duration| from.checked_add(wait).unwrap_or(from);
    match limit.resets_at {
        Some(at) => after(at.max(now), LIMIT_MARGIN),
        None => after(now, LIMIT_FALLBACK),
    }
}

/// Locks `mutex`. A worker that panicked while writing leaves nothing half-done worth refusing for, so poison is
/// ignored.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The time zone named by `zone`, or UTC when there is none or it is unknown.
fn zone_of(zone: Option<&str>) -> jiff::tz::TimeZone {
    zone.and_then(|name| jiff::tz::TimeZone::get(name).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC)
}

/// `ms`, UTC milliseconds, as `HH:MM` in the time zone named by `zone`, or in UTC when there is none or it is unknown.
fn local_time(ms: i64, zone: Option<&str>) -> String {
    match jiff::Timestamp::from_millisecond(ms) {
        Ok(at) => at.to_zoned(zone_of(zone)).strftime("%H:%M").to_string(),
        Err(_) => "--:--".to_owned(),
    }
}

/// How long a run took, from its length in milliseconds: `0m40s`, `2m10s`, `1h05m`.
pub(crate) fn format_duration(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    if seconds < 3600 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{:02}m", seconds / 3600, seconds % 3600 / 60)
    }
}

/// `text` on one line, so it can't forge another row.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn refused(status: u16) -> CliError {
        CliError::Client(ClientError::Problem {
            problem: Box::new(api_types::problem::Problem {
                code: "some_code".to_owned(),
                title: "Refused".to_owned(),
                status,
                field: None,
                hint: None,
                item_key: None,
                highest: None,
                request_id: None,
            }),
            body: String::new(),
        })
    }

    #[gtest]
    fn claudes_allow_rules_are_one_word_without_a_leading_dash() {
        expect_that!(check_allow_rule(Tool::Claude, "--allow", "Bash(npm:*)"), ok(anything()));
        for rule in ["", "-x", "Read,Edit", "Bash(npm run:*)", "Read\tEdit"] {
            expect_that!(
                check_allow_rule(Tool::Claude, "--allow", rule),
                err(matches_pattern!(UsageError::BadAllowRule { .. })),
                "{rule:?}"
            );
        }
    }

    #[gtest]
    fn copilots_allow_rules_may_hold_spaces_and_a_dash_but_not_be_empty() {
        for rule in ["shell(gh pr)", "-x"] {
            expect_that!(
                check_allow_rule(Tool::Copilot, "--allow", rule),
                ok(anything()),
                "{rule:?}"
            );
        }
        expect_that!(
            check_allow_rule(Tool::Copilot, "--allow-do", ""),
            err(matches_pattern!(UsageError::BadAllowRule { .. }))
        );
    }

    #[gtest]
    fn only_an_item_that_is_gone_lets_the_loop_go_on_after_a_failed_read() {
        expect_that!(is_about_the_item(&refused(404)), eq(true));
        expect_that!(is_about_the_item(&refused(403)), eq(true));
        expect_that!(is_about_the_item(&refused(401)), eq(false));
        expect_that!(is_about_the_item(&refused(500)), eq(false));
        expect_that!(is_about_the_item(&unreachable_error()), eq(false));
        expect_that!(is_about_the_item(&CliError::NoAgents), eq(false));
    }

    fn unreachable_error() -> CliError {
        // The cheapest `reqwest::Error` to make: a request to an address that isn't one.
        let source = reqwest::Client::new().get("not a url").build().map(|_| ()).unwrap_err();
        CliError::Client(ClientError::Unreachable {
            server: "http://127.0.0.1:1".to_owned(),
            source,
        })
    }

    #[gtest]
    fn a_server_out_of_reach_is_waited_for_unless_one_run_was_asked_for() {
        let unreachable = unreachable_error();

        expect_that!(after_failure(&unreachable, false), eq(&AfterFailure::TryAgain));
        expect_that!(after_failure(&unreachable, true), eq(&AfterFailure::Stop));
    }

    #[gtest]
    fn every_other_failure_ends_the_loop() {
        for err in [CliError::NoAgents, CliError::NoSuchRun { run: "x".to_owned() }] {
            expect_that!(after_failure(&err, false), eq(&AfterFailure::Stop));
            expect_that!(after_failure(&err, true), eq(&AfterFailure::Stop));
        }
    }

    #[gtest]
    fn heap_names_are_joined_with_commas_and_a_final_and() {
        expect_that!(join_names(&[]), eq(""));
        expect_that!(join_names(&["Work"]), eq("Work"));
        expect_that!(join_names(&["Work", "House"]), eq("Work and House"));
        expect_that!(join_names(&["Work", "House", "Garden"]), eq("Work, House and Garden"));
    }

    #[gtest]
    fn the_header_leaves_the_ctrl_c_sentence_out_with_once() {
        let heaps = vec![
            NamedHeap {
                handle: "ada/work".to_owned(),
                name: "Work".to_owned(),
            },
            NamedHeap {
                handle: "ada/house".to_owned(),
                name: "House".to_owned(),
            },
        ];

        expect_that!(
            header(&heaps, Tool::Claude, false, 1),
            eq("Working in Work and House with claude. Ctrl-C stops after the current run.")
        );
        expect_that!(
            header(&heaps, Tool::Claude, true, 1),
            eq("Working in Work and House with claude.")
        );
        expect_that!(
            header(&heaps, Tool::Claude, false, 3),
            eq("Working in Work and House with claude, 3 runs at a time. Ctrl-C stops after the current runs.")
        );
        expect_that!(
            header(&heaps, Tool::Claude, true, 3),
            eq("Working in Work and House with claude, 3 runs at a time.")
        );
    }

    #[gtest]
    fn a_duration_is_minutes_and_seconds_and_then_hours_and_minutes() {
        expect_that!(format_duration(0), eq("0m00s"));
        expect_that!(format_duration(40_000), eq("0m40s"));
        expect_that!(format_duration(130_000), eq("2m10s"));
        expect_that!(format_duration(3_599_999), eq("59m59s"));
        expect_that!(format_duration(3_900_000), eq("1h05m"));
        expect_that!(format_duration(-5), eq("0m00s"));
    }

    #[gtest]
    fn times_are_local_hours_and_minutes() {
        // 2027-01-01T12:00:00Z
        let noon = 1_798_804_800_000;

        expect_that!(local_time(noon, None), eq("12:00"));
        expect_that!(local_time(noon, Some("Europe/Berlin")), eq("13:00"));
        expect_that!(local_time(noon, Some("Not/AZone")), eq("12:00"));
    }

    #[gtest]
    fn the_hold_ends_a_minute_past_the_reset_and_never_sooner_than_a_minute_from_now() {
        let now = jiff::Timestamp::from_second(1_800_000_000).unwrap();
        let limit = |resets_at| UsageLimit {
            resets_at,
            window: None,
        };
        let later = |seconds| now.checked_add(jiff::SignedDuration::from_secs(seconds)).unwrap();

        expect_that!(hold_end(&limit(Some(later(3600))), now), eq(later(3660)));
        expect_that!(hold_end(&limit(Some(later(-3600))), now), eq(later(60)));
        expect_that!(hold_end(&limit(None), now), eq(later(30 * 60)));
    }

    #[gtest]
    fn a_failure_is_shown_on_one_line() {
        expect_that!(
            one_line("claude exited with code 1:\n  boom\r\n"),
            eq("claude exited with code 1: boom")
        );
    }
}
