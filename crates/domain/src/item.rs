//! Items and their state machine.

use crate::text::parse_required;
use crate::validation::{Field, ValidationError};
use crate::{Duration, HeapId, ItemId, Timestamp, Version};

/// A closed item can be reopened for exactly 7 × 24 hours after it was closed, independent of timezones.
pub const REOPEN_WINDOW: Duration = Duration::from_days(7);

/// Effort and impact scale. Ordered from smallest to largest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Size {
    S,
    M,
    L,
    Xl,
}

impl Size {
    pub const ALL: [Self; 4] = [Self::S, Self::M, Self::L, Self::Xl];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::S => "s",
            Self::M => "m",
            Self::L => "l",
            Self::Xl => "xl",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.code() == code)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Title(String);

impl Title {
    pub const MAX_CHARS: usize = 300;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Title, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Description(String);

impl Description {
    pub const MAX_CHARS: usize = 20_000;

    /// Blank input means "no description" and gives `None`. Surrounding whitespace is kept off.
    pub fn parse(raw: &str) -> Result<Option<Self>, ValidationError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.chars().count() > Self::MAX_CHARS {
            return Err(ValidationError::TooLong {
                field: Field::Description,
                max: Self::MAX_CHARS,
            });
        }
        Ok(Some(Self(trimmed.to_owned())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// When an item is due. Dates without a time are stored as the end of that day in the user's timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Due {
    pub at: Timestamp,
    /// `true` for an exact deadline, which can become overdue. `false` for a loose due time, which can't.
    pub firm: bool,
}

/// An item's state without its data. This is what clients send and what the database stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateKind {
    Open,
    InProgress,
    Blocked,
    Done,
    WontDo,
}

impl StateKind {
    pub const ALL: [Self; 5] = [Self::Open, Self::InProgress, Self::Blocked, Self::Done, Self::WontDo];

    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::WontDo)
    }

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::WontDo => "wont_do",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.code() == code)
    }
}

/// An item's state. Only the closed states carry a close time, so an open item with one can't exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Open,
    InProgress,
    Blocked,
    Done { closed_at: Timestamp },
    WontDo { closed_at: Timestamp },
}

/// A stored state and close time that don't fit together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("state {kind:?} doesn't match closed_at {closed_at:?}")]
pub struct InconsistentState {
    pub kind: StateKind,
    pub closed_at: Option<Timestamp>,
}

impl ItemState {
    /// Builds a state from its stored parts. `closed_at` must be set exactly when `kind` is closed.
    pub fn from_parts(kind: StateKind, closed_at: Option<Timestamp>) -> Result<Self, InconsistentState> {
        match (kind, closed_at) {
            (StateKind::Open, None) => Ok(Self::Open),
            (StateKind::InProgress, None) => Ok(Self::InProgress),
            (StateKind::Blocked, None) => Ok(Self::Blocked),
            (StateKind::Done, Some(closed_at)) => Ok(Self::Done { closed_at }),
            (StateKind::WontDo, Some(closed_at)) => Ok(Self::WontDo { closed_at }),
            _ => Err(InconsistentState { kind, closed_at }),
        }
    }

    pub const fn kind(self) -> StateKind {
        match self {
            Self::Open => StateKind::Open,
            Self::InProgress => StateKind::InProgress,
            Self::Blocked => StateKind::Blocked,
            Self::Done { .. } => StateKind::Done,
            Self::WontDo { .. } => StateKind::WontDo,
        }
    }

    /// `None` unless the state is closed.
    pub const fn closed_at(self) -> Option<Timestamp> {
        match self {
            Self::Done { closed_at } | Self::WontDo { closed_at } => Some(closed_at),
            Self::Open | Self::InProgress | Self::Blocked => None,
        }
    }
}

/// A state-machine rule an edit broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    #[error("item {item_id} is closed; reopen it before editing it")]
    Closed { item_id: ItemId },
    #[error("item {item_id} was closed at {closed_at}, more than a week ago, so it can't be reopened")]
    ReopenWindowExpired { item_id: ItemId, closed_at: Timestamp },
}

/// The fields of a new item. Everything except the title is optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    pub title: Title,
    pub description: Option<Description>,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    pub due: Option<Due>,
}

/// A partial edit. An outer `None` leaves the field unchanged; `Some(None)` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemPatch {
    pub title: Option<Title>,
    pub description: Option<Option<Description>>,
    pub effort: Option<Option<Size>>,
    pub impact: Option<Option<Size>>,
    pub due: Option<Option<Due>>,
    pub state: Option<StateKind>,
}

impl ItemPatch {
    /// Whether the patch changes anything besides the state.
    pub const fn edits_fields(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.effort.is_some()
            || self.impact.is_some()
            || self.due.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    pub heap_id: HeapId,
    pub title: Title,
    pub description: Option<Description>,
    pub state: ItemState,
    /// `None` when unset, which ranks as the largest effort.
    pub effort: Option<Size>,
    /// `None` when unset, which ranks as the smallest impact.
    pub impact: Option<Size>,
    pub due: Option<Due>,
    pub created_at: Timestamp,
    pub version: Version,
}

impl Item {
    /// A new open item created at `now`.
    pub fn new(id: ItemId, heap_id: HeapId, fields: NewItem, now: Timestamp) -> Self {
        Self {
            id,
            heap_id,
            title: fields.title,
            description: fields.description,
            state: ItemState::Open,
            effort: fields.effort,
            impact: fields.impact,
            due: fields.due,
            created_at: now,
            version: Version::INITIAL,
        }
    }

