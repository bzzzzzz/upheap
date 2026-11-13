//! Runs: one go an agent has at the turn of an item (Work with agents, Runs and The agent loop).
//!
//! A run is claimed from the queue, holds one item, and ends one of five ways. An agent that goes quiet is not waited
//! for: a run nobody has heard from in [`CLAIM_LAPSE`] counts as ended, failed, wherever it is read, and is written
//! as failed the next time something touches it (architecture.md, Runs).

use std::fmt;

use crate::handle::ItemKey;
use crate::item::{Item, ItemState, Kind, Stage};
use crate::text::parse_required;
use crate::turn::Goal;
use crate::validation::Field;
use crate::{Duration, ItemId, RunId, Timestamp, TurnId, UserId, ValidationError};

/// How long a run may go without being heard from, by a heartbeat or a write, before it lapses.
pub const CLAIM_LAPSE: Duration = Duration::from_minutes(30);

/// How many runs in a row a turn may fail before it is stuck.
pub const STUCK_AFTER: u32 = 3;

/// Why a lapsed run failed.
pub const LAPSE_FAILURE: &str = "Nothing was heard from the run for 30 minutes.";

/// Most heaps the queue is asked about at once.
pub const MAX_QUEUE_HEAPS: usize = 20;

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunEnd {
    /// It said the turn's goal was reached, and the check passed.
    Reached,
    /// It asked one or more questions, and the person has the item.
    Asked,
    /// It ended without either, or went quiet. Counts toward a stuck turn.
    Failed,
    /// The person took the turn back while the run was live. Not a failure.
    TakenBack,
    /// It found nothing to do for the goal and gave the turn back to the person. Not a failure.
    HandedBack,
}

impl RunEnd {
    pub const ALL: [Self; 5] = [
        Self::Reached,
        Self::Asked,
        Self::Failed,
        Self::TakenBack,
        Self::HandedBack,
    ];

    /// Stable code used in storage and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Reached => "reached",
            Self::Asked => "asked",
            Self::Failed => "failed",
            Self::TakenBack => "taken_back",
            Self::HandedBack => "handed_back",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|end| end.code() == code)
    }
}

/// What a run says it reached, or why it handed the turn back: trimmed, 1 to 2,000 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult(String);

