//! Claude's command-line tool as an agent: the command line for each kind of access, and what its stream says. The
//! permission flags are the ones in the README's agent paragraph, kept in `arguments`. Starting the process, and
//! watching and stopping it, are in [`super::process`].

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::process::{self, Exit, MAX_COST_USD, Output, StopTracker, last_line};
use super::{Access, ToolError, ToolReport, ToolStart, UsageLimit};
use crate::output::shorten;

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
/// given. [`DENY`] comes last, in every run: a rule of the person's (or their own settings) may allow `Bash`
/// wholesale, and Claude's deny wins over any allow.
///
/// A `model` goes in as `--model <model>`, as given, right before `--permission-mode`: after `--allowedTools` it
/// would be taken for one more of that flag's values.
pub(super) fn arguments<'a>(
    access: Access,
    own_directory: bool,
    allow: &'a [String],
    model: Option<&'a str>,
) -> Vec<&'a str> {
    let mut arguments = vec![
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
    ];
    if let Some(model) = model {
        arguments.extend(["--model", model]);
    }
    arguments.push("--permission-mode");
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
    arguments.push("--disallowedTools");
    arguments.extend(DENY);
    arguments
}

/// What every run is refused, as deny rules: `upheap auth` and `upheap heap use`. Both write on the person's own
/// machine (their sign-in, their default heap and server), which the server has no say in and nothing tells the
/// person about, so text an agent reads must not be able to talk it into them.
const DENY: [&str; 2] = ["Bash(upheap auth:*)", "Bash(upheap heap use:*)"];

/// What a run may do in a directory made for it, as allow rules: nearly everything. A run there has to clone and
/// build what it works on, and which commands that takes isn't known yet. `upheap auth` and `upheap heap use` stay
/// refused by the [`DENY`] every run gets, for the reason [`UPHEAP_COMMANDS`] leaves them out.
const IN_OWN_DIRECTORY: [&str; 6] = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];

