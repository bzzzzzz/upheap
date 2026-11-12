//! The boundary between `work run` and the agent tool it starts: what a run hands the tool, what comes back, and the
//! real tool, [`SystemTools`], which starts a process. The loop talks to [`AgentTools`] only, so tests pass a stand-in
//! and a second tool is an addition here, not a change to the loop.

mod claude;

use std::ffi::OsString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

/// An agent tool `work run` can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Tool {
    /// Claude's command-line tool, `claude`.
    Claude,
}

impl Tool {
    /// The tool's name as the person types it, and as its program is called.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
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
    /// Whether `tool` can be started here. Called once, before the first claim.
    fn check(&self, tool: Tool) -> Result<(), ToolError>;

    /// Runs `tool` to its end. Dropping the future stops the tool.
    fn run(&self, start: ToolStart) -> Pin<Box<dyn Future<Output = Result<ToolReport, ToolError>> + Send>>;
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
    /// How often a live run says so. 5 minutes in the binary: a run lapses after 30.
    pub heartbeat_every: Duration,
    /// How long to wait between tries to record a run's end. 5 seconds in the binary.
    pub finish_retry: Duration,
}

impl WorkDeps {
    /// The real tools, Ctrl-C and `SIGTERM`, and the waits the binary uses.
    pub fn system() -> Self {
        Self {
            tools: Box::new(SystemTools::new()),
            interrupts: Box::new(listen_for_signals),
            idle_wait: Duration::from_secs(60),
            heartbeat_every: Duration::from_secs(5 * 60),
            finish_retry: Duration::from_secs(5),
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

/// The tools of this machine: processes started from the programs on `PATH`.
pub struct SystemTools {
    /// The program to start for [`Tool::Claude`] in place of the one on `PATH`. For tests.
    claude: Option<PathBuf>,
}

impl SystemTools {
    pub fn new() -> Self {
        Self { claude: None }
    }

    /// Tools whose `claude` is the program at `path`, wherever `PATH` points.
    pub fn with_claude(path: PathBuf) -> Self {
        Self { claude: Some(path) }
    }

    fn program(&self, tool: Tool) -> Result<PathBuf, ToolError> {
        let Tool::Claude = tool;
        match &self.claude {
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
    fn check(&self, tool: Tool) -> Result<(), ToolError> {
        self.program(tool).map(|_| ())
    }

    fn run(&self, start: ToolStart) -> Pin<Box<dyn Future<Output = Result<ToolReport, ToolError>> + Send>> {
        let program = self.program(start.tool);
        Box::pin(async move {
            match start.tool {
                Tool::Claude => claude::run(program?, start).await,
            }
        })
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