impl RunResult {
    pub const MAX_CHARS: usize = 2_000;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Result, Self::MAX_CHARS).map(Self)
    }

    /// Why a run handed the turn back, which is stored as its result: the same text rules, reported as `reason`.
    pub fn parse_reason(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Reason, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A link a run reached with: an absolute `http` or `https` address of up to 2,000 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunLink(String);

impl RunLink {
    pub const MAX_CHARS: usize = 2_000;
    /// Most links one run carries.
    pub const MAX_COUNT: usize = 10;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let text = parse_required(raw, Field::Links, Self::MAX_CHARS)?;
        let lowered = text.to_ascii_lowercase();
        let rest = lowered
            .strip_prefix("https://")
            .or_else(|| lowered.strip_prefix("http://"))
            .ok_or(ValidationError::FieldInvalid { field: Field::Links })?;
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let clean = !host.is_empty() && !text.chars().any(|c| c.is_whitespace() || c.is_control());
        if !clean {
            return Err(ValidationError::FieldInvalid { field: Field::Links });
        }
        Ok(Self(text))
    }

    /// Checks `raw` as a run's whole list of links.
    pub fn parse_all<S: AsRef<str>>(raw: &[S]) -> Result<Vec<Self>, ValidationError> {
        if raw.len() > Self::MAX_COUNT {
            return Err(ValidationError::FieldInvalid { field: Field::Links });
        }
        raw.iter().map(|link| Self::parse(link.as_ref())).collect()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a run ended and what it left behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunEnded {
    pub end: RunEnd,
    pub at: Timestamp,
    /// What the run said it reached. Set exactly when it ended as [`RunEnd::Reached`] or [`RunEnd::HandedBack`],
    /// where it is the reason.
    pub result: Option<RunResult>,
    pub links: Vec<RunLink>,
    /// Why it failed. Set exactly when it ended as [`RunEnd::Failed`].
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub id: RunId,
    pub item_id: ItemId,
    pub turn_id: TurnId,
    /// The user the run belongs to: whoever claimed it. Nobody else may use it.
    pub user_id: UserId,
    pub started_at: Timestamp,
    /// When the run was last heard from: a heartbeat or a write made in it.
    pub heard_at: Timestamp,
    /// The item's stage when the run started.
    pub from_stage: Stage,
    /// `None` while the run has not ended.
    pub ended: Option<RunEnded>,
    /// What the run cost, in millionths of a US dollar. Set once, by `finish`.
    pub cost_micro_usd: Option<i64>,
}

impl Run {
    /// A run started at `now` by `user_id` on `item`, whose open turn is `turn_id`.
    pub fn start(id: RunId, item: &Item, turn_id: TurnId, user_id: UserId, now: Timestamp) -> Self {
        Self {
            id,
            item_id: item.id,
            turn_id,
            user_id,
            started_at: now,
            heard_at: now,
            from_stage: item.stage,
            ended: None,
            cost_micro_usd: None,
        }
    }

    /// Whether the run has not ended and was heard from less than [`CLAIM_LAPSE`] before `now`.
    pub fn is_live(&self, now: Timestamp) -> bool {
        self.ended.is_none() && now.since(self.heard_at) < CLAIM_LAPSE
    }

    /// Whether the run has not ended but has gone quiet for [`CLAIM_LAPSE`]. It counts as ended, failed.
    pub fn is_lapsed(&self, now: Timestamp) -> bool {
        self.ended.is_none() && !self.is_live(now)
    }

    /// This run ended as `end` at `at`.
    #[must_use]
    pub fn ended_as(
        &self,
        end: RunEnd,
        at: Timestamp,
        result: Option<RunResult>,
        links: Vec<RunLink>,
        failure: Option<String>,
    ) -> Self {
        Self {
            ended: Some(RunEnded {
                end,
                at,
                result,
                links,
                failure,
            }),
            ..self.clone()
        }
    }
}

/// A rule using a run broke.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// The run has ended or lapsed, or its turn is no longer open.
    #[error("This run is over.")]
    Over,
    /// A write in a run names an item other than the run's.
    #[error("This run is for item {key}.")]
    WrongItem { key: ItemKey },
    /// The cost of the run was sent before: it is set once.
    #[error("The run's cost is already recorded.")]
    CostRecorded,
    /// A write only the person makes, tried inside a run.
    #[error("{0}")]
    Forbidden(RunForbidden),
}

/// What a run is refused, and the message it gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunForbidden {
    /// Anything outside what a run may write.
    Person,
    /// Closing a task as done, which an agent does only after an approved review.
    Done,
}

impl fmt::Display for RunForbidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Person => "Only the person does that.",
            Self::Done => "An agent closes a task as done only after an approved review, which isn't available yet.",
        })
    }
}

/// The first condition of a goal that an item does not meet, as `work reached` reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NotReached {
    #[error("The item is closed.")]
    Closed,
    #[error("The item has no context yet.")]
    NoContext,
    #[error("The item is a note; a defined item is a task.")]
    NotATask,
    #[error("The task has no steps.")]
    NoSteps,
    #[error("The item has an open question.")]
    OpenQuestion,
    #[error("Reaching done takes an approved review, which isn't available yet.")]
    DoneNeedsReview,
}

/// Checks that `item` is at `goal`, as it is now: `open_questions` is how many open questions it has and `live_steps`
/// how many steps it has that are not dropped.
///
/// - `with_context`: open, a description, no open question.
/// - `defined`: open, a task, a description, a step, no open question.
/// - `done`: never passes until reviews exist.
pub fn reached_check(item: &Item, goal: Goal, open_questions: usize, live_steps: usize) -> Result<(), NotReached> {
    if goal == Goal::Done {
        return Err(NotReached::DoneNeedsReview);
    }
    if matches!(item.state, ItemState::Done { .. } | ItemState::WontDo { .. }) {
        return Err(NotReached::Closed);
    }
    if goal == Goal::Defined && item.kind != Kind::Task {
        return Err(NotReached::NotATask);
    }
    if item.description.is_none() {
        return Err(NotReached::NoContext);
    }
    if goal == Goal::Defined && live_steps == 0 {
        return Err(NotReached::NoSteps);
    }
    if open_questions > 0 {
        return Err(NotReached::OpenQuestion);
    }
    Ok(())
}

