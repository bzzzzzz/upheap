//! Claude's command-line tool as an agent: the command line for each kind of access, starting it, reading what it
//! streams while it works, and stopping it when it goes silent. The permission flags are the ones in the README's agent
//! paragraph, kept in `arguments`.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, ReadBuf};
use tokio::process::Command;
use tokio::time::Instant;

use super::{Access, Tool, ToolError, ToolReport, ToolStart};
use crate::output::{shorten, spoken_duration};

/// The most a run may cost, in US dollars: what the server accepts.
const MAX_COST_USD: f64 = 1_000_000.0;

/// How the tool ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    Code(i32),
    /// Stopped by a signal, so there is no exit code.
    Signal,
}

/// The arguments `claude` starts with: stream what happens as one JSON event a line, ending with a `result` event,
/// and read the prompt from stdin, keep the run out of the person's sessions, and refuse (not ask about) what isn't
/// allowed, since no one is there to ask.
///
/// `Prepare` may run `upheap`, read files under the directory it runs in (a bare `Read` would allow any path on the
/// machine) and use the web. `Do` may run `upheap` and has file edits accepted; every
/// other command follows the allow rules in the person's own and the project's Claude settings.
///
/// In a directory made for the run (`own_directory`), both get [`IN_OWN_DIRECTORY`] too: any command, edit and read.
/// The rules go on the command line because Claude ignores the allow rules of a settings file in a directory nobody
/// has trusted, which a new directory never is.
///
/// The person's own `allow` rules come right after all of these, as more values of `--allowedTools`, in the order
/// given. `upheap auth` is denied last, in every run: a rule of the person's (or their own settings) may allow `Bash`
/// wholesale, and Claude's deny wins over any allow.
pub(super) fn arguments(access: Access, own_directory: bool, allow: &[String]) -> Vec<&str> {
    let mut arguments = vec![
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--permission-mode",
    ];
    match access {
        Access::Prepare => {
            arguments.extend(["dontAsk", "--allowedTools"]);
            arguments.extend(UPHEAP_COMMANDS);
            arguments.extend(["Read(./**)", "Grep(./**)", "Glob(./**)", "WebFetch", "WebSearch"]);
        }
        Access::Do => {
            arguments.extend(["acceptEdits", "--allowedTools"]);
            arguments.extend(UPHEAP_COMMANDS);
        }
    }
    if own_directory {
        arguments.extend(IN_OWN_DIRECTORY);
    }
    arguments.extend(allow.iter().map(String::as_str));
    arguments.extend(["--disallowedTools", "Bash(upheap auth:*)"]);
    arguments
}

/// What a run may do in a directory made for it, as allow rules: nearly everything. A run there has to clone and
/// build what it works on, and which commands that takes isn't known yet. `upheap auth` stays refused by the deny
/// every run gets, for the reason [`UPHEAP_COMMANDS`] leaves it out.
const IN_OWN_DIRECTORY: [&str; 6] = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];

/// The `upheap` commands a run needs, as allow rules: reading, writing its item, asking, and saying reached or
/// handing the turn back. Not all of `upheap`: text an agent reads could talk it into `upheap auth logout`, which
/// the server has no say in.
const UPHEAP_COMMANDS: [&str; 11] = [
    "Bash(upheap item:*)",
    "Bash(upheap step:*)",
    "Bash(upheap question:*)",
    "Bash(upheap comment:*)",
    "Bash(upheap work reached:*)",
    "Bash(upheap work hand-back:*)",
    "Bash(upheap guide:*)",
    "Bash(upheap heap show:*)",
    "Bash(upheap heap list:*)",
    "Bash(upheap tag list:*)",
    "Bash(upheap parameter list:*)",
];

