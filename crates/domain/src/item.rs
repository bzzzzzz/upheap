//! Items and their state machine.

use std::collections::{BTreeMap, BTreeSet};

use crate::text::parse_required;
use crate::validation::{Field, ValidationError};
use crate::{Duration, HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, Version};

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

/// An item's state. Only the closed states carry a close time, and only the blocked state carries a blocker, so
/// an open item with either can't exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Open,
    InProgress,
    /// `by` is the item it waits for, of the same heap, if one was picked.
    Blocked {
        by: Option<ItemId>,
    },
    Done {
        closed_at: Timestamp,
    },
    WontDo {
        closed_at: Timestamp,
    },
}

/// A stored state, close time and blocker that don't fit together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("state {kind:?} doesn't match closed_at {closed_at:?} and blocked_by {blocked_by:?}")]
pub struct InconsistentState {
    pub kind: StateKind,
    pub closed_at: Option<Timestamp>,
    pub blocked_by: Option<ItemId>,
}

impl ItemState {
    /// Builds a state from its stored parts. `closed_at` must be set exactly when `kind` is closed, and
    /// `blocked_by` only when it's blocked.
    pub fn from_parts(
        kind: StateKind,
        closed_at: Option<Timestamp>,
        blocked_by: Option<ItemId>,
    ) -> Result<Self, InconsistentState> {
        match (kind, closed_at, blocked_by) {
            (StateKind::Open, None, None) => Ok(Self::Open),
            (StateKind::InProgress, None, None) => Ok(Self::InProgress),
            (StateKind::Blocked, None, by) => Ok(Self::Blocked { by }),
            (StateKind::Done, Some(closed_at), None) => Ok(Self::Done { closed_at }),
            (StateKind::WontDo, Some(closed_at), None) => Ok(Self::WontDo { closed_at }),
            _ => Err(InconsistentState {
                kind,
                closed_at,
                blocked_by,
            }),
        }
    }

    pub const fn kind(self) -> StateKind {
        match self {
            Self::Open => StateKind::Open,
            Self::InProgress => StateKind::InProgress,
            Self::Blocked { .. } => StateKind::Blocked,
            Self::Done { .. } => StateKind::Done,
            Self::WontDo { .. } => StateKind::WontDo,
        }
    }

    /// `None` unless the state is closed.
    pub const fn closed_at(self) -> Option<Timestamp> {
        match self {
            Self::Done { closed_at } | Self::WontDo { closed_at } => Some(closed_at),
            Self::Open | Self::InProgress | Self::Blocked { .. } => None,
        }
    }

    /// The item a blocked item waits for. `None` for every other state.
    pub const fn blocked_by(self) -> Option<ItemId> {
        match self {
            Self::Blocked { by } => by,
            Self::Open | Self::InProgress | Self::Done { .. } | Self::WontDo { .. } => None,
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
    /// Only a blocked item can point to a blocker.
    #[error("item {item_id} isn't blocked, so it can't have a blocker")]
    NotBlocked { item_id: ItemId },
}

/// The fields of a new item. Everything except the title is optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    pub title: Title,
    pub description: Option<Description>,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    pub due: Option<Due>,
    /// Tags of the item's heap. The service checks they belong to it.
    pub tags: BTreeSet<TagId>,
    /// An option for each custom parameter that is set. The service checks they are in use in the heap.
    pub parameters: BTreeMap<ParameterId, OptionId>,
}

/// A partial edit. An outer `None` leaves the field unchanged; `Some(None)` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemPatch {
    pub title: Option<Title>,
    pub description: Option<Option<Description>>,
    pub effort: Option<Option<Size>>,
    pub impact: Option<Option<Size>>,
    pub due: Option<Option<Due>>,
    /// Replaces the whole set of tags.
    pub tags: Option<BTreeSet<TagId>>,
    /// Sets (`Some`) or clears (`None`) the value of each listed custom parameter; others keep theirs.
    pub parameters: BTreeMap<ParameterId, Option<OptionId>>,
    pub state: Option<StateKind>,
    /// The item this one waits for. Only a blocked item can have one; leaving the blocked state clears it. The
    /// service checks the blocker is in the same heap and that no cycle forms.
    pub blocked_by: Option<Option<ItemId>>,
}

impl ItemPatch {
    /// Whether the patch changes anything besides the state.
    pub fn edits_fields(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.effort.is_some()
            || self.impact.is_some()
            || self.due.is_some()
            || self.tags.is_some()
            || !self.parameters.is_empty()
    }
}

/// Enough of an item to name it where another item refers to it, as a blocker does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSummary {
    pub id: ItemId,
    pub title: Title,
    pub state: ItemState,
}

