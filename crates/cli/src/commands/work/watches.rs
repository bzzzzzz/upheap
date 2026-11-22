//! What a watched pull request got since its watch began: reads what `gh pr view --json
//! state,mergeable,comments,reviews,statusCheckRollup` printed and says what came, one markdown line each.
//!
//! Pure: no `gh`, no clock. The loop (`run.rs`) runs `gh` and sends what this says to the server.

use serde::Deserialize;

/// Why what `gh` printed couldn't be read.
#[derive(Debug, thiserror::Error)]
pub(super) enum ReadError {
    #[error("gh printed something that isn't the pull request's JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("gh printed a time that can't be read: {value:?}")]
    Time {
        value: String,
        #[source]
        source: jiff::Error,
    },
}

/// The fields of `gh pr view --json state,mergeable,comments,reviews,statusCheckRollup` the loop uses. `gh` leaves out or nulls
/// what a pull request hasn't got, so every field is optional.
#[derive(Debug, Default, Deserialize)]
struct PullRequest {
    state: Option<String>,
    /// `MERGEABLE`, `CONFLICTING`, or `UNKNOWN` while GitHub works it out after a push.
    mergeable: Option<String>,
    #[serde(default)]
    comments: Vec<Comment>,
    #[serde(default)]
    reviews: Vec<Review>,
    #[serde(default, rename = "statusCheckRollup")]
    checks: Vec<Check>,
}