/// What the tool's exit and output say: its cost, whether it failed, and its last message. `result_line` is the line
/// of its stream that held the `result` event, if it printed one.
///
/// It ended normally when it exited 0 and its JSON says `is_error` is not true; otherwise it failed, and the failure
/// is named by what is known, best first: the subtype of the error it reported, its exit code with the last line of
/// its stderr, a signal, or output that could not be read. The cost is believed only from 0 to a million dollars.
pub(super) fn read_report(exit: Exit, result_line: Option<&str>, stderr: &str) -> ToolReport {
    let output: Option<Value> = result_line
        .and_then(|line| serde_json::from_str(line.trim()).ok())
        .filter(Value::is_object);
    let field = |name: &str| output.as_ref().and_then(|output| output.get(name));
    let is_error = field("is_error").and_then(Value::as_bool).unwrap_or(false);
    let subtype = field("subtype").and_then(Value::as_str);
    let cost_usd = field("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|cost| (0.0..=MAX_COST_USD).contains(cost));
    let last_message = field("result").and_then(Value::as_str).map(str::to_owned);
    let failure = match exit {
        Exit::Code(0) if output.is_none() => Some("claude's output couldn't be read.".to_owned()),
        Exit::Code(0) if !is_error => None,
        _ if is_error && (subtype == Some("success") || (subtype.is_none() && matches!(exit, Exit::Code(0)))) => {
            // Failures of the API or the login come as a `success` result with `is_error` set and the reason as its text.
            let text = last_message
                .as_deref()
                .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "));
            Some(match text.filter(|text| !text.is_empty()) {
                Some(text) => format!("claude reported an error: {}", shorten(&text, 200)),
                None => "claude reported an error.".to_owned(),
            })
        }
        _ if is_error && subtype.is_some() => subtype.map(|subtype| format!("claude ended with {subtype}.")),
        Exit::Code(0) => Some("claude reported an error.".to_owned()),
        Exit::Code(code) => Some(match last_line(stderr) {
            Some(line) => format!("claude exited with code {code}: {line}"),
            None => format!("claude exited with code {code}."),
        }),
        Exit::Signal => Some("claude was stopped by a signal.".to_owned()),
    };
    ToolReport {
        cost_usd,
        failure,
        last_message,
    }
}

/// The last line of `text` that is not blank, trimmed.
fn last_line(text: &str) -> Option<&str> {
    text.lines().rev().map(str::trim).find(|line| !line.is_empty())
}

/// What Claude reads as its instructions in an item's directory.
pub(super) const WORKSPACE_INSTRUCTIONS: &str = include_str!("claude/workspace.md");

/// Makes `dir` if it isn't there, and the directories above it, and writes what Claude reads from the directory it
/// starts in: `CLAUDE.md`, over any an earlier run left. Whatever else is in `dir` is left as it is.
fn set_up_directory(dir: &Path) -> std::io::Result<()> {
    // `create_dir_all` succeeds on a directory that is there, which is the point: it is an item's, kept between runs.
    // It fails on a file in the way, which is a `NoDirectory` error to the caller.
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("CLAUDE.md"), WORKSPACE_INSTRUCTIONS)
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

/// Starts `program` as Claude with `start`'s access, in `start`'s directory when it names one, which is made and set
/// up first, writes the brief to its stdin and closes it, and reads its output to the end, line by line as it comes.
/// Every line of stdout goes to `start.log` when there is one; the report is read from the stream's `result` event.
/// The process leads a group of its own on Unix, so a Ctrl-C typed in the terminal reaches `upheap` and not it.
/// Dropping the future stops the process: it is asked to end, and killed if it has not within a few seconds.
///
/// A tool that prints nothing, on stdout or stderr, for `silence_limit` is stopped the same way, and the run ends
/// with a failure that says so, once the process is gone. The limit counts on the monotonic clock, which stands
/// still while the machine sleeps, as the tool does: a sleep is not silence.
pub(super) async fn run(
    program: PathBuf,
    start: ToolStart,
    stops: StopTracker,
    silence_limit: Duration,
) -> Result<ToolReport, ToolError> {
    let mut command = Command::new(&program);
    command.args(arguments(start.access, start.dir.is_some(), &start.allow));
    if let Some(dir) = start.dir {
        let made = dir.clone();
        let set_up = tokio::task::spawn_blocking(move || set_up_directory(&made)).await;
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
        tool: Tool::Claude,
        source,
    })?;
    let mut running = Running::new(child, stops);
    let activity = Activity::new();
    let mut stream = Stream { result_line: None, log };
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
                stream.read(stdout.map(|pipe| Watched::new(pipe, &activity))),
                read_to_end(stderr.map(|pipe| Watched::new(pipe, &activity)), &mut stderr_bytes),
            );
        };
        // What was read before the silence stays in `stream` and `stderr_bytes`, which the dropped reading only
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
    let result_line = stream.result_line.as_deref().map(String::from_utf8_lossy);
    if silent {
        running.stop().await;
        // A tool that printed its result and then hung has done its work: report that.
        return Ok(match result_line {
            Some(line) => read_report(Exit::Code(0), Some(&line), &stderr_text),
            None => ToolReport {
                failure: Some(format!(
                    "The agent showed no activity for {}.",
                    spoken_duration(silence_limit)
                )),
                ..ToolReport::default()
            },
        });
    }
    let exit = match running.wait().await {
        Ok(status) => status.code().map_or(Exit::Signal, Exit::Code),
        Err(source) => {
            return Ok(ToolReport {
                failure: Some(format!("claude's exit couldn't be read: {source}")),
                ..ToolReport::default()
            });
        }
    };
    Ok(read_report(exit, result_line.as_deref(), &stderr_text))
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

