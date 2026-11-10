//! A task's steps: an ordered checklist, each step open, ticked or dropped.
//!
//! Steps are their own rows with their own versions, so ticking one never conflicts with editing the item
//! (architecture.md, Data model). They are never deleted: a dropped step stays, shown as dropped.

use crate::item::{Item, ItemError, Kind, Title};
use crate::text::Context;
use crate::{ItemId, StepId, Timestamp, Version};

/// The most steps one task has, dropped ones included. This is the size assumption that lets every response that
/// carries an item carry all of its steps (architecture.md, Data model).
pub const MAX_STEPS: usize = 100;

/// A step's number within its item: given when the step is added, never changed and never reused. A step is named
/// by its item's key and this number, as `k3x9a.3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepNumber(u32);

impl StepNumber {
    pub const FIRST: Self = Self(1);

    /// `None` for zero.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// A positive integer written in digits, as in a path or on a command line. `None` for anything else, a sign
    /// and a leading zero included.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() || raw.starts_with('0') || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        raw.parse().ok().and_then(Self::new)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The number after this one. Saturates instead of wrapping, which no real item reaches.
    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl std::fmt::Display for StepNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Whether a step is still to do, done, or no longer wanted. A step can move from any state to any other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepState {
    Open,
    Ticked,
    Dropped,
}

impl StepState {
    pub const ALL: [Self; 3] = [Self::Open, Self::Ticked, Self::Dropped];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Ticked => "ticked",
            Self::Dropped => "dropped",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.code() == code)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: StepId,
    pub item_id: ItemId,
    pub number: StepNumber,
    /// Where the step sits in its item's order, from the first up. Separate from the number so reordering never
    /// renames a step. Positions are not necessarily consecutive.
    pub position: u32,
    pub title: Title,
    pub context: Option<Context>,
    pub state: StepState,
    pub created_at: Timestamp,
    pub version: Version,
}

/// The fields of a step being added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewStep {
    pub title: Title,
    pub context: Option<Context>,
    /// The step to come right after, or `None` for the end.
    pub after: Option<StepNumber>,
}

/// A partial edit. An outer `None` leaves the field unchanged; `Some(None)` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepPatch {
    pub title: Option<Title>,
    pub context: Option<Option<Context>>,
    pub state: Option<StepState>,
}

/// Where a new step goes: its number, and its position, which the steps after it make room for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepPlacement {
    pub number: StepNumber,
    pub position: u32,
}

/// A rule adding a step broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StepError {
    #[error("item {item_id} is a note, and only a task has steps; make it a task first")]
    NeedsTask { item_id: ItemId },
    #[error("item {item_id} already has {max} steps, the most a task can have")]
    TooMany { item_id: ItemId, max: usize },
    #[error("the item has no step {number} to put the new step after")]
    AfterNotFound { number: StepNumber },
}

impl Step {
    /// A step added at `placement` at `now`.
    pub fn new(id: StepId, item_id: ItemId, placement: StepPlacement, fields: NewStep, now: Timestamp) -> Self {
        Self {
            id,
            item_id,
            number: placement.number,
            position: placement.position,
            title: fields.title,
            context: fields.context,
            state: StepState::Open,
            created_at: now,
            version: Version::INITIAL,
        }
    }

    /// Applies `patch` and returns the edited step, or `None` when the patch changes nothing. The returned step
    /// keeps the old version; the store bumps it on write.
    pub fn apply_patch(&self, patch: &StepPatch) -> Option<Self> {
        let edited = Self {
            title: patch.title.clone().unwrap_or_else(|| self.title.clone()),
            context: patch.context.clone().unwrap_or_else(|| self.context.clone()),
            state: patch.state.unwrap_or(self.state),
            ..self.clone()
        };
        (edited != *self).then_some(edited)
    }
}

/// Whether `item`'s steps can be changed: not once it is closed. Reopening makes them editable again, unchanged.
pub const fn check_steps_editable(item: &Item) -> Result<(), ItemError> {
    if item.is_closed() {
        return Err(ItemError::Closed { item_id: item.id });
    }
    Ok(())
}

