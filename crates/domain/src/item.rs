//! Items and their state machine.

use std::collections::{BTreeMap, BTreeSet};

use crate::handle::ItemKey;
use crate::text::{parse_optional, parse_required};
use crate::validation::{Field, ValidationError};
use crate::{Duration, HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, UserId, Version};

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

    /// The size as people read it: "S", "M", "L", "XL".
    pub const fn label(self) -> &'static str {
        match self {
            Self::S => "S",
            Self::M => "M",
            Self::L => "L",
            Self::Xl => "XL",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.code() == code)
    }
}

/// How much the item is needed: what happens if it never gets done. Ordered from least to most needed.
///
/// It is separate from impact, which says how much doing the item gains. Need says how much skipping it costs, so
/// a small fix someone is waiting for can outrank a bigger win nobody asked for. Each step is told apart from the
/// next by who is affected when the item is skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Need {
    /// Kept so it isn't forgotten. Nobody has decided it's wanted, and nothing is lost if it never happens.
    Idea,
    /// Wanted, but nobody is waiting for it. Skipping it only misses a gain.
    NiceToHave,
    /// Someone is counting on it. Skipping it lets them down.
    Expected,
    /// It has to happen. Skipping it does real damage: an outage, a lost customer, a broken commitment.
    Critical,
}

impl Need {
    pub const ALL: [Self; 4] = [Self::Idea, Self::NiceToHave, Self::Expected, Self::Critical];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Idea => "idea",
            Self::NiceToHave => "nice_to_have",
            Self::Expected => "expected",
            Self::Critical => "critical",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|need| need.code() == code)
    }

    /// The step's name as people read it, in contributors and audit entries.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idea => "Idea",
            Self::NiceToHave => "Nice to have",
            Self::Expected => "Expected",
            Self::Critical => "Critical",
        }
    }
}

/// What an item is. A note is kept so it isn't forgotten; a task is something to get done. A note becomes a task in
/// place, keeping its key, age and history, and a task never goes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Note,
    Task,
}

impl Kind {
    pub const ALL: [Self; 2] = [Self::Note, Self::Task];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Task => "task",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// Whether an item of this kind can be at `stage`. Only a task can be defined.
    pub const fn allows(self, stage: Stage) -> bool {
        match self {
            Self::Task => true,
            Self::Note => !matches!(stage, Stage::Defined),
        }
    }
}

/// How far an item has been prepared. Ordered from least to most prepared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Just as it was first put down.
    AsWritten,
    /// Background has been added, but what to do is still open.
    WithContext,
    /// Clear enough to start on. Only a task can be here.
    Defined,
}

impl Stage {
    pub const ALL: [Self; 3] = [Self::AsWritten, Self::WithContext, Self::Defined];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::AsWritten => "as_written",
            Self::WithContext => "with_context",
            Self::Defined => "defined",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|stage| stage.code() == code)
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
        parse_optional(raw, Field::Description, Self::MAX_CHARS).map(|text| text.map(Self))
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

    /// The states of an item that isn't closed, which are the ones that rank.
    pub const OPEN: [Self; 3] = [Self::Open, Self::InProgress, Self::Blocked];

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

/// An item's state. Only the closed states carry a close time and who closed it, and only the blocked state carries
/// a blocker, so an open item with any of them can't exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemState {
    Open,
    InProgress,
    /// `by` is the item it waits for, of the same heap, if one was picked.
    Blocked {
        by: Option<ItemId>,
    },
    /// `closed_by` is `None` only for an item closed before who closed it was recorded.
    Done {
        closed_at: Timestamp,
        closed_by: Option<UserId>,
    },
    /// As [`ItemState::Done`].
    WontDo {
        closed_at: Timestamp,
        closed_by: Option<UserId>,
    },
}

/// A stored state, close time, closer and blocker that don't fit together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("state {kind:?} doesn't match closed_at {closed_at:?}, closed_by {closed_by:?} and blocked_by {blocked_by:?}")]
pub struct InconsistentState {
    pub kind: StateKind,
    pub closed_at: Option<Timestamp>,
    pub closed_by: Option<UserId>,
    pub blocked_by: Option<ItemId>,
}

