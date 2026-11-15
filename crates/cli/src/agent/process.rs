//! Starting an agent tool's process, feeding it the brief, reading its output while watching for silence, and
//! stopping it. What differs between tools, the command line and what the output means, is passed in by each tool's
//! own module.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, ReadBuf};
use tokio::process::Command;
use tokio::time::Instant;

use super::{ToolError, ToolReport, ToolStart};
use crate::output::spoken_duration;

/// The most a run may cost, in US dollars: what the server accepts.
pub(super) const MAX_COST_USD: f64 = 1_000_000.0;

/// How the tool ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    Code(i32),
    /// Stopped by a signal, so there is no exit code.
    Signal,
}

/// The last line of `text` that is not blank, trimmed.
pub(super) fn last_line(text: &str) -> Option<&str> {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty())
}

/// What a tool reads as its instructions in an item's directory.
pub(super) const WORKSPACE_INSTRUCTIONS: &str = include_str!("workspace.md");

/// Makes `dir` if it isn't there, and the directories above it, and writes what the tool reads from the directory it
/// starts in: `instructions_file`, over any an earlier run left. Whatever else is in `dir` is left as it is.
fn set_up_directory(dir: &Path, instructions_file: &str) -> std::io::Result<()> {
    // `create_dir_all` succeeds on a directory that is there, which is the point: it is an item's, kept between runs.
    // It fails on a file in the way, which is a `NoDirectory` error to the caller.
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(instructions_file), WORKSPACE_INSTRUCTIONS)
}

/// How long a tool that was told to stop has to end before it is killed.
pub(super) const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// How long a killed tool has to be gone and reaped before it is counted out anyway.
#[cfg(unix)]
const REAP_LIMIT: std::time::Duration = std::time::Duration::from_secs(1);

/// How many tools that were told to stop have not ended yet, shared by every run a [`super::SystemTools`] starts, so
/// the loop can wait for them: a stopped tool still holds what it works in until it is gone.
#[derive(Clone, Debug)]
pub(super) struct StopTracker(std::sync::Arc<tokio::sync::watch::Sender<usize>>);

impl StopTracker {
    pub(super) fn new() -> Self {
        Self(std::sync::Arc::new(tokio::sync::watch::channel(0).0))
    }

    #[cfg(unix)]
    fn count_in(&self) {
        self.0.send_modify(|count| *count += 1);
    }

    #[cfg(unix)]
    fn count_out(&self) {
        self.0.send_modify(|count| *count = count.saturating_sub(1));
    }

    /// Waits until the count of tools being stopped is seen at 0, so one counted in while it waits is waited for too.
    pub(super) async fn settled(&self) {
        // `wait_for` looks at the current count before it waits, so a count that reached 0 already is not missed.
        let _ = self.0.subscribe().wait_for(|count| *count == 0).await;
    }
}

/// The tool's process. Dropped before the tool ended, it stops the tool: asked to end first, so the tool can end
/// what it started, then killed with its group.
///
/// Killing at once would not do: Claude runs each command in a process group of its own and ends them itself when
/// it is asked to end, but not when it is killed, so a build or a push under way would go on with nobody to end it.
struct Running {
    /// Only `None` while a stop has taken it to see the tool out.
    child: Option<tokio::process::Child>,
    /// The process group the tool leads, which `process_group(0)` gave it, until the tool has ended.
    #[cfg(unix)]
    group: Option<rustix::process::Pid>,
    /// Told when the tool is stopped, and again when it has ended.
    #[cfg_attr(not(unix), allow(dead_code))]
    stops: StopTracker,
}

impl Running {
    fn new(child: tokio::process::Child, stops: StopTracker) -> Self {
        Self {
            stops,
            #[cfg(unix)]
            group: child
                .id()
                .and_then(|id| i32::try_from(id).ok())
                .and_then(rustix::process::Pid::from_raw),
            child: Some(child),
        }
    }