/// The failure text of a run that ended without saying it reached `goal` or asking a question.
pub fn default_failure(goal: Goal) -> String {
    format!(
        "The run ended without saying it reached {} or asking a question.",
        goal.phrase()
    )
}

/// Most characters of a failure text kept.
pub const MAX_FAILURE_CHARS: usize = 2_000;

/// A failure text from a tool: trimmed and cut to [`MAX_FAILURE_CHARS`] characters, on a character boundary. Not
/// refused when it is long. Blank gives `None`.
pub fn cut_failure(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_FAILURE_CHARS).collect())
}

/// Most a run may cost, in US dollars.
pub const MAX_COST_USD: f64 = 1_000_000.0;

/// A cost in US dollars as whole millionths of a dollar, rounded. Fails unless it is a finite number from 0 to
/// [`MAX_COST_USD`].
pub fn parse_cost_usd(usd: f64) -> Result<i64, ValidationError> {
    if !usd.is_finite() || !(0.0..=MAX_COST_USD).contains(&usd) {
        return Err(ValidationError::FieldInvalid { field: Field::CostUsd });
    }
    // At most 10^12, so it fits.
    Ok((usd * 1_000_000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::HeapId;
    use crate::item::{Description, Need, NewItem, Title};

    fn at(minutes: i64) -> Timestamp {
        Timestamp::from_millis(Duration::from_minutes(minutes).as_millis()).expect("in range")
    }

    fn item() -> Item {
        let fields = NewItem {
            kind: Kind::Note,
            title: Title::parse("Rotate the certs").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
        };
        Item::new(
            ItemId::from_uuid(Uuid::from_u128(3)),
            HeapId::from_uuid(Uuid::from_u128(2)),
            ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        )
    }

    fn with_context(mut item: Item) -> Item {
        item.description = Description::parse("Why and how").expect("valid");
        item
    }

    fn task(mut item: Item) -> Item {
        item.kind = Kind::Task;
        item
    }

    fn run() -> Run {
        Run::start(
            RunId::from_uuid(Uuid::from_u128(50)),
            &item(),
            TurnId::from_uuid(Uuid::from_u128(40)),
            UserId::from_uuid(Uuid::from_u128(10)),
            at(0),
        )
    }

    #[gtest]
    fn a_note_reaches_with_context_when_it_has_a_description_and_no_open_question() {
        let note = item();
        expect_that!(
            reached_check(&note, Goal::WithContext, 0, 0),
            err(eq(NotReached::NoContext))
        );
        let note = with_context(note);
        expect_that!(reached_check(&note, Goal::WithContext, 0, 0), ok(()));
        expect_that!(
            reached_check(&note, Goal::WithContext, 1, 0),
            err(eq(NotReached::OpenQuestion))
        );
    }

    #[gtest]
    fn a_task_reaches_defined_with_a_description_a_live_step_and_no_open_question() {
        let note = with_context(item());
        expect_that!(reached_check(&note, Goal::Defined, 0, 1), err(eq(NotReached::NotATask)));
        let bare = task(item());
        expect_that!(
            reached_check(&bare, Goal::Defined, 0, 1),
            err(eq(NotReached::NoContext))
        );
        let ready = with_context(task(item()));
        expect_that!(reached_check(&ready, Goal::Defined, 0, 0), err(eq(NotReached::NoSteps)));
        expect_that!(
            reached_check(&ready, Goal::Defined, 1, 2),
            err(eq(NotReached::OpenQuestion))
        );
        expect_that!(reached_check(&ready, Goal::Defined, 0, 2), ok(()));
    }

    #[gtest]
    fn done_never_passes_and_a_closed_item_does_not_reach_anything() {
        let ready = with_context(task(item()));
        expect_that!(
            reached_check(&ready, Goal::Done, 0, 3),
            err(eq(NotReached::DoneNeedsReview))
        );
        let mut closed = with_context(item());
        closed.state = ItemState::Done {
            closed_at: Timestamp::EPOCH,
            closed_by: None,
        };
        expect_that!(
            reached_check(&closed, Goal::WithContext, 0, 0),
            err(eq(NotReached::Closed))
        );
    }

    #[gtest]
    fn a_run_is_live_until_it_has_been_quiet_for_the_whole_lapse() {
        let run = run();

        expect_that!(run.is_live(at(0)), eq(true));
        expect_that!(run.is_live(at(29)), eq(true));
        // Exactly the lapse is lapsed: heard_at must be later than now minus the lapse.
        expect_that!(run.is_live(at(30)), eq(false));
        expect_that!(run.is_lapsed(at(30)), eq(true));
        expect_that!(run.is_lapsed(at(29)), eq(false));
    }

    #[gtest]
    fn an_ended_run_is_neither_live_nor_lapsed() {
        let ended = run().ended_as(RunEnd::Asked, at(1), None, Vec::new(), None);

        expect_that!(ended.is_live(at(2)), eq(false));
        expect_that!(ended.is_lapsed(at(2000)), eq(false));
        expect_that!(ended.ended.map(|ended| ended.end), some(eq(RunEnd::Asked)));
    }

    #[gtest]
    fn a_failure_is_trimmed_cut_on_a_character_boundary_and_never_refused() {
        expect_that!(cut_failure("  \n "), none());
        expect_that!(cut_failure("  boom \n").as_deref(), some(eq("boom")));
        let long = "é".repeat(2_500);
        let cut = cut_failure(&long).expect("some");
        expect_that!(cut.chars().count(), eq(2_000));
        expect_that!(cut.len(), eq(4_000));
    }

    #[gtest]
    fn a_result_is_required_and_bounded() {
        expect_that!(
            RunResult::parse("  Added the context \n").map(|result| result.as_str().to_owned()),
            ok(eq("Added the context"))
        );
        expect_that!(
            RunResult::parse(" "),
            err(eq(&ValidationError::Empty { field: Field::Result }))
        );
        expect_that!(RunResult::parse(&"x".repeat(2_000)), ok(anything()));
        expect_that!(
            RunResult::parse(&"x".repeat(2_001)),
            err(eq(&ValidationError::TooLong {
                field: Field::Result,
                max: 2_000
            }))
        );
    }

    #[gtest]
    fn links_are_absolute_web_addresses_up_to_ten() {
        expect_that!(RunLink::parse("https://example.com/pr/1"), ok(anything()));
        expect_that!(RunLink::parse("HTTP://example.com"), ok(anything()));
        for bad in [
            "example.com",
            "ftp://example.com",
            "https://",
            "https:///path",
            "https://exa mple.com",
            "javascript:alert(1)",
        ] {
            expect_that!(
                RunLink::parse(bad),
                err(eq(&ValidationError::FieldInvalid { field: Field::Links })),
                "{bad}"
            );
        }
        let ten = vec!["https://example.com"; 10];
        expect_that!(RunLink::parse_all(&ten), ok(len(eq(10))));
        let eleven = vec!["https://example.com"; 11];
        expect_that!(
            RunLink::parse_all(&eleven),
            err(eq(&ValidationError::FieldInvalid { field: Field::Links }))
        );
        let long = format!("https://example.com/{}", "x".repeat(2_000));
        expect_that!(
            RunLink::parse(&long),
            err(eq(&ValidationError::TooLong {
                field: Field::Links,
                max: 2_000
            }))
        );
    }

    #[gtest]
    fn a_cost_is_a_finite_number_of_dollars_kept_in_millionths() {
        expect_that!(parse_cost_usd(0.0), ok(eq(&0)));
        expect_that!(parse_cost_usd(0.31), ok(eq(&310_000)));
        expect_that!(parse_cost_usd(1_000_000.0), ok(eq(&1_000_000_000_000)));
        for bad in [-0.01, 1_000_000.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            expect_that!(
                parse_cost_usd(bad),
                err(eq(&ValidationError::FieldInvalid { field: Field::CostUsd })),
                "{bad}"
            );
        }
    }

    #[gtest]
    fn the_default_failure_names_the_goal() {
        expect_that!(
            default_failure(Goal::WithContext),
            eq("The run ended without saying it reached with context or asking a question.")
        );
    }

    #[gtest]
    fn every_end_reads_back_from_its_code() {
        for end in RunEnd::ALL {
            expect_that!(RunEnd::from_code(end.code()), some(eq(end)));
        }
        expect_that!(RunEnd::from_code("handed_back"), some(eq(RunEnd::HandedBack)));
        expect_that!(RunEnd::from_code("lapsed"), none());
    }
}
