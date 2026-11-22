//! The boundary between `work run` and the agent tool it starts: what a run hands the tool, what comes back, and the
//! real tool, [`SystemTools`], which starts a process. The loop talks to [`AgentTools`] only, so tests pass a stand-in
//! and a second tool is an addition here, not a change to the loop.

mod claude;
mod copilot;
pub(crate) mod leftovers;
mod process;
mod version;

use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

pub use version::Version;

/// An agent tool `work run` can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Tool {
    /// Claude's command-line tool, `claude`.
    Claude,
    /// GitHub Copilot's command-line tool, `copilot`.
    Copilot,
}

impl Tool {
    /// The tool's name as the person types it, and as its program is called.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// What the tool may do in a run: `Prepare` for a turn that goes to `with_context` or `defined` (run `upheap`, read
/// files, use the web), `Do` for one that goes to `done` (edit files, and whatever the person's own settings allow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Prepare,
    Do,
}

/// What one run hands the tool.
#[derive(Clone, Debug)]
pub struct ToolStart {
    pub tool: Tool,
    pub access: Access,
    /// The instructions and what the tool works on, as one text for its standard input.
    pub brief: String,
    /// Variables to add to the tool's environment: the run, the server and the heap.
    pub env: Vec<(String, String)>,
    /// A directory for the tool to set up for itself and run in, made if it is missing. It may hold an earlier run's
    /// work, which is kept. `None` to run in the directory `work run` was started in, as it is.
    pub dir: Option<PathBuf>,
    /// Allow rules the person added for this run's access (`--allow` and the turn's own flag), in the tool's syntax,
    /// on top of the tool's own. Already checked to be one rule each.
    pub allow: Vec<String>,
    /// A file for the tool's streamed output, one event a line, made with the directory it is in. `None` for no log.
    /// A log that can't be written is a warning, not a failure of the run.
    pub log: Option<PathBuf>,
    /// The model to hand the tool as `--model`, as given. `None` to leave it to the tool's own default.
    pub model: Option<String>,
}

/// What the tool reported when it ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolReport {
    /// What the run cost in US dollars; `None` when the tool reported no cost, or one that cannot be believed.
    pub cost_usd: Option<f64>,
    /// Why the tool itself failed; `None` when it ended normally. Whether the run reached its goal is the server's to
    /// say, not the tool's.
    pub failure: Option<String>,
    /// The tool's last message. Logged at `DEBUG`, never shown.
    pub last_message: Option<String>,
    /// Set when the run failed because the account hit the tool's usage limit.
    pub limit: Option<UsageLimit>,
    /// The model the tool's own stream named; `None` when it named none.
    pub model: Option<String>,
}

/// An account's usage limit the tool stopped at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageLimit {
    /// When the limit resets; `None` when the tool didn't say.
    pub resets_at: Option<jiff::Timestamp>,
    /// Which window was reached, as the tool names it (`five_hour`, `seven_day`); `None` when not said.
    pub window: Option<String>,
}

/// Why a tool could not be started.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{tool} isn't installed, or isn't on PATH")]
    NotInstalled { tool: Tool },
    #[error("{tool} wouldn't start")]
    WouldNotStart {
        tool: Tool,
        #[source]
        source: std::io::Error,
    },
    #[error("{tool} is version {found}, and work run needs {needed} or newer")]
    TooOld {
        tool: Tool,
        found: Version,
        needed: Version,
    },
    #[error("couldn't read {tool}'s version: `{} --version` printed {printed:?}", program.display())]
    UnreadableVersion {
        tool: Tool,
        program: PathBuf,
        printed: String,
    },
    #[error("couldn't make a directory for the run at {}", dir.display())]
    NoDirectory {
        dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// `root` as an absolute path, made if it isn't there: where `work run --root` puts each run's directory. Checked
/// once, before the first claim, so a root that can't be used fails before any item is taken.
pub fn make_root(root: &Path) -> Result<PathBuf, ToolError> {
    let no_directory = |source| ToolError::NoDirectory {
        dir: root.to_owned(),
        source,
    };
    let root = std::path::absolute(root).map_err(no_directory)?;
    std::fs::create_dir_all(&root).map_err(no_directory)?;
    Ok(root)
}

impl ToolError {
    /// The error and its cause on one line, as a run's failure text says it.
    pub fn failure_text(&self) -> String {
        match std::error::Error::source(self) {
            Some(source) => format!("{self}: {source}"),
            None => self.to_string(),
        }
    }
}

/// The agent tools a machine has.
pub trait AgentTools: Send + Sync {
    /// Whether `tool` can be started here, and is at least [`Tool::min_version`]. Called once, before the first claim.
    fn check(&self, tool: Tool) -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send + '_>>;

    /// Runs `tool` to its end. Dropping the future stops the tool.
    fn run(&self, start: ToolStart) -> Pin<Box<dyn Future<Output = Result<ToolReport, ToolError>> + Send>>;

    /// Waits until every tool this has stopped has ended or been killed. A stopped tool takes up to a few seconds to
    /// end, and until it has, it still holds what it worked in. Ready at once when nothing was stopped.
    fn settled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::ready(()))
    }
}