    /// Takes the brief's pipe, and borrows the output pipes.
    fn pipes(&mut self) -> Pipes<'_> {
        match self.child.as_mut() {
            Some(child) => (child.stdin.take(), child.stdout.as_mut(), child.stderr.as_mut()),
            None => (None, None, None),
        }
    }

    /// Stops the tool and returns once it is gone: asked to end with its group, killed with it after `STOP_GRACE` if
    /// it has not. The tool stays in `self` while this waits, so that dropping the future mid-stop goes through the
    /// drop below: it is asked again, and seen out and killed with its group. After the tool is gone the drop has
    /// nothing left to do.
    async fn stop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        #[cfg(unix)]
        if let Some(group) = self.group {
            use rustix::process::{Signal, kill_process_group};
            // A group that is gone already is what was wanted.
            let _ = kill_process_group(group, Signal::TERM);
            if tokio::time::timeout(STOP_GRACE, child.wait()).await.is_err() {
                let _ = kill_process_group(group, Signal::KILL);
                if tokio::time::timeout(REAP_LIMIT, child.wait()).await.is_err() {
                    // Left in `self`, so the drop sees it out.
                    tracing::warn!("a killed tool was not gone after {REAP_LIMIT:?}");
                    return;
                }
            }
            // Reaped, so its group's number may be given to another: no more signals.
            self.group = None;
        }
        #[cfg(not(unix))]
        let _ = child.kill().await;
        self.child = None;
    }

    /// Waits for the tool to end on its own. After that nothing is left to stop.
    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let Some(child) = self.child.as_mut() else {
            return Err(std::io::Error::other("the tool was taken to be stopped"));
        };
        let status = child.wait().await;
        #[cfg(unix)]
        {
            self.group = None;
        }
        status
    }
}

/// The tool's pipes: the brief's to write, the output's to read.
type Pipes<'a> = (
    Option<tokio::process::ChildStdin>,
    Option<&'a mut tokio::process::ChildStdout>,
    Option<&'a mut tokio::process::ChildStderr>,
);

/// A tool that was asked to end and may not have yet. Killed with its group if it is dropped before it is seen out,
/// which is what happens to one whose waiting task the runtime shut down before it ran.
#[cfg(unix)]
struct Stopping {
    child: tokio::process::Child,
    group: rustix::process::Pid,
    ended: bool,
    stops: StopTracker,
}

#[cfg(unix)]
impl Stopping {
    /// Counts itself in; the count goes down again when it is dropped.
    fn new(child: tokio::process::Child, group: rustix::process::Pid, stops: StopTracker) -> Self {
        stops.count_in();
        Self {
            child,
            group,
            ended: false,
            stops,
        }
    }

