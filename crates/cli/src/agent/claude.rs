//! Claude's command-line tool as an agent: the command line for each kind of access, starting it, and reading what it
//! printed. The permission flags are the ones in the README's agent paragraph, kept in `arguments`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::{Access, Tool, ToolError, ToolReport, ToolStart};

/// The most a run may cost, in US dollars: what the server accepts.
const MAX_COST_USD: f64 = 1_000_000.0;

/// How the tool ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Exit {
    Code(i32),
    /// Stopped by a signal, so there is no exit code.
    Signal,
}

/// The arguments `claude` starts with: print the answer as one JSON object and read the prompt from stdin, keep the
/// run out of the person's sessions, and refuse (not ask about) what isn't allowed, since no one is there to ask.
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
        "json",
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

/// The `upheap` commands a run needs, as allow rules: reading, writing its item, asking, and saying reached. Not all
/// of `upheap`: text an agent reads could talk it into `upheap auth logout`, which the server has no say in.
const UPHEAP_COMMANDS: [&str; 10] = [
    "Bash(upheap item:*)",
    "Bash(upheap step:*)",
    "Bash(upheap question:*)",
    "Bash(upheap comment:*)",
    "Bash(upheap work reached:*)",
    "Bash(upheap guide:*)",
    "Bash(upheap heap show:*)",
    "Bash(upheap heap list:*)",
    "Bash(upheap tag list:*)",
    "Bash(upheap parameter list:*)",
];

/// What the tool's exit and output say: its cost, whether it failed, and its last message.
///
/// It ended normally when it exited 0 and its JSON says `is_error` is not true; otherwise it failed, and the failure
/// is named by what is known, best first: the subtype of the error it reported, its exit code with the last line of
/// its stderr, a signal, or output that could not be read. The cost is believed only from 0 to a million dollars.
pub(super) fn read_report(exit: Exit, stdout: &str, stderr: &str) -> ToolReport {
    let output: Option<Value> = serde_json::from_str(stdout.trim()).ok().filter(Value::is_object);
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

/// What Claude reads as its instructions in a run's own directory.
const WORKSPACE_INSTRUCTIONS: &str = include_str!("claude/workspace.md");

/// Makes `dir`, which must not exist yet, with what Claude reads from the directory it starts in: `CLAUDE.md`. Makes
/// the directories above it too.
fn set_up_directory(dir: &Path) -> std::io::Result<()> {
    if let Some(root) = dir.parent() {
        std::fs::create_dir_all(root)?;
    }
    // Not `create_dir_all`: a directory that is there already is someone else's, and its files are not written over.
    std::fs::create_dir(dir)?;
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
/// up first, writes the brief to its stdin and closes it, and reads it to its
/// end. The process leads a group of its own on Unix, so a Ctrl-C typed in the terminal reaches `upheap` and not
/// it. Dropping the future stops the process: it is asked to end, and killed if it has not within a few seconds.
pub(super) async fn run(program: PathBuf, start: ToolStart, stops: StopTracker) -> Result<ToolReport, ToolError> {
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
    let (stdin, stdout, stderr) = running.pipes();
    // The output pipes stay the child's, read through borrows: were they this function's own, a stop would close
    // them before the tool is asked to end, and what the tool prints on its way out would kill it mid-cleanup.
    let brief = start.brief;
    // Written while the output is read, so a tool that prints before it has read everything cannot stall. A tool that
    // exits without reading its input is not an error here: its exit says what happened.
    let write = async move {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(brief.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
    };
    let (_, stdout, stderr) = tokio::join!(write, read_to_end(stdout), read_to_end(stderr));
    let exit = match running.wait().await {
        Ok(status) => status.code().map_or(Exit::Signal, Exit::Code),
        Err(source) => {
            return Ok(ToolReport {
                failure: Some(format!("claude's exit couldn't be read: {source}")),
                ..ToolReport::default()
            });
        }
    };
    Ok(read_report(
        exit,
        &String::from_utf8_lossy(&stdout),
        &String::from_utf8_lossy(&stderr),
    ))
}

async fn read_to_end(pipe: Option<impl tokio::io::AsyncRead + Unpin>) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        // What was read before an error is still what the tool printed.
        let _ = pipe.read_to_end(&mut bytes).await;
    }
    bytes
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
                "json",
                "--no-session-persistence",
                "--permission-mode",
                "dontAsk",
                "--allowedTools",
                "Bash(upheap item:*)",
                "Bash(upheap step:*)",
                "Bash(upheap question:*)",
                "Bash(upheap comment:*)",
                "Bash(upheap work reached:*)",
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
                "json",
                "--no-session-persistence",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "Bash(upheap item:*)",
                "Bash(upheap step:*)",
                "Bash(upheap question:*)",
                "Bash(upheap comment:*)",
                "Bash(upheap work reached:*)",
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

    /// A row of the table: a name, the exit, stdout and stderr, and the cost and failure they make.
    type Case<'a> = (&'a str, Exit, &'a str, &'a str, Option<f64>, Option<&'a str>);

    /// What `read_report` makes of an exit and its output: the cost and the failure.
    #[gtest]
    fn the_report_follows_the_table_of_outputs() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.31,"result":"Done."}"#;
        let budget = r#"{"subtype":"error_max_budget_usd","is_error":true,"total_cost_usd":1.5}"#;
        let tokens_only = r#"{"subtype":"success","is_error":false,"usage":{"input_tokens":10}}"#;
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
            let report = read_report(exit, stdout, stderr);

            expect_that!(report.cost_usd, eq(cost), "{name}: cost");
            expect_that!(report.failure.as_deref(), eq(failure), "{name}: failure");
        }
    }

    #[gtest]
    fn the_last_message_is_the_result_when_there_is_one() {
        let report = read_report(Exit::Code(0), r#"{"is_error":false,"result":"All done"}"#, "");

        expect_that!(report.last_message, some(eq("All done")));
        expect_that!(read_report(Exit::Code(0), "{}", "").last_message, none());
    }
}