/// Why a pull request couldn't be read.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PullRequestError(pub String);

/// The pull requests a machine can read, with its own `gh`.
pub trait PullRequests: Send + Sync {
    /// What `gh pr view <url> --json state,mergeable,comments,reviews,statusCheckRollup` prints, as JSON text.
    fn view(&self, url: &str) -> Pin<Box<dyn Future<Output = Result<String, PullRequestError>> + Send + '_>>;
}

/// The pull requests of this machine: `gh`, signed in as the person.
pub struct SystemPullRequests;

impl PullRequests for SystemPullRequests {
    fn view(&self, url: &str) -> Pin<Box<dyn Future<Output = Result<String, PullRequestError>> + Send + '_>> {
        let url = url.to_owned();
        Box::pin(async move {
            let output = tokio::process::Command::new("gh")
                .args([
                    "pr",
                    "view",
                    &url,
                    "--json",
                    "state,mergeable,comments,reviews,statusCheckRollup",
                ])
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|err| match err.kind() {
                    std::io::ErrorKind::NotFound => PullRequestError("gh isn't installed, or isn't on PATH".to_owned()),
                    _ => PullRequestError(format!("gh wouldn't start: {err}")),
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let last = stderr
                    .lines()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or_default();
                return Err(PullRequestError(format!("gh {}: {}", output.status, last.trim())));
            }
            String::from_utf8(output.stdout)
                .map_err(|_| PullRequestError("gh printed text that isn't UTF-8".to_owned()))
        })
    }
}

/// An interruption of the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interrupt {
    /// The person pressed Ctrl-C. The first one stops the loop after the current run, the second stops the run.
    CtrlC,
    /// Stop at once, as a second Ctrl-C does: the process was asked to end (`SIGTERM`).
    Now,
}

/// Starts listening for interruptions and returns them. Called once, by `work run`, so no other command has its
/// Ctrl-C taken over.
pub type ListenForInterrupts = Box<dyn FnOnce() -> UnboundedReceiver<Interrupt> + Send>;

/// What `work run` takes from outside.
pub struct WorkDeps {
    pub tools: Box<dyn AgentTools>,
    pub interrupts: ListenForInterrupts,
    /// How long to wait before asking again when nothing is left to do. 60 seconds in the binary.
    pub idle_wait: Duration,
    /// How often a live run says so, by the wall clock. 5 minutes in the binary: a run lapses after 30.
    pub heartbeat_every: Duration,
    /// How often the loop looks at the clock to see whether a heartbeat is due, and so how often it tries again
    /// while heartbeats fail (a try that ran into the client's timeout is followed by the next at once). 30 seconds
    /// in the binary.
    pub heartbeat_check: Duration,
    /// How long to wait between tries to record a run's end. 5 seconds in the binary.
    pub finish_retry: Duration,
    /// How long between sweeps of the `--root` for directories to remove. 1 hour in the binary.
    pub sweep_every: Duration,
    /// The pull requests the loop checks for open watches.
    pub pull_requests: Box<dyn PullRequests>,
    /// How long between checks of the open watches. 60 seconds in the binary.
    pub watch_every: Duration,
}

impl WorkDeps {
    /// The real tools, Ctrl-C and `SIGTERM`, and the waits the binary uses.
    pub fn system() -> Self {
        Self {
            tools: Box::new(SystemTools::new()),
            interrupts: Box::new(listen_for_signals),
            idle_wait: Duration::from_secs(60),
            heartbeat_every: Duration::from_secs(5 * 60),
            heartbeat_check: Duration::from_secs(30),
            finish_retry: Duration::from_secs(5),
            sweep_every: Duration::from_secs(60 * 60),
            pull_requests: Box::new(SystemPullRequests),
            watch_every: Duration::from_secs(60),
        }
    }
}