impl From<&Item> for ItemSummary {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            state: item.state,
        }
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
    pub tags: BTreeSet<TagId>,
    /// An option for each custom parameter that is set. Unset parameters are left out, and rank as their
    /// lowest-priority option. A closed item may keep an option or a parameter that was archived since; reopening it
    /// brings them up to date (`parameter::current_values`).
    pub parameters: BTreeMap<ParameterId, OptionId>,
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
            tags: fields.tags,
            parameters: fields.parameters,
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

    /// The item this one waits for, if it's blocked and one was picked.
    pub const fn blocked_by(&self) -> Option<ItemId> {
        self.state.blocked_by()
    }

    /// Applies `patch` at `now` and returns the edited item, or `None` when the patch changes nothing.
    ///
    /// The state changes first, so a single patch can reopen an item and edit it, or block it and pick the
    /// blocker. Fields of an item that stays closed can't be edited. The returned item keeps the old version; the
    /// store bumps it on write.
    pub fn apply_patch(&self, patch: &ItemPatch, now: Timestamp) -> Result<Option<Self>, ItemError> {
        let mut state = match patch.state {
            Some(target) => self.transition(target, now)?,
            None => self.state,
        };
        if state.kind().is_closed() && patch.edits_fields() {
            return Err(ItemError::Closed { item_id: self.id });
        }
        match (&mut state, patch.blocked_by) {
            (_, None) => {}
            (ItemState::Blocked { by }, Some(blocker)) => *by = blocker,
            // Clearing the blocker of an item that has none is a no-op, whatever its state.
            (_, Some(None)) => {}
            (_, Some(Some(_))) => return Err(ItemError::NotBlocked { item_id: self.id }),
        }

        let edited = Self {
            title: patch.title.clone().unwrap_or_else(|| self.title.clone()),
            description: patch.description.clone().unwrap_or_else(|| self.description.clone()),
            effort: patch.effort.unwrap_or(self.effort),
            impact: patch.impact.unwrap_or(self.impact),
            due: patch.due.unwrap_or(self.due),
            tags: patch.tags.clone().unwrap_or_else(|| self.tags.clone()),
            parameters: self.with_values(&patch.parameters),
            state,
            ..self.clone()
        };
        Ok((edited != *self).then_some(edited))
    }

    /// The item's values with `changes` applied.
    fn with_values(&self, changes: &BTreeMap<ParameterId, Option<OptionId>>) -> BTreeMap<ParameterId, OptionId> {
        let mut values = self.parameters.clone();
        for (&parameter, &option) in changes {
            match option {
                Some(option) => values.insert(parameter, option),
                None => values.remove(&parameter),
            };
        }
        values
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
            StateKind::Blocked => ItemState::Blocked { by: None },
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
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
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
    fn retagging_a_closed_item_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let retag = ItemPatch {
            tags: Some(BTreeSet::from([TagId::from_uuid(Uuid::nil())])),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&retag, at(2)),
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
        let other = ItemId::from_uuid(Uuid::from_u128(1));
        expect_that!(
            ItemState::from_parts(StateKind::Open, Some(at(1)), None),
            err(anything())
        );
        expect_that!(ItemState::from_parts(StateKind::Done, None, None), err(anything()));
        expect_that!(
            ItemState::from_parts(StateKind::Open, None, Some(other)),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Blocked, None, Some(other)),
            ok(eq(ItemState::Blocked { by: Some(other) }))
        );
    }

    #[gtest]
    fn a_patch_sets_and_clears_parameter_values_and_keeps_the_others() {
        let (customer, area) = (
            ParameterId::from_uuid(Uuid::from_u128(1)),
            ParameterId::from_uuid(Uuid::from_u128(2)),
        );
        let (pro, infra) = (
            OptionId::from_uuid(Uuid::from_u128(11)),
            OptionId::from_uuid(Uuid::from_u128(21)),
        );
        let mut item = sample_item();
        item.parameters = BTreeMap::from([(customer, pro)]);
        let patch = ItemPatch {
            parameters: BTreeMap::from([(customer, None), (area, Some(infra))]),
            ..ItemPatch::default()
        };

        let edited = item.apply_patch(&patch, at(1)).expect("allowed").expect("changed");

        expect_that!(edited.parameters, eq(&BTreeMap::from([(area, infra)])));
    }

    #[gtest]
    fn setting_a_value_of_a_closed_item_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), at(1))
            .expect("ok")
            .expect("changed");
        let patch = ItemPatch {
            parameters: BTreeMap::from([(ParameterId::from_uuid(Uuid::nil()), None)]),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&patch, at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    fn blocker() -> ItemId {
        ItemId::from_uuid(Uuid::from_u128(7))
    }

    #[gtest]
    fn blocking_and_picking_the_blocker_in_one_patch() {
        let patch = ItemPatch {
            state: Some(StateKind::Blocked),
            blocked_by: Some(Some(blocker())),
            ..ItemPatch::default()
        };

        let blocked = sample_item()
            .apply_patch(&patch, at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(blocked.blocked_by(), some(eq(blocker())));
    }

    #[gtest]
    fn an_item_that_isnt_blocked_cant_have_a_blocker() {
        let item = sample_item();
        let patch = ItemPatch {
            blocked_by: Some(Some(blocker())),
            ..ItemPatch::default()
        };

        expect_that!(
            item.apply_patch(&patch, at(1)),
            err(eq(&ItemError::NotBlocked { item_id: item.id }))
        );
    }

    #[gtest]
    fn leaving_the_blocked_state_drops_the_blocker() {
        let block = ItemPatch {
            state: Some(StateKind::Blocked),
            blocked_by: Some(Some(blocker())),
            ..ItemPatch::default()
        };
        let blocked = sample_item().apply_patch(&block, at(1)).expect("ok").expect("changed");
        let unblock = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        let open = blocked.apply_patch(&unblock, at(2)).expect("allowed").expect("changed");

        expect_that!(open.state, eq(ItemState::Open));
        expect_that!(open.blocked_by(), none());
    }

    #[gtest]
    fn staying_blocked_keeps_the_blocker() {
        let block = ItemPatch {
            state: Some(StateKind::Blocked),
            blocked_by: Some(Some(blocker())),
            ..ItemPatch::default()
        };
        let blocked = sample_item().apply_patch(&block, at(1)).expect("ok").expect("changed");
        let retitle = ItemPatch {
            state: Some(StateKind::Blocked),
            title: Some(Title::parse("Renamed").expect("valid")),
            ..ItemPatch::default()
        };

        let edited = blocked.apply_patch(&retitle, at(2)).expect("allowed").expect("changed");

        expect_that!(edited.blocked_by(), some(eq(blocker())));
    }
}
