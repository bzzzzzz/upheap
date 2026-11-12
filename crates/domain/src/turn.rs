//! Turns: an item handed to an agent, and taken back (Work with agents, Concepts and Opening a turn).
//!
//! A turn is its own row, so sending an item or taking it back never conflicts with editing it (architecture.md,
//! Data model). An item has at most one open turn. Turns are never deleted: a taken-back one keeps its row.

use crate::heap::Heap;
use crate::item::{Item, Kind, Stage};
use crate::run::STUCK_AFTER;
use crate::text::parse_optional;
use crate::validation::Field;
use crate::{ItemId, Timestamp, TurnId, UserId, ValidationError};

/// How far a turn is meant to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reach {
    /// To the next stable state, then back to the person.
    One,
    /// On through the stable states until the work is done. Each turn still goes one stable state; the next opens
    /// when it is reached (see [`Turn::follow_on`]).
    AllTheWay,
}

impl Reach {
    pub const ALL: [Self; 2] = [Self::One, Self::AllTheWay];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::AllTheWay => "all_the_way",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reach| reach.code() == code)
    }
}

/// The stable state a turn goes to, worked out from the item when the turn opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Goal {
    WithContext,
    Defined,
    Done,
}

impl Goal {
    pub const ALL: [Self; 3] = [Self::WithContext, Self::Defined, Self::Done];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::WithContext => "with_context",
            Self::Defined => "defined",
            Self::Done => "done",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|goal| goal.code() == code)
    }

    /// The goal in words, as a sentence names it: "with context".
    pub const fn phrase(self) -> &'static str {
        match self {
            Self::WithContext => "with context",
            Self::Defined => "defined",
            Self::Done => "done",
        }
    }

    /// The stage an item is at when a turn reaches this goal, or `None` for a goal that is not a stage: done closes
    /// the item.
    pub const fn stage(self) -> Option<Stage> {
        match self {
            Self::WithContext => Some(Stage::WithContext),
            Self::Defined => Some(Stage::Defined),
            Self::Done => None,
        }
    }
}

/// How a turn came to be opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TurnSource {
    /// The person sent the item.
    GoAhead,
    /// The heap is set so agents prepare every new item.
    Prepare,
}

impl TurnSource {
    pub const ALL: [Self; 2] = [Self::GoAhead, Self::Prepare];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::GoAhead => "go_ahead",
            Self::Prepare => "prepare",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.code() == code)
    }
}

/// How a turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TurnEnd {
    TakenBack,
    /// A run said the turn's goal was reached, and the check passed.
    Reached,
}

impl TurnEnd {
    pub const ALL: [Self; 2] = [Self::TakenBack, Self::Reached];

    /// Stable code used in storage. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TakenBack => "taken_back",
            Self::Reached => "reached",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|end| end.code() == code)
    }
}

/// Why a turn was taken back, as its audit entry says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TakeBackWhy {
    /// The person took it back.
    TakeBack,
    /// The person started the item.
    Started,
    /// The item was closed.
    Closed,
    /// The heap was archived.
    HeapArchived,
}

impl TakeBackWhy {
    pub const ALL: [Self; 4] = [Self::TakeBack, Self::Started, Self::Closed, Self::HeapArchived];

    /// Stable code used in audit diffs. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::TakeBack => "take_back",
            Self::Started => "started",
            Self::Closed => "closed",
            Self::HeapArchived => "heap_archived",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|why| why.code() == code)
    }
}

/// The optional text the person sends an agent with: trimmed, blank means none, at most 2,000 characters. Line
/// breaks are allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnLine(String);

impl TurnLine {
    pub const MAX_CHARS: usize = 2_000;

