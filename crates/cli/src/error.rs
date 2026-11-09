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
//! | 6 | The local setup is broken: unreadable config, no keychain |

use std::error::Error as _;

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
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error(transparent)]
    TokenStore(#[from] TokenStoreError),
    #[error("can't write the output")]
    Output(#[from] std::io::Error),
}

impl CliError {
    /// The exit code for the failure's category.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::TokenStore(_) => 6,
            Self::Setting(_) | Self::Usage(_) => 2,
            Self::Lookup(LookupError::MalformedHeap { .. } | LookupError::MalformedItem { .. }) => 2,
            Self::Lookup(
                LookupError::UnknownHeap { .. }
                | LookupError::UnknownDueChoice { .. }
                | LookupError::AmbiguousHeap { .. }
                | LookupError::UnknownTag { .. }
                | LookupError::UnknownView { .. }
                | LookupError::AmbiguousView { .. }
                | LookupError::UnknownParameter { .. },
            ) => 3,
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
        let mut lines = vec![format!("error: {}", self.with_causes())];
        if let Some(problem) = self.problem()
            && problem.status >= 500
            && let Some(request_id) = &problem.request_id
        {
            lines.push(format!("request ID: {request_id}"));
        }
        if let Some(hint) = self.hint() {
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

    fn hint(&self) -> Option<String> {
        let status = self.problem().map(|problem| problem.status);
        match (self, status) {
            (
                Self::TokenStore(TokenStoreError::Corrupt { .. })
                | Self::Auth(AuthError::Store(TokenStoreError::Corrupt { .. })),
                _,
            ) => Some(
                "sign in again with `upheap auth login`, or forget it with `upheap auth logout --local`".to_owned(),
            ),
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
            (Self::Lookup(LookupError::UnknownTag { name, heap, .. }), _) => Some(format!(
                "add it with `upheap tag add {} --heap {heap}`",
                shell_quoted(name)
            )),
            (Self::NoSuchItem { .. }, _) => Some("`upheap item list` lists the heap's items".to_owned()),
            (_, Some(404)) => Some("`upheap heap list` lists the heaps you can use".to_owned()),
            (Self::ItemRefused { name, .. }, _) => match self.problem().map(|problem| problem.code.as_str()) {
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