/// What the tool's stdout has said so far, and where it is kept.
struct Stream {
    /// The last line that was a `result` event.
    result_line: Option<Vec<u8>>,
    /// The run's log; `None` without one, or once writing to it failed.
    log: Option<tokio::fs::File>,
}

impl Stream {
    /// Reads `pipe` to its end a line at a time, whatever the line's length, keeping each in the log and the last
    /// `result` event. A blank line is not parsed.
    async fn read(&mut self, pipe: Option<Watched<'_, &mut tokio::process::ChildStdout>>) {
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
                Ok(_) => self.take(&line).await,
            }
        }
    }

    /// Logs `line`, with its newline if it ended without one, and remembers it when it is the `result` event.
    async fn take(&mut self, line: &[u8]) {
        self.log_line(line).await;
        if is_result_line(line) {
            self.result_line = Some(line.to_vec());
        }
    }

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

/// Whether `line` is a JSON object whose `type` is `result`: the event that ends a stream. Anything else, a line that
/// is not JSON or not UTF-8 included, is not.
fn is_result_line(line: &[u8]) -> bool {
    serde_json::from_slice::<Value>(line)
        .is_ok_and(|event| event.is_object() && event.get("type").and_then(Value::as_str) == Some("result"))
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

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_arguments_for_preparing_are_exactly_these() {
        expect_that!(
            arguments(Access::Prepare, false, &[]),
            eq(&vec![
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--permission-mode",
                "dontAsk",
                "--allowedTools",
                "Bash(upheap item:*)",
                "Bash(upheap step:*)",
                "Bash(upheap question:*)",
                "Bash(upheap comment:*)",
                "Bash(upheap work reached:*)",
                "Bash(upheap work hand-back:*)",
                "Bash(upheap guide:*)",
                "Bash(upheap heap show:*)",
                "Bash(upheap heap list:*)",
                "Bash(upheap tag list:*)",
                "Bash(upheap parameter list:*)",
                "Read(./**)",
                "Grep(./**)",
                "Glob(./**)",
                "WebFetch",
                "WebSearch",
                "--disallowedTools",
                "Bash(upheap auth:*)",
            ])
        );
    }

    #[gtest]
    fn the_arguments_for_doing_are_exactly_these() {
        expect_that!(
            arguments(Access::Do, false, &[]),
            eq(&vec![
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "Bash(upheap item:*)",
                "Bash(upheap step:*)",
                "Bash(upheap question:*)",
                "Bash(upheap comment:*)",
                "Bash(upheap work reached:*)",
                "Bash(upheap work hand-back:*)",
                "Bash(upheap guide:*)",
                "Bash(upheap heap show:*)",
                "Bash(upheap heap list:*)",
                "Bash(upheap tag list:*)",
                "Bash(upheap parameter list:*)",
                "--disallowedTools",
                "Bash(upheap auth:*)",
            ])
        );
    }

    const AUTH_DENY: [&str; 2] = ["--disallowedTools", "Bash(upheap auth:*)"];

    /// The arguments for `access` outside its own directory and with no rules of the person's, up to the deny.
    fn narrow(access: Access) -> Vec<&'static str> {
        let mut narrow = arguments(access, false, &[]);
        narrow.truncate(narrow.len() - AUTH_DENY.len());
        narrow
    }

    #[gtest]
    fn in_its_own_directory_either_access_may_do_anything_but_upheap_auth() {
        let wide = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &wide, &AUTH_DENY].concat();

            // The wide rules follow the narrow ones, so they are still values of `--allowedTools`.
            expect_that!(arguments(access, true, &[]), eq(&expected));
        }
    }

    #[gtest]
    fn extra_allow_rules_follow_the_fixed_ones_and_upheap_auth_is_still_denied_after_them() {
        let extra = ["mcp__x".to_owned(), "Bash".to_owned()];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &["mcp__x", "Bash"], &AUTH_DENY].concat();

            expect_that!(arguments(access, false, &extra), eq(&expected));
        }
    }

    #[gtest]
    fn in_its_own_directory_extra_rules_come_after_the_wide_ones_and_before_the_auth_deny() {
        let extra = ["mcp__x".to_owned(), "Bash(npm:*)".to_owned()];
        let wide = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &wide, &["mcp__x", "Bash(npm:*)"], &AUTH_DENY].concat();

            expect_that!(arguments(access, true, &extra), eq(&expected));
        }
    }

    /// A row of the table: a name, the exit, the result line (empty for none) and stderr, and the cost and failure
    /// they make.
    type Case<'a> = (&'a str, Exit, &'a str, &'a str, Option<f64>, Option<&'a str>);

    /// What `read_report` makes of an exit and its output: the cost and the failure.
    #[gtest]
    fn the_report_follows_the_table_of_outputs() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.31,"result":"Done."}"#;
        let budget = r#"{"subtype":"error_max_budget_usd","is_error":true,"total_cost_usd":1.5}"#;
        let tokens_only = r#"{"subtype":"success","is_error":false,"usage":{"input_tokens":10}}"#;
        let long = format!(r#"{{"is_error":true,"result":"{}"}}"#, "x".repeat(300));
        let long_failure = format!("claude reported an error: {}…", "x".repeat(199));
        let cases: Vec<Case> = vec![
            ("a normal end with a cost", Exit::Code(0), ok, "", Some(0.31), None),
            (
                "is_error with a subtype, exit 1, cost still counted",
                Exit::Code(1),
                budget,
                "ignored",
                Some(1.5),
                Some("claude ended with error_max_budget_usd."),
            ),
            (
                "is_error with a subtype, exit 0",
                Exit::Code(0),
                budget,
                "",
                Some(1.5),
                Some("claude ended with error_max_budget_usd."),
            ),
            (
                "a non-zero exit, no output: the last line of stderr",
                Exit::Code(2),
                "",
                "warming up\n\nboom: bad flag  \n\n",
                None,
                Some("claude exited with code 2: boom: bad flag"),
            ),
            (
                "a non-zero exit, nothing on stderr",
                Exit::Code(3),
                "",
                " \n",
                None,
                Some("claude exited with code 3."),
            ),
            (
                "a non-zero exit with a JSON output that is not an error",
                Exit::Code(1),
                ok,
                "crashed late",
                Some(0.31),
                Some("claude exited with code 1: crashed late"),
            ),
            (
                "stopped by a signal",
                Exit::Signal,
                "",
                "",
                None,
                Some("claude was stopped by a signal."),
            ),
            (
                "exit 0 with rubbish",
                Exit::Code(0),
                "not json at all",
                "",
                None,
                Some("claude's output couldn't be read."),
            ),
            (
                "exit 0 with JSON that is not an object",
                Exit::Code(0),
                "[1,2]",
                "",
                None,
                Some("claude's output couldn't be read."),
            ),
            (
                "exit 0, is_error with no subtype",
                Exit::Code(0),
                r#"{"is_error":true}"#,
                "",
                None,
                Some("claude reported an error."),
            ),
            (
                "exit 0, is_error with a success subtype and the reason as its result, made one line",
                Exit::Code(0),
                r#"{"subtype":"success","is_error":true,"result":"Invalid API key\n  Please run /login"}"#,
                "",
                None,
                Some("claude reported an error: Invalid API key Please run /login"),
            ),
            (
                "exit 0, is_error with no subtype and a long result: shortened",
                Exit::Code(0),
                &long,
                "",
                None,
                Some(&long_failure),
            ),
            (
                "exit 0, is_error with a success subtype and no result",
                Exit::Code(0),
                r#"{"subtype":"success","is_error":true}"#,
                "",
                None,
                Some("claude reported an error."),
            ),
            (
                "a cost over the limit is no cost",
                Exit::Code(0),
                r#"{"is_error":false,"total_cost_usd":1000000.5}"#,
                "",
                None,
                None,
            ),
            (
                "a negative cost is no cost",
                Exit::Code(0),
                r#"{"is_error":false,"total_cost_usd":-0.1}"#,
                "",
                None,
                None,
            ),
            (
                "a cost that is not a number is no cost",
                Exit::Code(0),
                r#"{"is_error":false,"total_cost_usd":"0.3"}"#,
                "",
                None,
                None,
            ),
            (
                "zero is a cost",
                Exit::Code(0),
                r#"{"is_error":false,"total_cost_usd":0}"#,
                "",
                Some(0.0),
                None,
            ),
            (
                "the limit itself is a cost",
                Exit::Code(0),
                r#"{"is_error":false,"total_cost_usd":1000000}"#,
                "",
                Some(1_000_000.0),
                None,
            ),
            ("only tokens: no cost", Exit::Code(0), tokens_only, "", None, None),
        ];

        for (name, exit, stdout, stderr, cost, failure) in cases {
            let report = read_report(exit, Some(stdout).filter(|line| !line.is_empty()), stderr);

            expect_that!(report.cost_usd, eq(cost), "{name}: cost");
            expect_that!(report.failure.as_deref(), eq(failure), "{name}: failure");
        }
    }

    #[gtest]
    fn the_last_message_is_the_result_when_there_is_one() {
        let report = read_report(Exit::Code(0), Some(r#"{"is_error":false,"result":"All done"}"#), "");

        expect_that!(report.last_message, some(eq("All done")));
        expect_that!(read_report(Exit::Code(0), Some("{}"), "").last_message, none());
    }

    #[gtest]
    fn only_a_json_object_of_type_result_is_the_result_line() {
        expect_that!(is_result_line(br#"{"type":"result","is_error":false}"#), eq(true));
        expect_that!(is_result_line(b"{\"type\":\"result\"}\n"), eq(true));
        expect_that!(is_result_line(br#"{"type":"assistant","message":{}}"#), eq(false));
        expect_that!(is_result_line(br#"{"is_error":false}"#), eq(false));
        expect_that!(is_result_line(br#"["result"]"#), eq(false));
        expect_that!(is_result_line(b"not json"), eq(false));
        expect_that!(is_result_line(b"\n"), eq(false));
        expect_that!(is_result_line(b"{\"type\":\"result\",\"result\":\"\xff\"}"), eq(false));
    }

    #[gtest]
    #[tokio::test]
    async fn a_stream_keeps_the_last_result_event_and_ignores_the_rest() {
        // Arrange
        let mut stream = Stream {
            result_line: None,
            log: None,
        };
        let lines: [&[u8]; 5] = [
            br#"{"type":"system","subtype":"init"}"#,
            br#"{"type":"result","result":"first"}"#,
            b"warming up",
            br#"{"type":"assistant"}"#,
            br#"{"type":"result","result":"last"}"#,
        ];

        // Act
        for line in lines {
            stream.take(line).await;
        }

        // Assert
        expect_that!(
            stream.result_line.as_deref(),
            some(eq(&br#"{"type":"result","result":"last"}"#[..]))
        );
    }

    #[gtest]
    #[tokio::test]
    async fn a_stream_with_no_result_event_has_no_result_line() {
        let mut stream = Stream {
            result_line: None,
            log: None,
        };

        stream.take(br#"{"type":"assistant"}"#).await;

        expect_that!(stream.result_line, none());
    }
}