#[derive(Debug, Deserialize)]
struct Author {
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Comment {
    author: Option<Author>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Review {
    author: Option<Author>,
    /// `null` while the review is pending.
    #[serde(rename = "submittedAt")]
    submitted_at: Option<String>,
    state: Option<String>,
    url: Option<String>,
}

/// A check run or a status context, told apart by `__typename`.
#[derive(Debug, Deserialize)]
struct Check {
    #[serde(rename = "__typename")]
    kind: Option<String>,
    name: Option<String>,
    conclusion: Option<String>,
    #[serde(rename = "completedAt")]
    completed_at: Option<String>,
    #[serde(rename = "detailsUrl")]
    details_url: Option<String>,
    context: Option<String>,
    state: Option<String>,
    #[serde(rename = "startedAt")]
    started_at: Option<String>,
    #[serde(rename = "targetUrl")]
    target_url: Option<String>,
}

/// A time `gh` printed, in UTC milliseconds, or `None` when it printed none.
fn millis(text: Option<&str>) -> Result<Option<i64>, ReadError> {
    text.map(|value| {
        value
            .parse::<jiff::Timestamp>()
            .map(jiff::Timestamp::as_millisecond)
            .map_err(|source| ReadError::Time {
                value: value.to_owned(),
                source,
            })
    })
    .transpose()
}

/// Whether the time `gh` printed is after `since`, in UTC milliseconds.
fn after(text: Option<&str>, since: i64) -> Result<bool, ReadError> {
    Ok(millis(text)?.is_some_and(|at| at > since))
}

fn login(author: Option<&Author>) -> &str {
    author.and_then(|author| author.login.as_deref()).unwrap_or("ghost")
}

/// What came to the pull request that `json` describes since `since` (UTC milliseconds), as markdown lines in this
/// order: merged or closed, or a merge conflict on an open one, then new comments, new reviews and failed checks.
/// Merged, closed and a conflict are states, not events: they count whenever `since` is. `None` when nothing came.
/// When the lines
/// would pass `limit` characters, the lines that fit, then `- And <n> more.`
pub(super) fn what_came(json: &str, since: i64, limit: usize) -> Result<Option<String>, ReadError> {
    let pull: PullRequest = serde_json::from_str(json)?;
    let mut lines: Vec<String> = Vec::new();
    match pull.state.as_deref() {
        Some("MERGED") => lines.push("- Merged.".to_owned()),
        Some("CLOSED") => lines.push("- Closed without merging.".to_owned()),
        // Behind the base branch isn't a conflict: it can still be merged, and every merge to the base would fire it.
        _ if pull.mergeable.as_deref() == Some("CONFLICTING") => {
            lines.push("- Merge conflicts with the base branch.".to_owned());
        }
        _ => {}
    }
    for comment in &pull.comments {
        if after(comment.created_at.as_deref(), since)? {
            lines.push(format!(
                "- A comment by @{}: {}",
                login(comment.author.as_ref()),
                comment.url.as_deref().unwrap_or_default()
            ));
        }
    }
    for review in &pull.reviews {
        let how = match review.state.as_deref() {
            Some("APPROVED") => "approved",
            Some("CHANGES_REQUESTED") => "changes requested",
            Some("COMMENTED") => "commented",
            Some("DISMISSED") => "dismissed",
            _ => continue,
        };
        if after(review.submitted_at.as_deref(), since)? {
            lines.push(format!(
                "- A review by @{} ({how}): {}",
                login(review.author.as_ref()),
                review.url.as_deref().unwrap_or_default()
            ));
        }
    }
    for check in &pull.checks {
        match check.kind.as_deref() {
            Some("CheckRun")
                if matches!(
                    check.conclusion.as_deref(),
                    Some("FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED")
                ) && after(check.completed_at.as_deref(), since)? =>
            {
                lines.push(format!(
                    "- Check {} failed: {}",
                    check.name.as_deref().unwrap_or_default(),
                    check.details_url.as_deref().unwrap_or_default()
                ));
            }
            Some("StatusContext")
                if matches!(check.state.as_deref(), Some("FAILURE" | "ERROR"))
                    && after(check.started_at.as_deref(), since)? =>
            {
                lines.push(format!(
                    "- Check {} failed: {}",
                    check.context.as_deref().unwrap_or_default(),
                    check.target_url.as_deref().unwrap_or_default()
                ));
            }
            _ => {}
        }
    }
    if lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(cut(&lines, limit)))
}

/// `lines` joined, or when that passes `limit` characters, the first lines that leave room for `- And <n> more.`
fn cut(lines: &[String], limit: usize) -> String {
    let whole = lines.join("\n");
    if whole.chars().count() <= limit {
        return whole;
    }
    let mut kept = lines.len();
    loop {
        kept -= 1;
        let mut text = lines[..kept].join("\n");
        if kept > 0 {
            text.push('\n');
        }
        text.push_str(&format!("- And {} more.", lines.len() - kept));
        if kept == 0 || text.chars().count() <= limit {
            return text;
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    /// 2026-01-10T10:00:00Z.
    const SINCE: i64 = 1_768_039_200_000;
    const LIMIT: usize = 10_000;

    fn came(json: &str) -> Option<String> {
        what_came(json, SINCE, LIMIT).expect("readable")
    }

    #[gtest]
    fn a_merged_pull_request_says_so_whenever_it_happened() {
        expect_that!(came(r#"{"state":"MERGED"}"#), some(eq("- Merged.")));
    }

    #[gtest]
    fn a_closed_pull_request_says_so() {
        expect_that!(came(r#"{"state":"CLOSED"}"#), some(eq("- Closed without merging.")));
    }

    #[gtest]
    fn a_conflict_says_so_whenever_it_began() {
        expect_that!(
            came(r#"{"state":"OPEN","mergeable":"CONFLICTING"}"#),
            some(eq("- Merge conflicts with the base branch."))
        );
    }

    #[gtest]
    fn a_conflict_comes_before_what_else_came() {
        let json = r#"{"state":"OPEN","mergeable":"CONFLICTING","comments":[
            {"author":{"login":"bot"},"createdAt":"2026-01-10T10:00:01Z","url":"https://x/c"}]}"#;
        expect_that!(
            came(json),
            some(eq(
                "- Merge conflicts with the base branch.\n- A comment by @bot: https://x/c"
            ))
        );
    }

    #[gtest]
    fn mergeable_or_unknown_is_no_news() {
        expect_that!(came(r#"{"state":"OPEN","mergeable":"MERGEABLE"}"#), none());
        expect_that!(came(r#"{"state":"OPEN","mergeable":"UNKNOWN"}"#), none());
    }

    #[gtest]
    fn a_closed_pull_request_with_a_conflict_says_only_that_it_closed() {
        expect_that!(
            came(r#"{"state":"CLOSED","mergeable":"CONFLICTING"}"#),
            some(eq("- Closed without merging."))
        );
    }

    #[gtest]
    fn only_a_comment_after_since_counts() {
        let json = r#"{"state":"OPEN","comments":[
            {"author":{"login":"old"},"createdAt":"2026-01-10T09:59:59Z","url":"https://x/c1"},
            {"author":{"login":"bot"},"createdAt":"2026-01-10T10:00:01Z","url":"https://x/c2"}]}"#;
        expect_that!(came(json), some(eq("- A comment by @bot: https://x/c2")));
    }

    #[gtest]
    fn a_submitted_review_counts_and_a_pending_one_does_not() {
        let json = r#"{"state":"OPEN","reviews":[
            {"author":{"login":"ann"},"submittedAt":"2026-01-10T11:00:00Z","state":"CHANGES_REQUESTED","url":"https://x/r1"},
            {"author":{"login":"bob"},"submittedAt":null,"state":"PENDING","url":"https://x/r2"},
            {"author":{"login":"cy"},"submittedAt":"2026-01-10T11:05:00Z","state":"APPROVED","url":"https://x/r3"}]}"#;
        expect_that!(
            came(json),
            some(eq(
                "- A review by @ann (changes requested): https://x/r1\n- A review by @cy (approved): https://x/r3"
            ))
        );
    }

    #[gtest]
    fn a_check_run_counts_only_when_it_failed_after_since() {
        let json = r#"{"state":"OPEN","statusCheckRollup":[
            {"__typename":"CheckRun","name":"build","conclusion":"FAILURE","completedAt":"2026-01-10T10:30:00Z","detailsUrl":"https://x/b"},
            {"__typename":"CheckRun","name":"lint","conclusion":"SUCCESS","completedAt":"2026-01-10T10:30:00Z","detailsUrl":"https://x/l"},
            {"__typename":"CheckRun","name":"e2e","conclusion":"CANCELLED","completedAt":"2026-01-10T10:30:00Z","detailsUrl":"https://x/e"},
            {"__typename":"CheckRun","name":"old","conclusion":"TIMED_OUT","completedAt":"2026-01-10T09:00:00Z","detailsUrl":"https://x/o"},
            {"__typename":"CheckRun","name":"slow","conclusion":null,"completedAt":null,"detailsUrl":"https://x/s"}]}"#;
        expect_that!(came(json), some(eq("- Check build failed: https://x/b")));
    }

    #[gtest]
    fn a_failing_status_context_counts() {
        let json = r#"{"state":"OPEN","statusCheckRollup":[
            {"__typename":"StatusContext","context":"ci/legacy","state":"ERROR","startedAt":"2026-01-10T10:30:00Z","targetUrl":"https://x/t"},
            {"__typename":"StatusContext","context":"ci/fine","state":"SUCCESS","startedAt":"2026-01-10T10:30:00Z","targetUrl":"https://x/u"}]}"#;
        expect_that!(came(json), some(eq("- Check ci/legacy failed: https://x/t")));
    }

    #[gtest]
    fn nothing_new_is_none() {
        let json = r#"{"state":"OPEN","comments":[{"author":{"login":"old"},"createdAt":"2026-01-01T00:00:00Z","url":"u"}],
            "reviews":[],"statusCheckRollup":[]}"#;
        expect_that!(came(json), none());
    }

    #[gtest]
    fn the_lines_come_in_order_and_are_cut_with_a_count_of_the_rest() {
        let json = r#"{"state":"MERGED","comments":[
            {"author":{"login":"a"},"createdAt":"2026-01-10T10:30:00Z","url":"u1"},
            {"author":{"login":"b"},"createdAt":"2026-01-10T10:31:00Z","url":"u2"},
            {"author":{"login":"c"},"createdAt":"2026-01-10T10:32:00Z","url":"u3"}]}"#;
        let whole = what_came(json, SINCE, LIMIT).expect("readable");
        expect_that!(
            whole,
            some(eq(
                "- Merged.\n- A comment by @a: u1\n- A comment by @b: u2\n- A comment by @c: u3"
            ))
        );
        // Room for two lines and the count.
        let cut = what_came(json, SINCE, 50).expect("readable");
        expect_that!(cut, some(eq("- Merged.\n- A comment by @a: u1\n- And 2 more.")));
    }

    #[gtest]
    fn what_isnt_json_is_an_error() {
        expect_that!(what_came("not json", SINCE, LIMIT), err(anything()));
    }
}
