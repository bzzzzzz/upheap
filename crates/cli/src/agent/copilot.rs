//! GitHub Copilot's command-line tool as an agent: its command line for each kind of access, and what its stream
//! says. Starting the process, and watching and stopping it, are in [`super::process`]. Only Claude's usage limit is
//! recognised: a Copilot run that stops at its limit is a failed run like any other.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::process::{self, Exit, MAX_COST_USD, Output, StopTracker, last_line};
use super::{Access, ToolError, ToolReport, ToolStart};

/// US dollars in one nano AI unit, the unit of the usage Copilot reports: a billion of them make an AI credit, and 100
/// credits are 1 dollar. The 100 is the owner's working figure, not GitHub's published price.
const USD_PER_NANO_AIU: f64 = 1e-11;

/// The arguments `copilot` starts with: one JSON event a line, never asking the person (no one is there to ask), the
/// session kept off the account's sync, and every `upheap` command allowed but `upheap auth` and `upheap heap use`.
///
/// A Copilot allow rule can't scope a program by subcommand (`shell(upheap item:*)` allows nothing), only the whole
/// program, while a deny rule can. With `UPHEAP_RUN` set the server already refuses anything outside the run's own
/// item, so only the two commands that write on this machine, which the server has no say in, are denied, last:
/// Copilot's deny wins over every allow.
///
/// `Prepare` may use the web; reads under the directory it runs in need no rule. `Do` has file edits accepted, and
/// every other command follows the person's own saved Copilot approvals. In a directory made for the run
/// (`own_directory`), both may use any tool and the web. The person's own `allow` rules come right after, each as one
/// `--allow-tool=<rule>` argument, so a rule that starts with `-` or holds a space isn't split, and the flag's
/// several values can't swallow what follows.
///
/// A `model` goes in as `--model <model>`, as given, after the four leading flags.
pub(super) fn arguments(access: Access, own_directory: bool, allow: &[String], model: Option<&str>) -> Vec<String> {
    let mut arguments: Vec<String> = ["--output-format", "json", "--no-ask-user", "--sync-session=off"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if let Some(model) = model {
        arguments.extend(["--model".to_owned(), model.to_owned()]);
    }
    arguments.push("--allow-tool=shell(upheap:*)".to_owned());
    arguments.push(
        match access {
            Access::Prepare => "--allow-all-urls",
            Access::Do => "--allow-tool=write",
        }
        .to_owned(),
    );
    if own_directory {
        arguments.extend(["--allow-all-tools", "--allow-all-urls"].map(str::to_owned));
    }
    arguments.extend(allow.iter().map(|rule| format!("--allow-tool={rule}")));
    arguments.extend(LOCAL_WRITES_DENIED.map(str::to_owned));
    arguments
}

/// The `upheap` commands that change the person's own sign-in or config, as deny rules: text an agent reads could
/// talk it into signing the person out or pointing their default heap elsewhere.
const LOCAL_WRITES_DENIED: [&str; 2] = [
    "--deny-tool=shell(upheap auth:*)",
    "--deny-tool=shell(upheap heap use:*)",
];

/// What the tool's exit and output say: its cost and whether it failed. `nano_aiu` is the total usage of the last
/// `session.usage_checkpoint` line, if there was one.
///
/// The cost is believed only from 0 to a million dollars. The run failed when the process did: a non-zero exit with
/// the last line of its stderr, or a signal. Its stream's `result` line is not read for that.
fn read_report(exit: Exit, nano_aiu: Option<f64>, stderr: &str) -> ToolReport {
    let cost_usd = nano_aiu
        .map(|nano| nano * USD_PER_NANO_AIU)
        .filter(|cost| (0.0..=MAX_COST_USD).contains(cost));
    let failure = match exit {
        Exit::Code(0) => None,
        Exit::Code(code) => Some(match last_line(stderr) {
            Some(line) => format!("copilot exited with code {code}: {line}"),
            None => format!("copilot exited with code {code}."),
        }),
        Exit::Signal => Some("copilot was stopped by a signal.".to_owned()),
    };
    ToolReport {
        cost_usd,
        failure,
        ..ToolReport::default()
    }
}

/// What Copilot's stdout has said so far.
#[derive(Default)]
struct Stream {
    /// The session's running usage in nano AI units, from the last `session.usage_checkpoint` line; `None` before
    /// one, or when it held no number.
    nano_aiu: Option<f64>,
    /// The model of the last `session.usage_checkpoint` line (`data.accountingSnapshot.currentModel`); `None` before
    /// one, or when it named none.
    model: Option<String>,
}

impl Output for Stream {
    /// Remembers the usage of `line` when it is a `session.usage_checkpoint` event.
    fn take(&mut self, line: &[u8]) {
        // Most lines are small deltas, and there are many: only a line that names the event is parsed.
        const EVENT: &[u8] = b"session.usage_checkpoint";
        if !line.windows(EVENT.len()).any(|window| window == EVENT) {
            return;
        }
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if event.get("type").and_then(Value::as_str) == Some("session.usage_checkpoint") {
            let data = event.get("data");
            self.nano_aiu = data.and_then(|data| data.get("totalNanoAiu")).and_then(Value::as_f64);
            self.model = data
                .and_then(|data| data.get("accountingSnapshot"))
                .and_then(|snapshot| snapshot.get("currentModel"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }

    fn has_result(&self) -> bool {
        false
    }

    fn report(&self, exit: Exit, stderr: &str) -> ToolReport {
        ToolReport {
            model: self.model.clone(),
            ..read_report(exit, self.nano_aiu, stderr)
        }
    }
}

/// Starts `program` as Copilot with `start`'s access; the cost is read from the stream's usage checkpoints. How the
/// process is run is in [`process::run`].
pub(super) async fn run(
    program: PathBuf,
    start: ToolStart,
    stops: StopTracker,
    silence_limit: Duration,
) -> Result<ToolReport, ToolError> {
    let arguments = arguments(start.access, start.dir.is_some(), &start.allow, start.model.as_deref());
    process::run(
        program,
        arguments,
        "AGENTS.md",
        Stream::default(),
        start,
        stops,
        silence_limit,
    )
    .await
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn strings(arguments: &[&str]) -> Vec<String> {
        arguments.iter().map(|argument| (*argument).to_owned()).collect()
    }

    #[gtest]
    fn the_arguments_for_preparing_are_exactly_these() {
        expect_that!(
            arguments(Access::Prepare, false, &[], None),
            eq(&strings(&[
                "--output-format",
                "json",
                "--no-ask-user",
                "--sync-session=off",
                "--allow-tool=shell(upheap:*)",
                "--allow-all-urls",
                "--deny-tool=shell(upheap auth:*)",
                "--deny-tool=shell(upheap heap use:*)",
            ]))
        );
    }

    #[gtest]
    fn the_arguments_for_doing_are_exactly_these() {
        expect_that!(
            arguments(Access::Do, false, &[], None),
            eq(&strings(&[
                "--output-format",
                "json",
                "--no-ask-user",
                "--sync-session=off",
                "--allow-tool=shell(upheap:*)",
                "--allow-tool=write",
                "--deny-tool=shell(upheap auth:*)",
                "--deny-tool=shell(upheap heap use:*)",
            ]))
        );
    }

    #[gtest]
    fn in_its_own_directory_either_access_may_use_any_tool_and_the_deny_is_still_last() {
        for access in [Access::Prepare, Access::Do] {
            let mut expected = arguments(access, false, &[], None);
            let denies = expected.split_off(expected.len() - LOCAL_WRITES_DENIED.len());
            expected.extend(strings(&["--allow-all-tools", "--allow-all-urls"]));
            expected.extend(denies);

            expect_that!(arguments(access, true, &[], None), eq(&expected));
        }
    }

    #[gtest]
    fn the_persons_rules_follow_one_argument_each_before_the_deny() {
        let allow = ["shell(gh pr)".to_owned(), "-x".to_owned()];
        for access in [Access::Prepare, Access::Do] {
            let mut expected = arguments(access, true, &[], None);
            let denies = expected.split_off(expected.len() - LOCAL_WRITES_DENIED.len());
            expected.extend(strings(&["--allow-tool=shell(gh pr)", "--allow-tool=-x"]));
            expected.extend(denies);

            expect_that!(arguments(access, true, &allow, None), eq(&expected));
        }
    }

    #[gtest]
    fn a_model_goes_in_as_two_arguments_after_the_four_leading_flags() {
        for access in [Access::Prepare, Access::Do] {
            let mut expected = arguments(access, false, &[], None);
            expected.splice(4..4, strings(&["--model", "opus"]));

            expect_that!(arguments(access, false, &[], Some("opus")), eq(&expected));
        }
    }

    #[gtest]
    fn the_last_checkpoints_current_model_is_reported_and_one_without_it_leaves_none() {
        let mut stream = Stream::default();
        stream.take(
            br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":1,"accountingSnapshot":{"currentModel":"auto"}}}"#,
        );
        stream.take(
            br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":2,"accountingSnapshot":{"currentModel":"mai-code-1.1-flash"}}}"#,
        );

        expect_that!(stream.report(Exit::Code(0), "").model, some(eq("mai-code-1.1-flash")));

        stream.take(br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":3}}"#);

        expect_that!(stream.report(Exit::Code(0), "").model, none());
    }

    #[gtest]
    fn the_cost_is_the_last_checkpoints_nano_aiu_at_a_hundred_credits_to_the_dollar() {
        let mut stream = Stream::default();
        stream.take(br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":273068000}}"#);

        let report = stream.report(Exit::Code(0), "");

        expect_that!(report.cost_usd, some(approx_eq(0.002_730_68)));
        expect_that!(report.failure, none());
        expect_that!(report.last_message, none());
        expect_that!(report.limit, none());
    }

    #[gtest]
    fn the_last_of_two_checkpoints_wins() {
        let mut stream = Stream::default();
        stream.take(br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":100000000}}"#);
        stream.take(br#"{"type":"assistant.message_delta","data":{"deltaContent":"hi"}}"#);
        stream.take(b"not json at all");
        stream.take(br#"{"type":"session.usage_checkpoint","data":{"totalNanoAiu":300000000}}"#);

        expect_that!(stream.report(Exit::Code(0), "").cost_usd, some(approx_eq(0.003)));
    }

    #[gtest]
    fn no_checkpoint_is_no_cost() {
        let mut stream = Stream::default();
        stream.take(br#"{"type":"result","exitCode":0}"#);

        expect_that!(stream.report(Exit::Code(0), "").cost_usd, none());
    }

    #[gtest]
    fn a_failed_exit_is_named_by_its_code_and_the_last_line_of_stderr() {
        expect_that!(
            read_report(Exit::Code(2), None, "warming up\n\nboom: bad flag  \n\n").failure,
            some(eq("copilot exited with code 2: boom: bad flag"))
        );
        expect_that!(
            read_report(Exit::Code(3), None, " \n").failure,
            some(eq("copilot exited with code 3."))
        );
        expect_that!(
            read_report(Exit::Signal, None, "").failure,
            some(eq("copilot was stopped by a signal."))
        );
    }
}