/// Checks that a step can be added to `item`, whose steps are `steps`, after step `after`, and says where it goes.
/// The number is one more than the highest the item has had, dropped steps included, so numbers are never reused.
pub fn place_step(item: &Item, steps: &[Step], after: Option<StepNumber>) -> Result<StepPlacement, StepError> {
    if item.kind != Kind::Task {
        return Err(StepError::NeedsTask { item_id: item.id });
    }
    if steps.len() >= MAX_STEPS {
        return Err(StepError::TooMany {
            item_id: item.id,
            max: MAX_STEPS,
        });
    }
    let number = steps
        .iter()
        .map(|step| step.number)
        .max()
        .map_or(StepNumber::FIRST, StepNumber::next);
    let position = match after {
        Some(after) => {
            let Some(previous) = steps.iter().find(|step| step.number == after) else {
                return Err(StepError::AfterNotFound { number: after });
            };
            previous.position.saturating_add(1)
        }
        None => steps
            .iter()
            .map(|step| step.position)
            .max()
            .map_or(0, |last| last.saturating_add(1)),
    };
    Ok(StepPlacement { number, position })
}

/// How far a task's steps have got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepProgress {
    pub ticked: u32,
    /// Open and ticked steps. Dropped ones don't count.
    pub total: u32,
}

impl StepProgress {
    /// The progress of a task with `ticked` ticked steps among `total` open and ticked ones, or `None` when
    /// `total` is 0. The one place that rule lives, for counts read from storage and for a full list of steps.
    pub fn from_counts(ticked: usize, total: usize) -> Option<Self> {
        (total > 0).then(|| Self {
            ticked: u32::try_from(ticked).unwrap_or(u32::MAX),
            total: u32::try_from(total).unwrap_or(u32::MAX),
        })
    }
}