impl ItemState {
    /// Builds a state from its stored parts. `closed_at` must be set exactly when `kind` is closed, `closed_by`
    /// only when it's closed, and `blocked_by` only when it's blocked.
    pub fn from_parts(
        kind: StateKind,
        closed_at: Option<Timestamp>,
        closed_by: Option<UserId>,
        blocked_by: Option<ItemId>,
    ) -> Result<Self, InconsistentState> {
        match (kind, closed_at, closed_by, blocked_by) {
            (StateKind::Open, None, None, None) => Ok(Self::Open),
            (StateKind::InProgress, None, None, None) => Ok(Self::InProgress),
            (StateKind::Blocked, None, None, by) => Ok(Self::Blocked { by }),
            (StateKind::Done, Some(closed_at), closed_by, None) => Ok(Self::Done { closed_at, closed_by }),
            (StateKind::WontDo, Some(closed_at), closed_by, None) => Ok(Self::WontDo { closed_at, closed_by }),
            _ => Err(InconsistentState {
                kind,
                closed_at,
                closed_by,
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
            Self::Done { closed_at, .. } | Self::WontDo { closed_at, .. } => Some(closed_at),
            Self::Open | Self::InProgress | Self::Blocked { .. } => None,
        }
    }

    /// Who closed the item. `None` unless the state is closed, and for an item closed before this was recorded.
    pub const fn closed_by(self) -> Option<UserId> {
        match self {
            Self::Done { closed_by, .. } | Self::WontDo { closed_by, .. } => closed_by,
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
    /// A task never goes back to a note.
    #[error("item {item_id} is a {from} and can't become a {to}", from = .from.code(), to = .to.code())]
    KindChangeNotAllowed { item_id: ItemId, from: Kind, to: Kind },
    /// Only a task can be defined.
    #[error("item {item_id} is a note, so it can't be defined; make it a task first")]
    StageNeedsTask { item_id: ItemId },
}

/// The fields of a new item. Everything except the title and the need is optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    pub title: Title,
    pub description: Option<Description>,
    pub need: Need,
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
    /// Need can be changed but never cleared, so it has no inner `Option`.
    pub need: Option<Need>,
    /// A note can become a task; the reverse is refused. Applied before `stage`.
    pub kind: Option<Kind>,
    pub stage: Option<Stage>,
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
            || self.need.is_some()
            || self.kind.is_some()
            || self.stage.is_some()
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
    pub key: ItemKey,
    pub title: Title,
    pub state: ItemState,
}

impl From<&Item> for ItemSummary {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id,
            key: item.key,
            title: item.title.clone(),
            state: item.state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    pub heap_id: HeapId,
    /// Unique within the heap.
    pub key: ItemKey,
    pub title: Title,
    pub description: Option<Description>,
    pub state: ItemState,
    pub kind: Kind,
    /// Set by hand or, later, when an agent's turn reaches a stable state; never worked out from the item's
    /// contents. It is shown only while the item is open.
    pub stage: Stage,
    pub need: Need,
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
    /// Who added it. `None` only for an item added before creators were stored whose `item_created` audit entry
    /// couldn't be found to fill it in.
    pub created_by: Option<UserId>,
    pub version: Version,
}

impl Item {
    /// A new open item added by `created_by` at `now`.
    pub fn new(id: ItemId, heap_id: HeapId, key: ItemKey, fields: NewItem, created_by: UserId, now: Timestamp) -> Self {
        Self {
            id,
            heap_id,
            key,
            title: fields.title,
            description: fields.description,
            state: ItemState::Open,
            kind: Kind::Note,
            stage: Stage::AsWritten,
            need: fields.need,
            effort: fields.effort,
            impact: fields.impact,
            due: fields.due,
            tags: fields.tags,
            parameters: fields.parameters,
            created_at: now,
            created_by: Some(created_by),
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

    /// The stages that can be set on the item now, least prepared first. None when it is closed; a note can't be
    /// defined.
    pub fn stage_choices(&self) -> Vec<Stage> {
        if self.is_closed() {
            return Vec::new();
        }
        Stage::ALL
            .into_iter()
            .filter(|stage| self.kind.allows(*stage))
            .collect()
    }

    /// The item this one waits for, if it's blocked and one was picked.
    pub const fn blocked_by(&self) -> Option<ItemId> {
        self.state.blocked_by()
    }

    /// Applies `patch`, made by `actor` at `now`, and returns the edited item, or `None` when the patch changes
    /// nothing. Closing the item records `actor` as who closed it.
    ///
    /// The state changes first, so a single patch can reopen an item and edit it, or block it and pick the
    /// blocker. The kind changes before the stage, so one patch can make a note a task and define it. Fields of an
    /// item that stays closed can't be edited. The returned item keeps the old version; the store bumps it on
    /// write.
    pub fn apply_patch(&self, patch: &ItemPatch, actor: UserId, now: Timestamp) -> Result<Option<Self>, ItemError> {
        let mut state = match patch.state {
            Some(target) => self.transition(target, actor, now)?,
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

        let kind = match (self.kind, patch.kind) {
            (_, None) => self.kind,
            (Kind::Task, Some(Kind::Note)) => {
                return Err(ItemError::KindChangeNotAllowed {
                    item_id: self.id,
                    from: Kind::Task,
                    to: Kind::Note,
                });
            }
            (_, Some(target)) => target,
        };
        let stage = patch.stage.unwrap_or(self.stage);
        if !kind.allows(stage) {
            return Err(ItemError::StageNeedsTask { item_id: self.id });
        }

        let edited = Self {
            kind,
            stage,
            title: patch.title.clone().unwrap_or_else(|| self.title.clone()),
            description: patch.description.clone().unwrap_or_else(|| self.description.clone()),
            need: patch.need.unwrap_or(self.need),
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

    /// The item as it is after it moves to `heap_id` under `key`, carrying `tags` and `parameters` of that heap (see
    /// [`crate::item_move::carry_over`]). Everything else stays, and so does the version, which the store bumps on
    /// write. An item that waits for another item still waits, with nobody picked: its blocker is in the heap it left.
    pub fn moved_to(
        &self,
        heap_id: HeapId,
        key: ItemKey,
        tags: BTreeSet<TagId>,
        parameters: BTreeMap<ParameterId, OptionId>,
    ) -> Self {
        let state = match self.state {
            ItemState::Blocked { .. } => ItemState::Blocked { by: None },
            other => other,
        };
        Self {
            heap_id,
            key,
            tags,
            parameters,
            state,
            ..self.clone()
        }
    }

    /// The item with its blocker taken away: still blocked, waiting for nobody in particular. An item that isn't
    /// blocked is returned as it is.
    pub fn without_blocker(&self) -> Self {
        let state = match self.state {
            ItemState::Blocked { .. } => ItemState::Blocked { by: None },
            other => other,
        };
        Self { state, ..self.clone() }
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

    /// The state after `actor` moves it to `target` at `now`. Moving to the current state changes nothing.
    fn transition(&self, target: StateKind, actor: UserId, now: Timestamp) -> Result<ItemState, ItemError> {
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
            StateKind::Done => ItemState::Done {
                closed_at: now,
                closed_by: Some(actor),
            },
            StateKind::WontDo => ItemState::WontDo {
                closed_at: now,
                closed_by: Some(actor),
            },
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
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
        };
        Item::new(
            ItemId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            crate::handle::ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::nil()),
            at(0),
        )
    }

    /// Who makes the edits in these tests.
    fn actor() -> UserId {
        UserId::from_uuid(Uuid::from_u128(42))
    }

    fn closing_patch() -> ItemPatch {
        ItemPatch {
            state: Some(StateKind::Done),
            ..ItemPatch::default()
        }
    }

    #[gtest]
    fn closing_records_when_and_by_whom() {
        let item = sample_item();

        let closed = item
            .apply_patch(&closing_patch(), actor(), at(2))
            .expect("allowed")
            .expect("changed");

        expect_that!(
            closed.state,
            eq(ItemState::Done {
                closed_at: at(2),
                closed_by: Some(actor())
            })
        );
        expect_that!(closed.state.closed_by(), some(eq(actor())));
    }

    #[gtest]
    fn reopening_forgets_who_closed_it() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::InProgress),
            ..ItemPatch::default()
        };

        let reopened = closed
            .apply_patch(&reopen, UserId::from_uuid(Uuid::from_u128(43)), at(2))
            .expect("inside window")
            .expect("changed");

        expect_that!(reopened.state.closed_by(), none());
        expect_that!(reopened.state.closed_at(), none());
    }

    #[gtest]
    fn reopening_within_a_week_is_allowed() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        let reopened = closed
            .apply_patch(&reopen, actor(), at(8))
            .expect("inside window")
            .expect("changed");

        expect_that!(reopened.state, eq(ItemState::Open));
    }

    #[gtest]
    fn reopening_after_a_week_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };
        let just_after = at(8).checked_add(Duration::from_millis(1)).expect("in range");

        let result = closed.apply_patch(&reopen, actor(), just_after);

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
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let rename = ItemPatch {
            title: Some(Title::parse("New").expect("valid")),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&rename, actor(), at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    #[gtest]
    fn retagging_a_closed_item_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let retag = ItemPatch {
            tags: Some(BTreeSet::from([TagId::from_uuid(Uuid::nil())])),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&retag, actor(), at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    #[gtest]
    fn reopen_and_edit_in_one_patch() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let patch = ItemPatch {
            state: Some(StateKind::InProgress),
            effort: Some(Some(Size::S)),
            ..ItemPatch::default()
        };

        let edited = closed
            .apply_patch(&patch, actor(), at(2))
            .expect("allowed")
            .expect("changed");

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

        expect_that!(item.apply_patch(&patch, actor(), at(1)), ok(none()));
    }

    #[gtest]
    fn a_patch_changes_the_need_but_not_of_a_closed_item() {
        let patch = ItemPatch {
            need: Some(Need::Critical),
            ..ItemPatch::default()
        };
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");

        let edited = sample_item()
            .apply_patch(&patch, actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(edited.need, eq(Need::Critical));
        expect_that!(
            closed.apply_patch(&patch, actor(), at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    #[gtest]
    fn need_codes_round_trip() {
        for need in Need::ALL {
            expect_that!(Need::from_code(need.code()), some(eq(need)));
        }
    }

    #[gtest]
    fn inconsistent_stored_state_is_rejected() {
        let other = ItemId::from_uuid(Uuid::from_u128(1));
        expect_that!(
            ItemState::from_parts(StateKind::Open, Some(at(1)), None, None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Done, None, None, None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Open, None, None, Some(other)),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Blocked, None, None, Some(other)),
            ok(eq(ItemState::Blocked { by: Some(other) }))
        );
    }

    #[gtest]
    fn only_a_closed_stored_state_has_a_closer() {
        for kind in [StateKind::Open, StateKind::InProgress, StateKind::Blocked] {
            expect_that!(
                ItemState::from_parts(kind, None, Some(actor()), None),
                err(anything()),
                "{kind:?}"
            );
        }
        expect_that!(
            ItemState::from_parts(StateKind::Done, None, Some(actor()), None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::WontDo, Some(at(1)), Some(actor()), None),
            ok(eq(ItemState::WontDo {
                closed_at: at(1),
                closed_by: Some(actor())
            }))
        );
        // Items closed before closers were recorded.
        expect_that!(
            ItemState::from_parts(StateKind::Done, Some(at(1)), None, None),
            ok(eq(ItemState::Done {
                closed_at: at(1),
                closed_by: None
            }))
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

        let edited = item
            .apply_patch(&patch, actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(edited.parameters, eq(&BTreeMap::from([(area, infra)])));
    }

    #[gtest]
    fn setting_a_value_of_a_closed_item_fails() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");
        let patch = ItemPatch {
            parameters: BTreeMap::from([(ParameterId::from_uuid(Uuid::nil()), None)]),
            ..ItemPatch::default()
        };

        expect_that!(
            closed.apply_patch(&patch, actor(), at(2)),
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
            .apply_patch(&patch, actor(), at(1))
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
            item.apply_patch(&patch, actor(), at(1)),
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
        let blocked = sample_item()
            .apply_patch(&block, actor(), at(1))
            .expect("ok")
            .expect("changed");
        let unblock = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        let open = blocked
            .apply_patch(&unblock, actor(), at(2))
            .expect("allowed")
            .expect("changed");

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
        let blocked = sample_item()
            .apply_patch(&block, actor(), at(1))
            .expect("ok")
            .expect("changed");
        let retitle = ItemPatch {
            state: Some(StateKind::Blocked),
            title: Some(Title::parse("Renamed").expect("valid")),
            ..ItemPatch::default()
        };

        let edited = blocked
            .apply_patch(&retitle, actor(), at(2))
            .expect("allowed")
            .expect("changed");

        expect_that!(edited.blocked_by(), some(eq(blocker())));
    }

    #[gtest]
    fn a_new_item_is_a_note_as_written() {
        let item = sample_item();

        expect_that!(item.kind, eq(Kind::Note));
        expect_that!(item.stage, eq(Stage::AsWritten));
    }

    fn make_task_patch() -> ItemPatch {
        ItemPatch {
            kind: Some(Kind::Task),
            ..ItemPatch::default()
        }
    }

    #[gtest]
    fn a_note_becomes_a_task_in_place() {
        let mut note = sample_item();
        note.stage = Stage::WithContext;

        let task = note
            .apply_patch(&make_task_patch(), actor(), at(3))
            .expect("allowed")
            .expect("changed");

        expect_that!(task.kind, eq(Kind::Task));
        expect_that!(task.stage, eq(Stage::WithContext));
        expect_that!(task.id, eq(note.id));
        expect_that!(task.key, eq(note.key));
        expect_that!(task.created_at, eq(note.created_at));
        expect_that!(task.need, eq(note.need));
    }

    fn sample_task() -> Item {
        sample_item()
            .apply_patch(&make_task_patch(), actor(), at(1))
            .expect("allowed")
            .expect("changed")
    }

    #[gtest]
    fn making_a_task_a_task_or_a_note_a_note_changes_nothing() {
        let note = ItemPatch {
            kind: Some(Kind::Note),
            ..ItemPatch::default()
        };

        expect_that!(
            sample_task().apply_patch(&make_task_patch(), actor(), at(2)),
            ok(none())
        );
        expect_that!(sample_item().apply_patch(&note, actor(), at(2)), ok(none()));
    }

    #[gtest]
    fn a_task_never_goes_back_to_a_note() {
        let task = sample_task();
        let patch = ItemPatch {
            kind: Some(Kind::Note),
            ..ItemPatch::default()
        };

        expect_that!(
            task.apply_patch(&patch, actor(), at(2)),
            err(eq(&ItemError::KindChangeNotAllowed {
                item_id: task.id,
                from: Kind::Task,
                to: Kind::Note
            }))
        );
    }

    #[gtest]
    fn the_kind_of_a_closed_item_cant_change() {
        let closed = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");

        expect_that!(
            closed.apply_patch(&make_task_patch(), actor(), at(2)),
            err(eq(&ItemError::Closed { item_id: closed.id }))
        );
    }

    #[gtest]
    fn a_note_cant_be_defined_but_a_task_can() {
        let define = ItemPatch {
            stage: Some(Stage::Defined),
            ..ItemPatch::default()
        };
        let note = sample_item();

        expect_that!(
            note.apply_patch(&define, actor(), at(2)),
            err(eq(&ItemError::StageNeedsTask { item_id: note.id }))
        );
        let defined = sample_task()
            .apply_patch(&define, actor(), at(2))
            .expect("allowed")
            .expect("changed");
        expect_that!(defined.stage, eq(Stage::Defined));
    }

    #[gtest]
    fn one_patch_makes_a_note_a_defined_task() {
        let patch = ItemPatch {
            kind: Some(Kind::Task),
            stage: Some(Stage::Defined),
            ..ItemPatch::default()
        };

        let task = sample_item()
            .apply_patch(&patch, actor(), at(2))
            .expect("allowed")
            .expect("changed");

        expect_that!((task.kind, task.stage), eq((Kind::Task, Stage::Defined)));
    }

    #[gtest]
    fn setting_the_stage_it_has_changes_nothing() {
        let patch = ItemPatch {
            stage: Some(Stage::AsWritten),
            ..ItemPatch::default()
        };

        expect_that!(sample_item().apply_patch(&patch, actor(), at(2)), ok(none()));
    }

    #[gtest]
    fn reopening_keeps_the_kind_and_stage() {
        let define = ItemPatch {
            stage: Some(Stage::Defined),
            ..ItemPatch::default()
        };
        let closed = sample_task()
            .apply_patch(&define, actor(), at(1))
            .expect("ok")
            .expect("changed")
            .apply_patch(&closing_patch(), actor(), at(2))
            .expect("ok")
            .expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        let reopened = closed
            .apply_patch(&reopen, actor(), at(3))
            .expect("inside window")
            .expect("changed");

        expect_that!((reopened.kind, reopened.stage), eq((Kind::Task, Stage::Defined)));
    }

    #[gtest]
    fn kind_and_stage_codes_round_trip() {
        for kind in Kind::ALL {
            expect_that!(Kind::from_code(kind.code()), some(eq(kind)));
        }
        for stage in Stage::ALL {
            expect_that!(Stage::from_code(stage.code()), some(eq(stage)));
        }
    }

    #[gtest]
    fn stage_choices_depend_on_the_kind_and_are_empty_when_closed() {
        let closed = sample_task()
            .apply_patch(&closing_patch(), actor(), at(2))
            .expect("ok")
            .expect("changed");

        expect_that!(
            sample_item().stage_choices(),
            eq(&vec![Stage::AsWritten, Stage::WithContext])
        );
        expect_that!(sample_task().stage_choices(), eq(&Stage::ALL.to_vec()));
        expect_that!(closed.stage_choices(), is_empty());
    }
}