/// The `upheap` commands a run needs, as allow rules: reading, writing its item, asking, and saying reached,
/// handing the turn back or watching its pull request. Not all of `upheap`: text an agent reads could talk it into `upheap auth logout` or
/// `upheap heap use`, which the server has no say in.
const UPHEAP_COMMANDS: [&str; 12] = [
    "Bash(upheap item:*)",
    "Bash(upheap step:*)",
    "Bash(upheap question:*)",
    "Bash(upheap comment:*)",
    "Bash(upheap work reached:*)",
    "Bash(upheap work hand-back:*)",
    "Bash(upheap work watch:*)",
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
///
/// A failed run hit the usage limit when `rejected_line`, the stream's last `rate_limit_event` that was rejected and
/// not an overage, is there, or the result says `api_error` is `usage_limit_reached`. The reset time and the window
/// come from the rejected event only; the result's text is not read for them.
pub(super) fn read_report(
    exit: Exit,
    result_line: Option<&str>,
    rejected_line: Option<&str>,
    stderr: &str,
) -> ToolReport {
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
    let limit = failure.is_some().then(|| {
        let rejected: Option<Value> = rejected_line.and_then(|line| serde_json::from_str(line.trim()).ok());
        let info = rejected.as_ref().and_then(|event| event.get("rate_limit_info"));
        (rejected.is_some() || field("api_error").and_then(Value::as_str) == Some("usage_limit_reached")).then(|| {
            UsageLimit {
                resets_at: info
                    .and_then(|info| info.get("resetsAt"))
                    .and_then(Value::as_i64)
                    .and_then(|seconds| jiff::Timestamp::from_second(seconds).ok()),
                window: info
                    .and_then(|info| info.get("rateLimitType"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            }
        })
    });
    ToolReport {
        cost_usd,
        failure,
        last_message,
        limit: limit.flatten(),
        model: None,
    }
}

/// What Claude's stdout has said so far.
#[derive(Default)]
struct Stream {
    /// The last line that was a `result` event.
    result_line: Option<Vec<u8>>,
    /// The last line that was a `rate_limit_event` the account was rejected by, not counting one for overage use.
    rejected_line: Option<Vec<u8>>,
    /// The model of the first `system` event with subtype `init`: the one the session's main turns run on.
    model: Option<String>,
}

impl Output for Stream {
    /// Remembers `line` when it is the `result` event, or a rejected `rate_limit_event`, or the first `init` event's
    /// model.
    fn take(&mut self, line: &[u8]) {
        if self.model.is_none() {
            self.model = init_model(line);
        }
        if is_result_line(line) {
            self.result_line = Some(line.to_vec());
        } else if is_rejected_line(line) {
            self.rejected_line = Some(line.to_vec());
        }
    }

    fn has_result(&self) -> bool {
        self.result_line.is_some()
    }

    fn report(&self, exit: Exit, stderr: &str) -> ToolReport {
        let result_line = self.result_line.as_deref().map(String::from_utf8_lossy);
        let rejected_line = self.rejected_line.as_deref().map(String::from_utf8_lossy);
        ToolReport {
            model: self.model.clone(),
            ..read_report(exit, result_line.as_deref(), rejected_line.as_deref(), stderr)
        }
    }
}

/// Starts `program` as Claude with `start`'s access; the report is read from the stream's `result` event. How the
/// process is run is in [`process::run`].
pub(super) async fn run(
    program: PathBuf,
    start: ToolStart,
    stops: StopTracker,
    silence_limit: Duration,
) -> Result<ToolReport, ToolError> {
    let arguments = arguments(start.access, start.dir.is_some(), &start.allow, start.model.as_deref())
        .into_iter()
        .map(str::to_owned)
        .collect();
    process::run(
        program,
        arguments,
        "CLAUDE.md",
        Stream::default(),
        start,
        stops,
        silence_limit,
    )
    .await
}

/// Whether `line` is a JSON object whose `type` is `result`: the event that ends a stream. Anything else, a line that
/// is not JSON or not UTF-8 included, is not.
fn is_result_line(line: &[u8]) -> bool {
    serde_json::from_slice::<Value>(line)
        .is_ok_and(|event| event.is_object() && event.get("type").and_then(Value::as_str) == Some("result"))
}

/// The `model` of `line` when it is the `system` event with subtype `init`, which names the model the session runs
/// on.
fn init_model(line: &[u8]) -> Option<String> {
    // Most lines are messages and tool output, some of them large: only a line that holds `"init"` is parsed.
    const SUBTYPE: &[u8] = b"\"init\"";
    if !line.windows(SUBTYPE.len()).any(|window| window == SUBTYPE) {
        return None;
    }
    let event: Value = serde_json::from_slice(line).ok()?;
    if event.get("type").and_then(Value::as_str) != Some("system")
        || event.get("subtype").and_then(Value::as_str) != Some("init")
    {
        return None;
    }
    event.get("model").and_then(Value::as_str).map(str::to_owned)
}

/// Whether `line` is a JSON object whose `type` is `rate_limit_event` with status `rejected` and `isUsingOverage` not
/// true: the account's usage limit was reached.
fn is_rejected_line(line: &[u8]) -> bool {
    // Most lines are messages and tool output, some of them large: only a line that names the event is parsed.
    const EVENT: &[u8] = b"rate_limit_event";
    if !line.windows(EVENT.len()).any(|window| window == EVENT) {
        return false;
    }
    let Ok(event) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    event.get("type").and_then(Value::as_str) == Some("rate_limit_event")
        && event.get("rate_limit_info").is_some_and(|info| {
            info.get("status").and_then(Value::as_str) == Some("rejected")
                && info.get("isUsingOverage").and_then(Value::as_bool) != Some(true)
        })
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_arguments_for_preparing_are_exactly_these() {
        expect_that!(
            arguments(Access::Prepare, false, &[], None),
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
                "Bash(upheap work watch:*)",
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
                "Bash(upheap heap use:*)",
            ])
        );
    }

    #[gtest]
    fn the_arguments_for_doing_are_exactly_these() {
        expect_that!(
            arguments(Access::Do, false, &[], None),
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
                "Bash(upheap work watch:*)",
                "Bash(upheap guide:*)",
                "Bash(upheap heap show:*)",
                "Bash(upheap heap list:*)",
                "Bash(upheap tag list:*)",
                "Bash(upheap parameter list:*)",
                "--disallowedTools",
                "Bash(upheap auth:*)",
                "Bash(upheap heap use:*)",
            ])
        );
    }

    #[gtest]
    fn a_model_goes_in_as_two_arguments_before_the_permission_mode() {
        for access in [Access::Prepare, Access::Do] {
            let mut expected = arguments(access, false, &[], None);
            let at = expected
                .iter()
                .position(|argument| *argument == "--permission-mode")
                .expect("flag");
            expected.splice(at..at, ["--model", "opus"]);

            expect_that!(arguments(access, false, &[], Some("opus")), eq(&expected));
        }
    }

    const DENIES: [&str; 3] = ["--disallowedTools", "Bash(upheap auth:*)", "Bash(upheap heap use:*)"];

    /// The arguments for `access` outside its own directory and with no rules of the person's, up to the denies.
    fn narrow(access: Access) -> Vec<&'static str> {
        let mut narrow = arguments(access, false, &[], None);
        narrow.truncate(narrow.len() - DENIES.len());
        narrow
    }

    #[gtest]
    fn in_its_own_directory_either_access_may_do_anything_but_upheap_auth_and_heap_use() {
        let wide = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &wide, &DENIES].concat();

            // The wide rules follow the narrow ones, so they are still values of `--allowedTools`.
            expect_that!(arguments(access, true, &[], None), eq(&expected));
        }
    }

    #[gtest]
    fn extra_allow_rules_follow_the_fixed_ones_and_the_denies_still_come_after_them() {
        let extra = ["mcp__x".to_owned(), "Bash".to_owned()];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &["mcp__x", "Bash"], &DENIES].concat();

            expect_that!(arguments(access, false, &extra, None), eq(&expected));
        }
    }

    #[gtest]
    fn in_its_own_directory_extra_rules_come_after_the_wide_ones_and_before_the_denies() {
        let extra = ["mcp__x".to_owned(), "Bash(npm:*)".to_owned()];
        let wide = ["Bash", "Read", "Edit", "Write", "WebFetch", "WebSearch"];
        for access in [Access::Prepare, Access::Do] {
            let narrow = narrow(access);
            let expected = [&narrow[..], &wide, &["mcp__x", "Bash(npm:*)"], &DENIES].concat();

            expect_that!(arguments(access, true, &extra, None), eq(&expected));
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
            let report = read_report(exit, Some(stdout).filter(|line| !line.is_empty()), None, stderr);

            expect_that!(report.cost_usd, eq(cost), "{name}: cost");
            expect_that!(report.failure.as_deref(), eq(failure), "{name}: failure");
        }
    }

    const WARNING: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1794687600,"rateLimitType":"five_hour","utilization":0.99,"overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false,"surpassedThreshold":0.9},"uuid":"9030abd5-db6f-4797-b965-c62f6dfbc274","session_id":"ee45"}"#;
    const REJECTED: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1794687600,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false},"uuid":"5e7a81a9-7f74-43fc-b664-1f174490a445","session_id":"ee45"}"#;
    const LIMIT_RESULT: &str = r#"{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","api_error_status":429,"api_error_type":"rate_limit_error","api_error":"usage_limit_reached","result":"You've hit your 5-hour limit · resets 12:20pm (America/Los_Angeles)","total_cost_usd":3.4656993,"num_turns":48,"session_id":"ee45"}"#;

    #[gtest]
    fn a_failed_run_with_a_rejected_event_hit_the_limit_with_its_reset_time_and_window() {
        let report = read_report(Exit::Code(1), Some(LIMIT_RESULT), Some(REJECTED), "");

        expect_that!(
            report.limit,
            some(eq(&UsageLimit {
                resets_at: jiff::Timestamp::from_second(1_794_687_600).ok(),
                window: Some("five_hour".to_owned()),
            }))
        );
    }

    #[gtest]
    fn the_stream_keeps_the_rejected_event_and_not_a_warning_or_an_overage() {
        let overage = REJECTED.replace(r#""isUsingOverage":false"#, r#""isUsingOverage":true"#);

        expect_that!(is_rejected_line(REJECTED.as_bytes()), eq(true));
        expect_that!(is_rejected_line(WARNING.as_bytes()), eq(false));
        expect_that!(is_rejected_line(overage.as_bytes()), eq(false));
        expect_that!(is_rejected_line(LIMIT_RESULT.as_bytes()), eq(false));
        expect_that!(is_rejected_line(b"not json"), eq(false));
    }

    #[gtest]
    fn the_api_error_alone_is_a_limit_without_a_reset_time() {
        let report = read_report(Exit::Code(1), Some(LIMIT_RESULT), None, "");

        expect_that!(
            report.limit,
            some(eq(&UsageLimit {
                resets_at: None,
                window: None,
            }))
        );
    }

    #[gtest]
    fn a_rejected_event_in_a_run_that_did_not_fail_is_no_limit() {
        let report = read_report(Exit::Code(0), Some(r#"{"is_error":false}"#), Some(REJECTED), "");

        expect_that!(report.limit, none());
    }

    #[gtest]
    fn a_failure_with_neither_sign_is_no_limit() {
        let report = read_report(
            Exit::Code(1),
            Some(r#"{"is_error":true,"subtype":"error_max_turns"}"#),
            None,
            "",
        );

        expect_that!(report.limit, none());
    }

    #[gtest]
    fn the_last_message_is_the_result_when_there_is_one() {
        let report = read_report(
            Exit::Code(0),
            Some(r#"{"is_error":false,"result":"All done"}"#),
            None,
            "",
        );

        expect_that!(report.last_message, some(eq("All done")));
        expect_that!(read_report(Exit::Code(0), Some("{}"), None, "").last_message, none());
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
    fn a_stream_keeps_the_last_result_event_and_ignores_the_rest() {
        // Arrange
        let mut stream = Stream::default();
        let lines: [&[u8]; 5] = [
            br#"{"type":"system","subtype":"init"}"#,
            br#"{"type":"result","result":"first"}"#,
            b"warming up",
            br#"{"type":"assistant"}"#,
            br#"{"type":"result","result":"last"}"#,
        ];

        // Act
        for line in lines {
            stream.take(line);
        }

        // Assert
        expect_that!(
            stream.result_line.as_deref(),
            some(eq(&br#"{"type":"result","result":"last"}"#[..]))
        );
    }

    #[gtest]
    fn a_stream_with_no_result_event_has_no_result_line() {
        let mut stream = Stream::default();

        stream.take(br#"{"type":"assistant"}"#);

        expect_that!(stream.result_line, none());
    }

    #[gtest]
    fn a_stream_reports_the_model_of_its_init_event() {
        let mut stream = Stream::default();
        stream.take(br#"{"type":"system","subtype":"init","model":"claude-opus-5-5","tools":[]}"#);
        stream.take(br#"{"type":"assistant","message":{"text":"init"}}"#);

        expect_that!(stream.report(Exit::Code(0), "").model, some(eq("claude-opus-5-5")));
    }

    #[gtest]
    fn a_stream_without_an_init_event_reports_no_model() {
        let mut stream = Stream::default();
        stream.take(br#"{"type":"assistant","message":{"text":"init"}}"#);
        stream.take(br#"{"type":"system","subtype":"init"}"#);

        expect_that!(stream.report(Exit::Code(0), "").model, none());
    }
}