    /// Waits up to `STOP_GRACE` for the tool to end, and kills its group when it has not. Blocks the thread.
    fn see_out(mut self) {
        let deadline = std::time::Instant::now() + STOP_GRACE;
        while std::time::Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                // It ended and was reaped, so its group's number may be given to another: nothing more to signal.
                self.ended = true;
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

#[cfg(unix)]
impl Drop for Stopping {
    fn drop(&mut self) {
        if !self.ended {
            // A group that is gone already is what was wanted.
            let _ = rustix::process::kill_process_group(self.group, rustix::process::Signal::KILL);
            // Counted out only once the leader is reaped, so that whoever waits finds it gone, not dying or a zombie.
            // Reaped, its group's number may be given to another: no more signals.
            let deadline = std::time::Instant::now() + REAP_LIMIT;
            loop {
                if !matches!(self.child.try_wait(), Ok(None)) {
                    self.ended = true;
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    tracing::warn!("a killed tool was not gone after {REAP_LIMIT:?}");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        // Last, so whoever waits for the tool to be gone finds it so.
        self.stops.count_out();
    }
}

#[cfg(unix)]
impl Drop for Running {
    fn drop(&mut self) {
        use rustix::process::{Signal, kill_process_group};
        let (Some(group), Some(child)) = (self.group, self.child.take()) else {
            return;
        };
        // A group that is gone already is what was wanted.
        let _ = kill_process_group(group, Signal::TERM);
        let stopping = Stopping::new(child, group, self.stops.clone());
        // A drop cannot await, and under a runtime this thread may be the only one, running every other run too: the
        // wait for the tool to end and the kill after it go to a blocking thread. The runtime waits for it when it
        // shuts down, so `upheap` does not exit with the tool still running. Outside a runtime there is nothing else
        // to hold up, and the wait is here.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(move || stopping.see_out());
            }
            Err(_) => stopping.see_out(),
        }
    }
}

/// What a tool makes of its own output.
pub(super) trait Output: Send {
    /// One line of stdout, as it came, newline included.
    fn take(&mut self, line: &[u8]);
    /// Whether the line that ends a stream has been seen.
    fn has_result(&self) -> bool;
    /// The report for a tool that ended with `exit`, having printed `stderr`.
    fn report(&self, exit: Exit, stderr: &str) -> ToolReport;
}

/// Starts `program` with `arguments` for `start`, in `start`'s directory when it names one, which is made and set
/// up first with `instructions_file` in it, writes the brief to its stdin and closes it, and reads its output to the
/// end, line by line as it comes. Every line of stdout goes to `start.log` when there is one, and to `output`, which
/// makes the report. The process leads a group of its own on Unix, so a Ctrl-C typed in the terminal reaches `upheap`
/// and not it. Dropping the future stops the process: it is asked to end, and killed if it has not within a few
/// seconds.
///
/// A tool that prints nothing, on stdout or stderr, for `silence_limit` is stopped the same way, and the run ends
/// with a failure that says so, once the process is gone. The limit counts on the monotonic clock, which stands
/// still while the machine sleeps, as the tool does: a sleep is not silence.
pub(super) async fn run(
    program: PathBuf,
    arguments: Vec<String>,
    instructions_file: &'static str,
    mut output: impl Output,
    start: ToolStart,
    stops: StopTracker,
    silence_limit: Duration,
) -> Result<ToolReport, ToolError> {
    let mut command = Command::new(&program);
    command.args(arguments);
    if let Some(dir) = start.dir {
        let made = dir.clone();
        let set_up = tokio::task::spawn_blocking(move || set_up_directory(&made, instructions_file)).await;
        if let Err(source) = set_up.unwrap_or_else(|panicked| Err(std::io::Error::other(panicked))) {
            return Err(ToolError::NoDirectory { dir, source });
        }
        command.current_dir(dir);
    }
    let log = match start.log {
        Some(path) => open_log(path).await,
        None => None,
    };
    command
        .envs(start.env.iter().map(|(name, value)| (name, value)))
        .env("PATH", path_with_this_binary_first())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let child = command.spawn().map_err(|source| ToolError::WouldNotStart {
        tool: start.tool,
        source,
    })?;
    let mut running = Running::new(child, stops);
    let activity = Activity::new();
    let mut stream = Stream { log };
    let mut stderr_bytes = Vec::new();
    let silent = {
        let (stdin, stdout, stderr) = running.pipes();
        // The output pipes stay the child's, read through borrows: were they this function's own, a stop would close
        // them before the tool is asked to end, and what the tool prints on its way out would kill it mid-cleanup.
        let brief = start.brief;
        // Written while the output is read, so a tool that prints before it has read everything cannot stall. A tool
        // that exits without reading its input is not an error here: its exit says what happened.
        let write = async move {
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(brief.as_bytes()).await;
                let _ = stdin.shutdown().await;
            }
        };
        let reading = async {
            tokio::join!(
                write,
                stream.read(stdout.map(|pipe| Watched::new(pipe, &activity)), &mut output),
                read_to_end(stderr.map(|pipe| Watched::new(pipe, &activity)), &mut stderr_bytes),
            );
        };
        // What was read before the silence stays in `output` and `stderr_bytes`, which the dropped reading only
        // borrowed.
        // Biased to `reading`: output that arrived while this future was not polled is read, and counts as activity,
        // before the silence is checked.
        tokio::select! {
            biased;
            () = reading => false,
            () = activity.silent_for(silence_limit) => true,
        }
    };
    let stderr_text = String::from_utf8_lossy(&stderr_bytes);
    if silent {
        running.stop().await;
        // A tool that printed its result and then hung has done its work: report that.
        return Ok(if output.has_result() {
            output.report(Exit::Code(0), &stderr_text)
        } else {
            ToolReport {
                failure: Some(format!(
                    "The agent showed no activity for {}.",
                    spoken_duration(silence_limit)
                )),
                ..ToolReport::default()
            }
        });
    }
    let exit = match running.wait().await {
        Ok(status) => status.code().map_or(Exit::Signal, Exit::Code),
        Err(source) => {
            return Ok(ToolReport {
                failure: Some(format!("{}'s exit couldn't be read: {source}", start.tool)),
                ..ToolReport::default()
            });
        }
    };
    Ok(output.report(exit, &stderr_text))
}

/// Opens the run's log for appending, with the directory it is in made. A log that can't be had is a warning and no
/// log: a run is never failed for it.
async fn open_log(path: PathBuf) -> Option<tokio::fs::File> {
    let opened = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::OpenOptions::new().create(true).append(true).open(path)
        })
        .await
    };
    match opened.unwrap_or_else(|panicked| Err(std::io::Error::other(panicked))) {
        Ok(file) => Some(tokio::fs::File::from_std(file)),
        Err(err) => {
            tracing::warn!(log = %path.display(), error = %err, "couldn't open the run's log; going on without it");
            None
        }
    }
}