    pub fn parse(raw: &str) -> Result<Option<Self>, ValidationError> {
        parse_optional(raw, Field::Line, Self::MAX_CHARS).map(|text| text.map(Self))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How and when a turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ended {
    pub end: TurnEnd,
    pub at: Timestamp,
    /// `None` when it can't be traced.
    pub by: Option<UserId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub id: TurnId,
    pub item_id: ItemId,
    pub reach: Reach,
    /// The stable state it goes to, fixed when it opened: a note made a task meanwhile doesn't change it.
    pub goal: Goal,
    pub line: Option<TurnLine>,
    /// `None` when it can't be traced.
    pub opened_by: Option<UserId>,
    pub source: TurnSource,
    pub opened_at: Timestamp,
    /// `None` while the turn is open.
    pub ended: Option<Ended>,
    /// How many of its runs in a row failed. A run that asked a question sets it back to 0. At [`STUCK_AFTER`] the
    /// turn is stuck.
    pub failed_runs: u32,
}

/// A rule opening or retrying a turn broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TurnError {
    #[error("item {item_id} already has an open turn")]
    AlreadyOpen { item_id: ItemId },
    /// A retry on an item whose turn is not stuck, or has ended.
    #[error("item {item_id} has no stuck turn")]
    NotStuck { item_id: ItemId },
    /// A go-ahead on a reminder. It must become a task first.
    #[error("item {item_id} is a reminder; make it a task before sending it to an agent")]
    NeedsNoteOrTask { item_id: ItemId },
    /// A go-ahead on a blocked item. It must be unblocked first.
    #[error("item {item_id} is blocked; unblock it before sending it to an agent")]
    Blocked { item_id: ItemId },
}

impl Turn {
    /// A turn opened on `item` by `opened_by` at `now`, going to the goal [`goal_for`] says.
    pub fn open(
        id: TurnId,
        item: &Item,
        reach: Reach,
        line: Option<TurnLine>,
        source: TurnSource,
        opened_by: UserId,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            item_id: item.id,
            reach,
            goal: goal_for(item),
            line,
            opened_by: Some(opened_by),
            source,
            opened_at: now,
            ended: None,
            failed_runs: 0,
        }
    }

    pub const fn is_open(&self) -> bool {
        self.ended.is_none()
    }

    /// Whether the turn's runs keep failing: it is open, and has failed [`STUCK_AFTER`] runs in a row. A stuck turn is
    /// not in the queue, and its item waits for the person.
    pub const fn is_stuck(&self) -> bool {
        self.ended.is_none() && self.failed_runs >= STUCK_AFTER
    }

    /// The turn as it reads right now: when it has a run nobody has heard from for [`crate::run::CLAIM_LAPSE`]
    /// (`lapsed_unended`), that run counts as failed though it isn't written yet (architecture.md, Runs), so
    /// `failed_runs` is one more. Every place that reads whether a turn is stuck, or how many runs failed, reads
    /// through this, so none disagrees with another. Never write the result back: the stored count only grows when a
    /// lapsed run is written.
    #[must_use]
    pub fn counting_lapse(&self, lapsed_unended: bool) -> Self {
        Self {
            failed_runs: self.failed_runs.saturating_add(u32::from(lapsed_unended)),
            ..self.clone()
        }
    }

    /// This turn ended because its goal was reached, by `by` at `now`.
    #[must_use]
    pub fn reached(&self, by: UserId, now: Timestamp) -> Self {
        Self {
            ended: Some(Ended {
                end: TurnEnd::Reached,
                at: now,
                by: Some(by),
            }),
            ..self.clone()
        }
    }

    /// The turn that opens when this one, sent all the way, is reached short of done: the same reach, line, source and
    /// opener, going to the goal [`goal_for`] says for `item` as it is once this turn is reached. `None` for a turn
    /// of one state, for one that reached done, and for a reminder, which takes no turn.
    #[must_use]
    pub fn follow_on(&self, id: TurnId, item: &Item, now: Timestamp) -> Option<Self> {
        if self.reach != Reach::AllTheWay || self.goal == Goal::Done || item.kind == Kind::Reminder {
            return None;
        }
        Some(Self {
            id,
            item_id: self.item_id,
            reach: self.reach,
            goal: goal_for(item),
            line: self.line.clone(),
            opened_by: self.opened_by,
            source: self.source,
            opened_at: now,
            ended: None,
            failed_runs: 0,
        })
    }

    /// This turn taken back by `by` at `now`.
    #[must_use]
    pub fn take_back(&self, by: UserId, now: Timestamp) -> Self {
        Self {
            ended: Some(Ended {
                end: TurnEnd::TakenBack,
                at: now,
                by: Some(by),
            }),
            ..self.clone()
        }
    }
}

/// The stable state a turn opened on `item` goes to: a note as written to with context, a note with context to
/// defined (it goes on by becoming tasks), a task as written or with context to defined, and a defined task to done.
pub const fn goal_for(item: &Item) -> Goal {
    match (item.kind, item.stage) {
        (Kind::Note, Stage::AsWritten) => Goal::WithContext,
        (Kind::Note, _) | (Kind::Task, Stage::AsWritten | Stage::WithContext) => Goal::Defined,
        (Kind::Task, Stage::Defined) => Goal::Done,
        // No turn opens on a reminder (`send_to_agent` refuses it), so this is never used; it keeps the match total.
        (Kind::Reminder, _) => Goal::WithContext,
    }
}

