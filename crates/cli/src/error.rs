//! Every way a command fails, and the one place that turns a failure into a message and an exit code.
//!
//! | Exit | Meaning |
//! | :---: | --- |
//! | 0 | Done, or nothing to do |
//! | 1 | Unexpected: a bug in the CLI or a `500` from the server |
//! | 2 | The command line was wrong, or doesn't say what the command needs |
//! | 3 | The request was refused: bad input or a rule, or the CLI's own lookups |
//! | 4 | Not signed in, not allowed, or not found |
//! | 5 | The server couldn't be reached or is unavailable |
//! | 6 | The local setup is broken: unreadable config, no keychain, no agent tool |

use std::error::Error as _;

use crate::agent::ToolError;
use crate::auth::login::LoginError;
use crate::auth::loopback::LoopbackError;
use crate::auth::{AuthError, TokenStoreError};
use crate::client::ClientError;
use crate::commands::UsageError;
use crate::config::{ConfigError, SettingError};
use crate::lookup::LookupError;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Setting(#[from] SettingError),
    #[error(transparent)]
    Usage(#[from] UsageError),
    #[error(transparent)]
    Lookup(#[from] LookupError),
    /// The server doesn't have the item a command named. Kept apart from other `404`s to give the hint that fits.
    #[error("no item `{name}` in {heap}")]
    NoSuchItem {
        name: String,
        heap: String,
        #[source]
        source: ClientError,
    },
    /// The item a command named has no step by the number it gave.
    #[error("no step `{name}` in {heap}")]
    NoSuchStep { name: String, heap: String },
    /// The item a command named has no question by the number it gave.
    #[error("no question `{name}` in {heap}")]
    NoSuchQuestion { name: String, heap: String },
    /// A question that, by the item just read, takes no reply from this command: it was withdrawn, or it is answered
    /// already with another reply.
    #[error("{}", .refusal.message(.name))]
    QuestionRefused { name: String, refusal: QuestionRefusal },
    /// The server refused a change to an item because it moved on or is closed. Carries the item as `team/heap/key`,
    /// for a hint that can be pasted whatever heap is the default.
    #[error("{source}")]
    ItemRefused {
        name: String,
        #[source]
        source: ClientError,
    },
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Client(#[from] ClientError),
    /// The agent tool `work run` starts isn't there, or wouldn't start.
    #[error(transparent)]
    Tool(#[from] ToolError),
    /// The file `work run --instructions` names can't be read.
    #[error("couldn't read the instructions at {}", .path.display())]
    NoInstructions {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error(transparent)]
    TokenStore(#[from] TokenStoreError),
    #[error("can't write the output")]
    Output(#[from] std::io::Error),
    /// A `/work` route answered `404`: an older server without the `/work` routes.
    #[error("This server doesn't run agents.")]
    NoAgents,
    /// A route of the run `UPHEAP_RUN` names answered `404`: the server doesn't know the run (it is another user's, or
    /// from another server), or the server lacks the `/work` routes.
    #[error("no run {run} on this server, or it doesn't run agents")]
    NoSuchRun { run: String },
}

/// Why a question takes no reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionRefusal {
    /// It was taken back unanswered: its item closed, or its agent's turn was taken back. Nothing brings it back.
    Withdrawn,
    /// It has another reply, and a reply can't be changed here.
    AnsweredDifferently,
}

impl QuestionRefusal {
    fn message(self, name: &str) -> String {
        match self {
            Self::Withdrawn => {
                format!("{name} was withdrawn, so there is nothing to reply to")
            }
            Self::AnsweredDifferently => {
                format!("{name} is answered already with another reply, which can't be changed here")
            }
        }
    }
}

impl CliError {
    /// The exit code for the failure's category.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::TokenStore(_) | Self::Tool(_) | Self::NoInstructions { .. } => 6,
            Self::Setting(_) | Self::Usage(_) => 2,
            Self::NoSuchStep { .. } | Self::NoSuchQuestion { .. } | Self::NoAgents | Self::NoSuchRun { .. } => 4,
            Self::Lookup(
                LookupError::MalformedHeap { .. }
                | LookupError::MalformedItem { .. }
                | LookupError::MalformedStep { .. }
                | LookupError::MalformedQuestion { .. },
            ) => 2,
            Self::Lookup(
                LookupError::UnknownHeap { .. }
                | LookupError::UnknownDueChoice { .. }
                | LookupError::AmbiguousHeap { .. }
                | LookupError::NoPersonalTeam
                | LookupError::UnknownTag { .. }
                | LookupError::UnknownView { .. }
                | LookupError::AmbiguousView { .. }
                | LookupError::UnknownParameter { .. }
                | LookupError::UnknownOption { .. }
                | LookupError::UnknownChoice { .. }
                | LookupError::NoPickGiven { .. }
                | LookupError::NoChoicesToPick { .. },
            ) => 3,
            Self::QuestionRefused { .. } => 3,
            Self::Auth(AuthError::NotSignedIn { .. }) => 4,
            Self::Auth(AuthError::Store(_) | AuthError::Lock { .. } | AuthError::RefreshNotKept(_)) => 6,
            Self::Auth(AuthError::Client(err) | AuthError::Revoke(err))
            | Self::Client(err)
            | Self::NoSuchItem { source: err, .. }
            | Self::ItemRefused { source: err, .. } => client_exit_code(err),
            Self::Login(LoginError::Loopback(LoopbackError::Bind(_))) => 6,
            Self::Login(LoginError::Loopback(LoopbackError::TimedOut(_)) | LoginError::Cancelled) => 4,
            Self::Login(LoginError::Refused { .. }) => 4,
            Self::Login(LoginError::Random(_) | LoginError::NoCode) => 1,
            Self::Output(_) => 1,
        }
    }

    /// What to tell the user: the message, the error's causes, and a hint at the next step when there's an obvious one.
    pub fn to_message(&self) -> Vec<String> {
        self.to_message_in(false)
    }

    /// [`to_message`](Self::to_message) for a command run inside a run (`UPHEAP_RUN` is set), whose hints are about
    /// what an agent does next.
    pub fn to_message_in(&self, in_run: bool) -> Vec<String> {
        let mut lines = vec![format!("error: {}", self.with_causes())];
        if let Some(problem) = self.problem()
            && problem.status >= 500
            && let Some(request_id) = &problem.request_id
        {
            lines.push(format!("request ID: {request_id}"));
        }
        if let Some(hint) = self.hint(in_run) {
            lines.push(format!("hint: {hint}"));
        }
        lines
    }

    /// The problem body the server answered with, for `--json` to print as it came.
    pub fn problem_body(&self) -> Option<&str> {
        match self {
            Self::Client(ClientError::Problem { body, .. })
            | Self::NoSuchItem {
                source: ClientError::Problem { body, .. },
                ..
            }
            | Self::ItemRefused {
                source: ClientError::Problem { body, .. },
                ..
            }
            | Self::Auth(
                AuthError::Client(ClientError::Problem { body, .. })
                | AuthError::Revoke(ClientError::Problem { body, .. }),
            ) => Some(body),
            _ => None,
        }
    }

    /// The HTTP status the server refused with, when the failure is a refusal.
    pub(crate) fn problem_status(&self) -> Option<u16> {
        self.problem().map(|problem| problem.status)
    }

    fn problem(&self) -> Option<&api_types::problem::Problem> {
        match self {
            Self::Client(ClientError::Problem { problem, .. })
            | Self::NoSuchItem {
                source: ClientError::Problem { problem, .. },
                ..
            }
            | Self::ItemRefused {
                source: ClientError::Problem { problem, .. },
                ..
            }
            | Self::Auth(
                AuthError::Client(ClientError::Problem { problem, .. })
                | AuthError::Revoke(ClientError::Problem { problem, .. }),
            ) => Some(problem),
            _ => None,
        }
    }

    fn hint(&self, in_run: bool) -> Option<String> {
        let status = self.problem().map(|problem| problem.status);
        if let Some(hint) = self.problem().and_then(run_hint) {
            return Some(hint);
        }
        match (self, status) {
            (
                Self::TokenStore(TokenStoreError::Corrupt { .. })
                | Self::Auth(AuthError::Store(TokenStoreError::Corrupt { .. })),
                _,
            ) => Some(
                "sign in again with `upheap auth login`, or forget it with `upheap auth logout --local`".to_owned(),
            ),
            (Self::Tool(ToolError::NotInstalled { tool }), _) => Some(format!(
                "install Claude Code, or check that `{tool} --version` works in this shell"
            )),
            (Self::Tool(ToolError::NoDirectory { .. }), _) => {
                Some("check that `--root` names a directory you can write to".to_owned())
            }
            (Self::NoInstructions { .. }, _) => {
                Some("check that `--instructions` names a text file you can read".to_owned())
            }
            (Self::Auth(AuthError::RefreshNotKept(_)), _) => Some("sign in again with `upheap auth login`".to_owned()),
            (Self::Config(ConfigError::Parse { .. }), _) => {
                Some("if a newer `upheap` wrote it, upgrade this one; otherwise fix or remove the file".to_owned())
            }
            (Self::Auth(AuthError::Revoke(_)), _) => {
                Some("to forget the sign-in on this machine only, run `upheap auth logout --local`".to_owned())
            }
            (Self::Auth(AuthError::NotSignedIn { .. }), _) | (_, Some(401)) => {
                Some("sign in with `upheap auth login`".to_owned())
            }
            (
                Self::Lookup(LookupError::UnknownTag {
                    name,
                    heap,
                    offer_add: true,
                    ..
                }),
                _,
            ) => Some(format!("add it with `{}`", tag_add_command(name, heap))),
            (Self::NoSuchItem { .. }, _) => Some("`upheap item list` lists the heap's items".to_owned()),
            (Self::NoSuchStep { name, .. }, _) => Some(format!(
                "`upheap item show {}` lists the item's steps",
                name.rsplit_once('.').map_or(name.as_str(), |(item, _)| item)
            )),
            (Self::NoSuchQuestion { name, .. }, _) => Some(format!(
                "`upheap item show {}` lists its questions",
                name.rsplit_once(".q").map_or(name.as_str(), |(item, _)| item)
            )),
            (_, Some(404)) => Some("`upheap heap list` lists the heaps you can use".to_owned()),
            (Self::ItemRefused { name, .. }, _) => match self.problem().map(|problem| problem.code.as_str()) {
                Some("version_conflict") if in_run => Some(format!("Read it again: upheap item show {name}")),
                Some("version_conflict") => Some(format!(
                    "someone else changed it since it was read; look with `upheap item show {name}` and try again"
                )),
                Some("item_closed") => Some(format!("reopen it first with `upheap item reopen {name}`")),
                _ => None,
            },
            _ => None,
        }
    }

    /// The message followed by each cause the message doesn't already say, as `a: b: c`.
    fn with_causes(&self) -> String {
        let mut text = self.to_string();
        let mut cause = self.source();
        while let Some(err) = cause {
            let said = err.to_string();
            if !text.contains(&said) {
                text = format!("{text}: {said}");
            }
            cause = err.source();
        }
        text
    }
}

/// What a run is told when the server refuses a write it made (design: What a run is refused), or `None` for any other
/// refusal.
fn run_hint(problem: &api_types::problem::Problem) -> Option<String> {
    match problem.code.as_str() {
        "run_over" => Some("This run is over. Stop.".to_owned()),
        "run_wrong_item" => problem.item_key.as_ref().map(|key| {
            format!("This run holds {key}. Besides adding items, and steps to the items it added, write to {key} only.")
        }),
        "run_forbidden" => Some("Only the person does that.".to_owned()),
        "run_add_reminder" => Some("Add it as a task or a note; leave the date in its context.".to_owned()),
        "run_add_needs_context" => {
            Some("Add it again with --context-file - and say what it is and why it came up.".to_owned())
        }
        "run_done_needs_review" => {
            Some("Propose closing: upheap question propose-done <key> \"<what was done>\"".to_owned())
        }
        "not_ready_to_close" => Some("Finish what it names, then propose again.".to_owned()),
        "review_open" => Some("Wait for the person's reply to that review.".to_owned()),
        "question_needs_task" | "question_needs_do_turn" => {
            Some("Only an agent doing a task asks for review or proposes closing.".to_owned())
        }
        "not_reached" => Some("Fix what's missing, then run the same command again.".to_owned()),
        "run_need_above_origin" => Some(match &problem.highest {
            Some(highest) => format!("Give --need {highest} or lower."),
            None => "Give --need at or below the note's.".to_owned(),
        }),
        "run_blocker_not_made" => Some("Block only on a task you made from this item in this turn.".to_owned()),
        "nothing_made" => Some(
            "Make the tasks first: upheap item add \"<title>\" --task --from <key> --need <need> --age keep|new"
                .to_owned(),
        ),
        "became_needs_note" => Some("Only a note becomes other items; a task or reminder stays one.".to_owned()),
        "origin_not_a_note" => Some("Only a note can be split; make items from a note.".to_owned()),
        "origin_closed" => Some("Reopen the note first, or make the items from another one.".to_owned()),
        "became_needs_split" => Some("Make items from the note, then close it with `upheap item became`.".to_owned()),
        _ => None,
    }
}

/// The command that adds the tag `name` to `heap`, safe to paste: the name is quoted for the shell, and comes after
/// `--` when it starts with `-`, so it isn't read as a flag.
fn tag_add_command(name: &str, heap: &str) -> String {
    let name = name.trim();
    if name.starts_with('-') {
        format!("upheap tag add --heap {heap} -- {}", shell_quoted(name))
    } else {
        format!("upheap tag add {} --heap {heap}", shell_quoted(name))
    }
}

/// `text` as one shell word: unchanged when it is only ASCII letters, digits and `-_./`, otherwise in single
/// quotes with each `'` written as `'\''`.
fn shell_quoted(text: &str) -> String {
    let is_plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if is_plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn client_exit_code(err: &ClientError) -> u8 {
    match err {
        ClientError::Setup(_) => 6,
        ClientError::Unreachable { .. } => 5,
        ClientError::Redirected { .. } => 2,
        // Refused because of the run, not the caller: a rule to read, not a missing permission.
        ClientError::Problem { problem, .. }
            if matches!(
                problem.code.as_str(),
                "run_forbidden"
                    | "run_done_needs_review"
                    | "run_wrong_item"
                    | "run_over"
                    | "run_cost_recorded"
                    | "run_add_reminder"
                    | "run_add_needs_context"
                    | "run_need_above_origin"
                    | "run_blocker_not_made"
            ) =>
        {
            3
        }
        ClientError::Problem { problem, .. } => status_exit_code(problem.status),
        ClientError::OAuthRefused { .. } => 4,
        ClientError::Unreadable { status, .. } => status_exit_code(status.as_u16()),
    }
}

/// `401`, `403` and `404` are 4, as the server doesn't tell "not allowed" from "not there"; `503`, and a proxy's `502`
/// or `504` for a server behind it that's down or slow, are 5; other refusals are 3; anything else, a `500` included,
/// is unexpected.
fn status_exit_code(status: u16) -> u8 {
    match status {
        401 | 403 | 404 => 4,
        502..=504 => 5,
        400..=499 => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn problem(status: u16, code: &str) -> ClientError {
        ClientError::Problem {
            problem: Box::new(api_types::problem::Problem {
                code: code.to_owned(),
                title: "Refused".to_owned(),
                status,
                field: None,
                hint: None,
                item_key: None,
                highest: None,
                request_id: None,
            }),
            body: String::new(),
        }
    }

    fn refused(status: u16, code: &str) -> CliError {
        CliError::ItemRefused {
            name: "k3x9a".to_owned(),
            source: problem(status, code),
        }
    }

    #[gtest]
    fn a_stale_edit_and_an_edit_of_a_closed_item_say_what_to_do_next() {
        let stale = refused(409, "version_conflict");
        let closed = refused(409, "item_closed");
        let other = refused(409, "state_not_allowed");

        expect_that!(stale.exit_code(), eq(3));
        expect_that!(
            stale.to_message(),
            contains(eq(
                "hint: someone else changed it since it was read; look with `upheap item show k3x9a` and try again"
            ))
        );
        expect_that!(other.to_message().len(), eq(1));
        expect_that!(closed.exit_code(), eq(3));
        expect_that!(
            closed.to_message(),
            contains(eq("hint: reopen it first with `upheap item reopen k3x9a`"))
        );
    }

    #[gtest]
    fn a_stale_edit_in_a_run_says_to_read_the_item_again() {
        let stale = refused(409, "version_conflict");

        expect_that!(
            stale.to_message_in(true),
            contains(eq("hint: Read it again: upheap item show k3x9a"))
        );
    }

    #[gtest]
    fn what_a_run_is_refused_is_exit_3_whatever_the_status_with_a_hint() {
        let run_refusal = |status, code: &str, title: &str| {
            let mut err = problem(status, code);
            if let ClientError::Problem { problem, .. } = &mut err {
                problem.title = title.to_owned();
                if code == "run_wrong_item" {
                    problem.item_key = Some("k3x9a".to_owned());
                }
            }
            CliError::Client(err)
        };
        let over = run_refusal(409, "run_over", "This run is over.");
        let wrong = run_refusal(403, "run_wrong_item", "This run is for another item.");
        let forbidden = run_refusal(403, "run_forbidden", "Only the person does that.");
        let done = run_refusal(
            403,
            "run_done_needs_review",
            "An agent closes a task as done only after a review.",
        );
        let recorded = run_refusal(409, "run_cost_recorded", "The run's cost is already recorded.");
        let missed = run_refusal(422, "not_reached", "The task has no steps.");
        let reminder = run_refusal(403, "run_add_reminder", "A run adds tasks and notes, not reminders.");
        let no_context = run_refusal(403, "run_add_needs_context", "What a run adds needs a context.");
        let too_high = {
            let mut err = problem(403, "run_need_above_origin");
            if let ClientError::Problem { problem, .. } = &mut err {
                problem.highest = Some("expected".to_owned());
            }
            CliError::Client(err)
        };
        let not_made = run_refusal(403, "run_blocker_not_made", "Blocks only among the items it made.");

        for err in [
            &over,
            &wrong,
            &forbidden,
            &done,
            &recorded,
            &missed,
            &reminder,
            &no_context,
            &too_high,
            &not_made,
        ] {
            expect_that!(err.exit_code(), eq(3));
        }
        expect_that!(
            done.to_message(),
            contains(eq(
                "hint: Propose closing: upheap question propose-done <key> \"<what was done>\""
            ))
        );
        // Their messages say it all: a hint would only contradict them.
        expect_that!(recorded.to_message(), not(contains(starts_with("hint:"))));
        expect_that!(over.to_message(), contains(eq("hint: This run is over. Stop.")));
        expect_that!(
            wrong.to_message(),
            contains(eq(
                "hint: This run holds k3x9a. Besides adding items, and steps to the items it added, write to k3x9a only."
            ))
        );
        expect_that!(forbidden.to_message(), contains(eq("hint: Only the person does that.")));
        expect_that!(
            reminder.to_message(),
            contains(eq("hint: Add it as a task or a note; leave the date in its context."))
        );
        expect_that!(
            no_context.to_message(),
            contains(eq(
                "hint: Add it again with --context-file - and say what it is and why it came up."
            ))
        );
        expect_that!(
            too_high.to_message(),
            contains(eq("hint: Give --need expected or lower."))
        );
        expect_that!(
            not_made.to_message(),
            contains(eq("hint: Block only on a task you made from this item in this turn."))
        );
        expect_that!(
            missed.to_message(),
            contains(eq("hint: Fix what's missing, then run the same command again."))
        );
    }

    #[gtest]
    fn a_server_without_agents_is_exit_4() {
        expect_that!(CliError::NoAgents.exit_code(), eq(4));
        expect_that!(
            CliError::NoAgents.to_message(),
            elements_are![eq("error: This server doesn't run agents.")]
        );
    }

    #[gtest]
    fn an_item_the_server_lacks_points_to_the_list_of_items() {
        let missing = CliError::NoSuchItem {
            name: "k3x9a".to_owned(),
            heap: "ada/personal".to_owned(),
            source: problem(404, "not_found"),
        };

        expect_that!(missing.exit_code(), eq(4));
        expect_that!(
            missing.to_message(),
            contains(eq("hint: `upheap item list` lists the heap's items"))
        );
    }

    #[gtest]
    fn names_are_quoted_for_the_shell_only_when_they_need_it() {
        expect_that!(shell_quoted("infra-2_a.b/c"), eq("infra-2_a.b/c"));
        expect_that!(shell_quoted("Big rock"), eq("'Big rock'"));
        expect_that!(shell_quoted("it's"), eq("'it'\\''s'"));
        expect_that!(shell_quoted(""), eq("''"));
    }

    #[gtest]
    fn the_tag_add_hint_puts_a_name_that_looks_like_a_flag_after_two_dashes() {
        expect_that!(
            tag_add_command(" ops ", "ada/personal"),
            eq("upheap tag add ops --heap ada/personal")
        );
        expect_that!(
            tag_add_command("-urgent", "ada/personal"),
            eq("upheap tag add --heap ada/personal -- -urgent")
        );
    }

    #[gtest]
    fn statuses_map_to_their_category() {
        expect_that!(status_exit_code(400), eq(3));
        expect_that!(status_exit_code(409), eq(3));
        expect_that!(status_exit_code(422), eq(3));
        expect_that!(status_exit_code(401), eq(4));
        expect_that!(status_exit_code(403), eq(4));
        expect_that!(status_exit_code(404), eq(4));
        expect_that!(status_exit_code(502), eq(5));
        expect_that!(status_exit_code(503), eq(5));
        expect_that!(status_exit_code(504), eq(5));
        expect_that!(status_exit_code(500), eq(1));
    }
}