/// The progress of `steps`, or `None` when none is open or ticked.
pub fn progress(steps: &[Step]) -> Option<StepProgress> {
    let ticked = steps.iter().filter(|step| step.state == StepState::Ticked).count();
    let total = steps.iter().filter(|step| step.state != StepState::Dropped).count();
    StepProgress::from_counts(ticked, total)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::handle::ItemKey;
    use crate::item::{ItemState, Need, NewItem};
    use crate::{HeapId, UserId};

    fn sample_item(kind: Kind, closed: bool) -> Item {
        let fields = NewItem {
            title: Title::parse("Rotate the certs").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
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
        item.kind = kind;
        if closed {
            item.state = ItemState::Done {
                closed_at: Timestamp::EPOCH,
                closed_by: None,
            };
        }
        item
    }

    fn number(value: u32) -> StepNumber {
        StepNumber::new(value).expect("positive")
    }

    fn sample_step(value: u32, position: u32, state: StepState) -> Step {
        Step {
            id: StepId::from_uuid(Uuid::from_u128(u128::from(value))),
            item_id: ItemId::from_uuid(Uuid::from_u128(3)),
            number: number(value),
            position,
            title: Title::parse("Run in staging").expect("valid"),
            context: None,
            state,
            created_at: Timestamp::EPOCH,
            version: Version::INITIAL,
        }
    }

    #[gtest]
    fn the_first_step_is_number_one_at_the_start() {
        let placed = place_step(&sample_item(Kind::Task, false), &[], None);

        expect_that!(
            placed,
            ok(eq(StepPlacement {
                number: number(1),
                position: 0
            }))
        );
    }

    #[gtest]
    fn the_next_number_is_one_past_the_highest_even_when_steps_were_dropped() {
        let steps = [
            sample_step(1, 0, StepState::Ticked),
            sample_step(2, 1, StepState::Dropped),
            sample_step(3, 2, StepState::Open),
        ];

        let placed = place_step(&sample_item(Kind::Task, false), &steps, None).expect("placed");

        expect_that!(placed.number, eq(number(4)));
        expect_that!(placed.position, eq(3));
    }

    #[gtest]
    fn a_step_added_after_another_goes_right_behind_it() {
        let steps = [sample_step(1, 0, StepState::Open), sample_step(2, 1, StepState::Open)];

        let placed = place_step(&sample_item(Kind::Task, false), &steps, Some(number(1))).expect("placed");

        expect_that!(placed.number, eq(number(3)));
        expect_that!(placed.position, eq(1));
    }

    #[gtest]
    fn a_step_can_go_after_a_dropped_one() {
        let steps = [sample_step(1, 0, StepState::Dropped)];

        let placed = place_step(&sample_item(Kind::Task, false), &steps, Some(number(1)));

        expect_that!(placed.map(|placed| placed.position), ok(eq(1)));
    }

    #[gtest]
    fn a_note_has_no_steps() {
        let item = sample_item(Kind::Note, false);

        expect_that!(
            place_step(&item, &[], None),
            err(eq(StepError::NeedsTask { item_id: item.id }))
        );
    }

    #[gtest]
    fn a_closed_item_refuses_step_changes() {
        let closed = sample_item(Kind::Task, true);

        expect_that!(
            check_steps_editable(&closed),
            err(eq(ItemError::Closed { item_id: closed.id }))
        );
        expect_that!(check_steps_editable(&sample_item(Kind::Task, false)), ok(()));
    }

    #[gtest]
    fn a_task_has_at_most_a_hundred_steps() {
        let item = sample_item(Kind::Task, false);
        let steps: Vec<Step> = (1..=100).map(|n| sample_step(n, n, StepState::Open)).collect();

        expect_that!(
            place_step(&item, &steps, None),
            err(eq(StepError::TooMany {
                item_id: item.id,
                max: MAX_STEPS
            }))
        );
        expect_that!(place_step(&item, &steps[..99], None), ok(anything()));
    }

    #[gtest]
    fn after_a_step_the_item_does_not_have_is_refused() {
        let steps = [sample_step(1, 0, StepState::Open)];

        expect_that!(
            place_step(&sample_item(Kind::Task, false), &steps, Some(number(9))),
            err(eq(StepError::AfterNotFound { number: number(9) }))
        );
    }

    #[gtest]
    fn a_patch_that_changes_nothing_gives_none() {
        let step = sample_step(1, 0, StepState::Open);

        expect_that!(step.apply_patch(&StepPatch::default()), none());
        expect_that!(
            step.apply_patch(&StepPatch {
                title: Some(Title::parse("Run in staging").expect("valid")),
                context: Some(None),
                state: Some(StepState::Open),
            }),
            none()
        );
    }

    #[gtest]
    fn a_patch_changes_each_field_and_any_state_to_any_state() {
        let open = sample_step(1, 0, StepState::Open);

        let retitled = open
            .apply_patch(&StepPatch {
                title: Some(Title::parse("Run in prod").expect("valid")),
                ..StepPatch::default()
            })
            .expect("changed");
        expect_that!(retitled.title.as_str(), eq("Run in prod"));

        let with_context = open
            .apply_patch(&StepPatch {
                context: Some(Context::parse("Use the blue stack").expect("valid")),
                ..StepPatch::default()
            })
            .expect("changed");
        expect_that!(
            with_context.context.as_ref().map(Context::as_str),
            some(eq("Use the blue stack"))
        );
        let cleared = with_context
            .apply_patch(&StepPatch {
                context: Some(None),
                ..StepPatch::default()
            })
            .expect("changed");
        expect_that!(cleared.context, none());

        for from in StepState::ALL {
            for to in StepState::ALL {
                let moved = sample_step(1, 0, from).apply_patch(&StepPatch {
                    state: Some(to),
                    ..StepPatch::default()
                });
                expect_that!(moved.map(|moved| moved.state), eq((from != to).then_some(to)));
            }
        }
    }

    #[gtest]
    fn progress_counts_ticked_of_open_and_ticked() {
        let steps = [
            sample_step(1, 0, StepState::Ticked),
            sample_step(2, 1, StepState::Ticked),
            sample_step(3, 2, StepState::Open),
            sample_step(4, 3, StepState::Dropped),
        ];

        expect_that!(progress(&steps), some(eq(StepProgress { ticked: 2, total: 3 })));
    }

    #[gtest]
    fn progress_is_none_without_an_open_or_ticked_step() {
        expect_that!(progress(&[]), none());
        expect_that!(progress(&[sample_step(1, 0, StepState::Dropped)]), none());
    }

    #[gtest]
    fn a_step_number_is_a_positive_integer_in_digits() {
        expect_that!(StepNumber::parse("3"), some(eq(number(3))));
        for bad in ["", "0", "-1", "+1", "abc", "1.5", "03", " 3", "99999999999"] {
            expect_that!(StepNumber::parse(bad), none());
        }
    }
}