/// Whether an agent has `item`: it is open, its heap is active, it has an open turn, and it doesn't wait for the
/// person. `open_questions` is how many open questions it has, `stuck` whether its open turn is stuck, and
/// `blocker_gone` whether its blocker is gone (see [`crate::question::waits_for_you`]).
pub const fn with_agent(
    item: &Item,
    heap: &Heap,
    open_questions: usize,
    has_open_turn: bool,
    stuck: bool,
    blocker_gone: bool,
) -> bool {
    has_open_turn && !stuck && !blocker_gone && !item.is_closed() && !heap.is_archived() && open_questions == 0
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::HeapId;
    use crate::Version;
    use crate::handle::{Handle, ItemKey};
    use crate::item::{ItemState, Need, NewItem, Title};
    use crate::text::Name;

    fn item(kind: Kind, stage: Stage) -> Item {
        let fields = NewItem {
            kind,
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
        let mut item = Item::new(
            ItemId::from_uuid(Uuid::from_u128(3)),
            HeapId::from_uuid(Uuid::from_u128(2)),
            ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        );
        item.stage = stage;
        item
    }

    fn heap(archived: bool) -> Heap {
        Heap {
            id: HeapId::from_uuid(Uuid::from_u128(2)),
            team_id: crate::TeamId::from_uuid(Uuid::from_u128(1)),
            slug: Handle::parse("ops", Field::Slug).expect("valid"),
            name: Name::parse("Ops").expect("valid"),
            policy: crate::heap::HeapPolicy::default(),
            context: None,
            agents_prepare: false,
            archived_at: archived.then_some(Timestamp::EPOCH),
            version: Version::INITIAL,
        }
    }

    #[gtest]
    fn the_goal_is_the_next_stable_state() {
        expect_that!(goal_for(&item(Kind::Note, Stage::AsWritten)), eq(Goal::WithContext));
        expect_that!(goal_for(&item(Kind::Note, Stage::WithContext)), eq(Goal::Defined));
        expect_that!(goal_for(&item(Kind::Task, Stage::AsWritten)), eq(Goal::Defined));
        expect_that!(goal_for(&item(Kind::Task, Stage::WithContext)), eq(Goal::Defined));
        expect_that!(goal_for(&item(Kind::Task, Stage::Defined)), eq(Goal::Done));
        // Never used: no turn opens on a reminder.
        expect_that!(goal_for(&item(Kind::Reminder, Stage::AsWritten)), eq(Goal::WithContext));
    }

    #[gtest]
    fn a_line_is_trimmed_blank_means_none_and_is_bounded() {
        expect_that!(TurnLine::parse(" \n "), ok(none()));
        expect_that!(
            TurnLine::parse("  use the new CA\nplease ").map(|line| line.map(|line| line.as_str().to_owned())),
            ok(some(eq("use the new CA\nplease")))
        );
        expect_that!(TurnLine::parse(&"x".repeat(2_000)), ok(some(anything())));
        expect_that!(
            TurnLine::parse(&"x".repeat(2_001)),
            err(eq(&ValidationError::TooLong {
                field: Field::Line,
                max: 2_000
            }))
        );
    }

    #[gtest]
    fn an_agent_has_an_open_item_with_a_turn_that_does_not_wait() {
        let open = item(Kind::Note, Stage::AsWritten);
        expect_that!(with_agent(&open, &heap(false), 0, true, false, false), eq(true));
        expect_that!(with_agent(&open, &heap(false), 0, false, false, false), eq(false));
        expect_that!(with_agent(&open, &heap(false), 1, true, false, false), eq(false));
        expect_that!(with_agent(&open, &heap(true), 0, true, false, false), eq(false));
        // A stuck turn waits for the person.
        expect_that!(with_agent(&open, &heap(false), 0, true, true, false), eq(false));
        // So does an item whose blocker is gone.
        expect_that!(with_agent(&open, &heap(false), 0, true, false, true), eq(false));
        let mut closed = open;
        closed.state = ItemState::Done {
            closed_at: Timestamp::EPOCH,
            closed_by: None,
        };
        expect_that!(with_agent(&closed, &heap(false), 0, true, false, false), eq(false));
    }

    fn all_the_way(note: &Item) -> Turn {
        let mut turn = Turn::open(
            TurnId::from_uuid(Uuid::from_u128(40)),
            note,
            Reach::AllTheWay,
            TurnLine::parse("Use the new CA.").expect("valid"),
            TurnSource::GoAhead,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        );
        turn.failed_runs = 2;
        turn
    }

    #[gtest]
    fn a_turn_sent_all_the_way_is_followed_by_one_to_the_next_state() {
        // Arrange
        let reached_item = item(Kind::Note, Stage::WithContext);
        let turn = all_the_way(&item(Kind::Note, Stage::AsWritten));
        let id = TurnId::from_uuid(Uuid::from_u128(41));
        let now = Timestamp::EPOCH;

        // Act
        let next = turn.follow_on(id, &reached_item, now);

        // Assert
        expect_that!(
            next,
            some(eq(&Turn {
                id,
                item_id: turn.item_id,
                reach: Reach::AllTheWay,
                goal: Goal::Defined,
                line: turn.line.clone(),
                opened_by: turn.opened_by,
                source: TurnSource::GoAhead,
                opened_at: now,
                ended: None,
                failed_runs: 0,
            }))
        );
    }

    #[gtest]
    fn the_follow_on_of_a_defined_task_goes_to_done() {
        let turn = all_the_way(&item(Kind::Task, Stage::WithContext));
        let next = turn.follow_on(
            TurnId::from_uuid(Uuid::from_u128(41)),
            &item(Kind::Task, Stage::Defined),
            Timestamp::EPOCH,
        );
        expect_that!(next.map(|next| next.goal), some(eq(Goal::Done)));
    }

    #[gtest]
    fn a_turn_of_one_state_a_turn_to_done_and_a_reminder_have_no_follow_on() {
        let id = TurnId::from_uuid(Uuid::from_u128(41));
        let note = item(Kind::Note, Stage::WithContext);
        let mut one = all_the_way(&note);
        one.reach = Reach::One;
        expect_that!(one.follow_on(id, &note, Timestamp::EPOCH), none());

        let mut to_done = all_the_way(&note);
        to_done.goal = Goal::Done;
        expect_that!(to_done.follow_on(id, &note, Timestamp::EPOCH), none());

        let turn = all_the_way(&note);
        let reminder = item(Kind::Reminder, Stage::AsWritten);
        expect_that!(turn.follow_on(id, &reminder, Timestamp::EPOCH), none());
    }

    #[gtest]
    fn taking_a_turn_back_ends_it() {
        let note = item(Kind::Note, Stage::AsWritten);
        let user = UserId::from_uuid(Uuid::from_u128(10));
        let turn = Turn::open(
            TurnId::from_uuid(Uuid::from_u128(40)),
            &note,
            Reach::One,
            None,
            TurnSource::GoAhead,
            user,
            Timestamp::EPOCH,
        );
        expect_that!(turn.is_open(), eq(true));

        let ended = turn.take_back(user, Timestamp::EPOCH);

        expect_that!(ended.is_open(), eq(false));
        expect_that!(ended.ended.map(|ended| ended.end), some(eq(TurnEnd::TakenBack)));
        expect_that!(ended.goal, eq(Goal::WithContext));
    }
}

#[cfg(test)]
mod run_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::HeapId;
    use crate::handle::ItemKey;
    use crate::item::{Need, NewItem, Title};

    fn note() -> Item {
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

    fn open_turn() -> Turn {
        Turn::open(
            TurnId::from_uuid(Uuid::from_u128(40)),
            &note(),
            Reach::One,
            None,
            TurnSource::GoAhead,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        )
    }

    #[gtest]
    fn a_turn_is_stuck_after_three_failed_runs_in_a_row() {
        let mut turn = open_turn();

        turn.failed_runs = 2;
        expect_that!(turn.is_stuck(), eq(false));
        turn.failed_runs = 3;
        expect_that!(turn.is_stuck(), eq(true));
        // An ended turn isn't stuck, whatever it failed before.
        expect_that!(
            turn.take_back(UserId::from_uuid(Uuid::from_u128(10)), Timestamp::EPOCH)
                .is_stuck(),
            eq(false)
        );
    }

    #[gtest]
    fn reaching_the_goal_ends_the_turn() {
        let ended = open_turn().reached(UserId::from_uuid(Uuid::from_u128(10)), Timestamp::EPOCH);

        expect_that!(ended.is_open(), eq(false));
        expect_that!(ended.ended.map(|ended| ended.end), some(eq(TurnEnd::Reached)));
        expect_that!(TurnEnd::from_code("reached"), some(eq(TurnEnd::Reached)));
    }

    #[gtest]
    fn a_goal_is_a_stage_except_done() {
        expect_that!(Goal::WithContext.stage(), some(eq(Stage::WithContext)));
        expect_that!(Goal::Defined.stage(), some(eq(Stage::Defined)));
        expect_that!(Goal::Done.stage(), none());
    }
}