/// Forwards Ctrl-C, and on Unix `SIGTERM`, until the receiver is dropped. Listening starts here, not in the spawned
/// task, so a signal that arrives right away is not missed. Must run inside the runtime.
///
/// `SIGHUP` is left alone: listening for it would undo `nohup`, which is how a loop is kept going after its terminal
/// closes.
fn listen_for_signals() -> UnboundedReceiver<Interrupt> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let (Ok(mut ctrl_c), Ok(mut terminate)) = (signal(SignalKind::interrupt()), signal(SignalKind::terminate()))
        {
            tokio::spawn(async move {
                loop {
                    let interrupt = tokio::select! {
                        _ = ctrl_c.recv() => Interrupt::CtrlC,
                        _ = terminate.recv() => Interrupt::Now,
                    };
                    if sender.send(interrupt).is_err() {
                        return;
                    }
                }
            });
        }
    }
    #[cfg(not(unix))]
    tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            if sender.send(Interrupt::CtrlC).is_err() {
                return;
            }
        }
    });
    receiver
}

/// How long a tool may print nothing before it is stopped and its run failed, in the binary.
const SILENCE_LIMIT: Duration = Duration::from_secs(30 * 60);

/// The tools of this machine: processes started from the programs on `PATH`.
pub struct SystemTools {
    /// The program to start for [`Tool::Claude`] in place of the one on `PATH`. For tests.
    claude: Option<PathBuf>,
    /// The program to start for [`Tool::Copilot`] in place of the one on `PATH`. For tests.
    copilot: Option<PathBuf>,
    /// The tools stopped and not yet ended.
    stops: process::StopTracker,
    /// How long a tool may print nothing before it is stopped and its run failed. 30 minutes in the binary.
    silence_limit: Duration,
}

impl SystemTools {
    pub fn new() -> Self {
        Self {
            claude: None,
            copilot: None,
            stops: process::StopTracker::new(),
            silence_limit: SILENCE_LIMIT,
        }
    }

    /// Tools whose `claude` is the program at `path`, wherever `PATH` points.
    pub fn with_claude(path: PathBuf) -> Self {
        Self {
            claude: Some(path),
            copilot: None,
            stops: process::StopTracker::new(),
            silence_limit: SILENCE_LIMIT,
        }
    }

    /// Tools whose `copilot` is the program at `path`, wherever `PATH` points.
    pub fn with_copilot(path: PathBuf) -> Self {
        Self {
            claude: None,
            copilot: Some(path),
            stops: process::StopTracker::new(),
            silence_limit: SILENCE_LIMIT,
        }
    }

    /// These tools, stopping a tool after `limit` without output. For tests.
    pub fn with_silence_limit(self, limit: Duration) -> Self {
        Self {
            silence_limit: limit,
            ..self
        }
    }

    fn program(&self, tool: Tool) -> Result<PathBuf, ToolError> {
        let overridden = match tool {
            Tool::Claude => &self.claude,
            Tool::Copilot => &self.copilot,
        };
        match overridden {
            Some(path) if is_runnable(path) => Ok(path.clone()),
            Some(_) => Err(ToolError::NotInstalled { tool }),
            None => find_on_path(tool.name(), std::env::var_os("PATH")).ok_or(ToolError::NotInstalled { tool }),
        }
    }
}

impl Default for SystemTools {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentTools for SystemTools {
    fn check(&self, tool: Tool) -> Pin<Box<dyn Future<Output = Result<(), ToolError>> + Send + '_>> {
        Box::pin(async move {
            let program = self.program(tool)?;
            let output = tokio::process::Command::new(&program)
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|source| ToolError::WouldNotStart { tool, source })?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let Some(found) = version::parse(&stdout) else {
                let printed = format!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
                return Err(ToolError::UnreadableVersion {
                    tool,
                    program,
                    printed: printed.trim().to_owned(),
                });
            };
            let needed = tool.min_version();
            if found < needed {
                return Err(ToolError::TooOld { tool, found, needed });
            }
            Ok(())
        })
    }

    fn run(&self, start: ToolStart) -> Pin<Box<dyn Future<Output = Result<ToolReport, ToolError>> + Send>> {
        let program = self.program(start.tool);
        let stops = self.stops.clone();
        let silence_limit = self.silence_limit;
        Box::pin(async move {
            match start.tool {
                Tool::Claude => claude::run(program?, start, stops, silence_limit).await,
                Tool::Copilot => copilot::run(program?, start, stops, silence_limit).await,
            }
        })
    }

    fn settled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.stops.settled())
    }
}

/// The first directory of `path_var` with an executable file called `name` in it.
fn find_on_path(name: &str, path_var: Option<OsString>) -> Option<PathBuf> {
    let path_var = path_var?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|candidate| is_runnable(candidate))
}

/// Whether `path` is a file the process can start: on Unix, one with an execute bit.
fn is_runnable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

#[cfg(test)]
mod tests;