/// Where the tool's stdout is kept as it is read.
struct Stream {
    /// The run's log; `None` without one, or once writing to it failed.
    log: Option<tokio::fs::File>,
}

impl Stream {
    /// Reads `pipe` to its end a line at a time, whatever the line's length, keeping each in the log and giving it to
    /// `output`. A blank line is not parsed.
    async fn read(&mut self, pipe: Option<Watched<'_, &mut tokio::process::ChildStdout>>, output: &mut impl Output) {
        let Some(pipe) = pipe else {
            return;
        };
        let mut pipe = BufReader::new(pipe);
        let mut line = Vec::new();
        loop {
            line.clear();
            // What was read before an error is still what the tool printed.
            match pipe.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    self.log_line(&line).await;
                    output.take(&line);
                }
            }
        }
    }

    /// Logs `line`, with its newline if it ended without one.
    async fn log_line(&mut self, line: &[u8]) {
        let Some(file) = self.log.as_mut() else {
            return;
        };
        let newline: &[u8] = if line.ends_with(b"\n") { b"" } else { b"\n" };
        // Flushed, so that what is in the log is what was read, even if the run is dropped at the next await.
        let written = async {
            file.write_all(line).await?;
            file.write_all(newline).await?;
            file.flush().await
        };
        if let Err(err) = written.await {
            tracing::warn!(error = %err, "couldn't write to the run's log; going on without it");
            self.log = None;
        }
    }
}

/// When the tool last printed, which the pipes it is read through keep up to date.
#[derive(Clone)]
struct Activity(Arc<Mutex<Instant>>);

impl Activity {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    fn mark(&self) {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Instant::now();
    }

    fn last(&self) -> Instant {
        *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ready once `limit` has passed with no activity.
    async fn silent_for(&self, limit: Duration) {
        loop {
            let deadline = self.last() + limit;
            if Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep_until(deadline).await;
        }
    }
}

/// A pipe that marks the activity whenever bytes come out of it, so a line still being written counts.
struct Watched<'a, R> {
    pipe: R,
    activity: &'a Activity,
}

impl<'a, R> Watched<'a, R> {
    fn new(pipe: R, activity: &'a Activity) -> Self {
        Self { pipe, activity }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Watched<'_, R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let polled = Pin::new(&mut self.pipe).poll_read(cx, buf);
        if matches!(polled, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            self.activity.mark();
        }
        polled
    }
}

async fn read_to_end(pipe: Option<impl AsyncRead + Unpin>, bytes: &mut Vec<u8>) {
    if let Some(mut pipe) = pipe {
        // What was read before an error is still what the tool printed.
        let _ = pipe.read_to_end(bytes).await;
    }
}

/// `PATH` with the directory of the running `upheap` first, so the `upheap` the agent runs is this one and the two
/// cannot differ in version.
fn path_with_this_binary_first() -> std::ffi::OsString {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let this_binary_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let Some(dir) = this_binary_dir else {
        return current;
    };
    let dirs = std::iter::once(dir).chain(std::env::split_paths(&current));
    std::env::join_paths(dirs).unwrap_or(current)
}
