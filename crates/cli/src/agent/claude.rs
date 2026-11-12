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
pub(super) fn arguments(access: Access) -> Vec<&'static str> {
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
    arguments
}

/// The `upheap` commands a run needs, as allow rules: reading, writing its item, asking, and saying reached. Not all
/// of `upheap`: text an agent reads could talk it into `upheap auth logout`, which the server has no say in.
const UPHEAP_COMMANDS: [&str; 9] = [
    "Bash(upheap item:*)",
    "Bash(upheap step:*)",
    "Bash(upheap question:*)",
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

/// Kills the tool's whole process group when dropped before the tool ended: the tool and everything it started, a
/// build or a push under way included. Killing the tool alone would leave those running with nobody to end them.
#[cfg(unix)]
struct GroupGuard(Option<rustix::process::Pid>);

#[cfg(unix)]
impl GroupGuard {
    /// The group `child` leads, which `process_group(0)` gave it: its number is the child's own.
    fn of(child: &tokio::process::Child) -> Self {
        Self(
            child
                .id()
                .and_then(|id| i32::try_from(id).ok())
                .and_then(rustix::process::Pid::from_raw),
        )
    }

    fn disarm(&mut self) {
        self.0 = None;
    }
}

#[cfg(unix)]
impl Drop for GroupGuard {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            // A group that is gone already is what was wanted.
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
    }
}

/// Starts `program` as Claude with `start`'s access, writes the brief to its stdin and closes it, and reads it to its
/// end. The process leads a group of its own on Unix, so a Ctrl-C typed in the terminal reaches `upheap` and not
/// it. Dropping the future kills the process and everything it started.
pub(super) async fn run(program: PathBuf, start: ToolStart) -> Result<ToolReport, ToolError> {
    let mut command = Command::new(&program);
    command
        .args(arguments(start.access))
        .envs(start.env.iter().map(|(name, value)| (name, value)))
        .env("PATH", path_with_this_binary_first())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|source| ToolError::WouldNotStart {
        tool: Tool::Claude,
        source,
    })?;
    #[cfg(unix)]
    let mut group = GroupGuard::of(&child);
    let stdin = child.stdin.take();
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
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
    let waited = child.wait().await;
    // It ended on its own: nothing is left to stop, and its group's number may be given to another.
    #[cfg(unix)]
    group.disarm();
    let exit = match waited {
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
            arguments(Access::Prepare),
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
            ])
        );
    }

    #[gtest]
    fn the_arguments_for_doing_are_exactly_these() {
        expect_that!(
            arguments(Access::Do),
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
                "Bash(upheap work reached:*)",
                "Bash(upheap guide:*)",
                "Bash(upheap heap show:*)",
                "Bash(upheap heap list:*)",
                "Bash(upheap tag list:*)",
                "Bash(upheap parameter list:*)",
            ])
        );
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