    pub const fn is_closed(&self) -> bool {
        self.state.kind().is_closed()
    }

    /// Whether a closed item is still inside the reopen window at `now`. Always `false` for open items.
    pub fn can_reopen(&self, now: Timestamp) -> bool {
        self.state
            .closed_at()
            .is_some_and(|closed_at| now.since(closed_at) <= REOPEN_WINDOW)
    }

    /// Applies `patch` at `now` and returns the edited item, or `None` when the patch changes nothing.
    ///
    /// The state changes first, so a single patch can reopen an item and edit it. Fields of an item that
    /// stays closed can't be edited. The returned item keeps the old version; the store bumps it on write.
    pub fn apply_patch(&self, patch: &ItemPatch, now: Timestamp) -> Result<Option<Self>, ItemError> {
        let state = match patch.state {
            Some(target) => self.transition(target, now)?,
            None => self.state,
        };
        if state.kind().is_closed() && patch.edits_fields() {
            return Err(ItemError::Closed { item_id: self.id });
        }

        let edited = Self {
            title: patch.title.clone().unwrap_or_else(|| self.title.clone()),
            description: patch.description.clone().unwrap_or_else(|| self.description.clone()),
            effort: patch.effort.unwrap_or(self.effort),
            impact: patch.impact.unwrap_or(self.impact),
            due: patch.due.unwrap_or(self.due),
            state,
            ..self.clone()
        };
        Ok((edited != *self).then_some(edited))
    }

    /// The state after moving to `target` at `now`. Moving to the current state changes nothing.
    fn transition(&self, target: StateKind, now: Timestamp) -> Result<ItemState, ItemError> {
        let current = self.state.kind();
        if current == target {
            return Ok(self.state);
        }
        if let Some(closed_at) = self.state.closed_at() {
            if target.is_closed() {
                return Err(ItemError::Closed { item_id: self.id });
            }
            if !self.can_reopen(now) {
                return Err(ItemError::ReopenWindowExpired {
                    item_id: self.id,
                    closed_at,
                });
            }
        }
        Ok(match target {
            StateKind::Open => ItemState::Open,
            StateKind::InProgress => ItemState::InProgress,
            StateKind::Blocked => ItemState::Blocked,
            StateKind::Done => ItemState::Done { closed_at: now },
            StateKind::WontDo => ItemState::WontDo { closed_at: now },
        })
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn at(days: i64) -> Timestamp {
        Timestamp::from_millis(Duration::from_days(days).as_millis()).expect("in range")
    }

    fn sample_item() -> Item {
        let fields = NewItem {
            title: Title::parse("Write tests").expect("valid"),
            description: None,
            effort: None,
            impact: None,
            due: None,
        };
        Item::new(
            ItemId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            at(0),
        )
    }

    fn closing_patch() -> ItemPatch {
        ItemPatch {
            state: Some(StateKind::Done),
            ..ItemPatch::default()
        }
    }

    #[gtest]
    fn closing_sets_closed_at() {
        let item = sample_item();

        let closed = item
            .apply_patch(&closing_patch(), at(2))
            .expect("allowed")
            .expect("changed");

        expect_that!(closed.state, eq(ItemState::Done { closed_at: at(2) }));
    }

    #[gtest]
    fn reopening_within_a_week_is_allowed() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        let reopened = closed
            .apply_patch(&reopen, at(8))
            .expect("inside window")
            .expect("changed");

        expect_that!(reopened.state, eq(ItemState::Open));
    }

    #[gtest]
    fn reopening_after_a_week_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };
        let just_after = at(8).checked_add(Duration::from_millis(1)).expect("in range");

        let result = closed.apply_patch(&reopen, just_after);

        expect_that!(
            result,
            err(eq(&ItemError::ReopenWindowExpired {
                item_id: closed.id,
                closed_at: at(1)
            }))
        );
    }

    #[gtest]
    fn editing_a_closed_item_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let rename = ItemPatch {
            title: Some(Title::parse("New").expect("valid")),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&rename, at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    #[gtest]
    fn reopen_and_edit_in_one_patch() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let patch = ItemPatch {
            state: Some(StateKind::InProgress),
            effort: Some(Some(Size::S)),
            ..ItemPatch::default()
        };

        let edited = closed.apply_patch(&patch, at(2)).expect("allowed").expect("changed");

        expect_that!(edited.state, eq(ItemState::InProgress));
        expect_that!(edited.effort, some(eq(Size::S)));
    }

    #[gtest]
    fn patch_without_changes_is_a_no_op() {
        let item = sample_item();
        let patch = ItemPatch {
            state: Some(StateKind::Open),
            effort: Some(None),
            ..ItemPatch::default()
        };

        expect_that!(item.apply_patch(&patch, at(1)), ok(none()));
    }

    #[gtest]
    fn inconsistent_stored_state_is_rejected() {
        expect_that!(ItemState::from_parts(StateKind::Open, Some(at(1))), err(anything()));
        expect_that!(ItemState::from_parts(StateKind::Done, None), err(anything()));
    }
}
