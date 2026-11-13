//! Items and their state machine.

use std::collections::{BTreeMap, BTreeSet};

use crate::handle::ItemKey;
use crate::text::{parse_optional, parse_required};
use crate::validation::{Field, ValidationError};
use crate::{Duration, HeapId, ItemId, OptionId, ParameterId, RunId, TagId, Timestamp, UserId, Version};

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

/// What an item is. A note is kept so it isn't forgotten; a task is something to get done; a reminder has a date
/// and joins the ranked list when it comes. A note or a reminder becomes a task in place, keeping its key, age and
/// history, and a task never goes back. No other kind changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Note,
    Task,
    Reminder,
}

impl Kind {
    pub const ALL: [Self; 3] = [Self::Note, Self::Task, Self::Reminder];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Task => "task",
            Self::Reminder => "reminder",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// Whether an item of this kind can be at `stage`. Only a task can be defined.
    pub const fn allows(self, stage: Stage) -> bool {
        match self {
            Self::Task => true,
            Self::Note | Self::Reminder => !matches!(stage, Stage::Defined),
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

/// Why an item is blocked, in words: what it waits for outside the heap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockReason(String);

impl BlockReason {
    pub const MAX_CHARS: usize = 300;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::BlockedReason, Self::MAX_CHARS).map(Self)
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
    /// Closed because it was split: the items made from it carry on (Rules, Kinds and moves).
    Became,
}

impl StateKind {
    pub const ALL: [Self; 6] = [
        Self::Open,
        Self::InProgress,
        Self::Blocked,
        Self::Done,
        Self::WontDo,
        Self::Became,
    ];

    /// The states of an item that isn't closed, which are the ones that rank.
    pub const OPEN: [Self; 3] = [Self::Open, Self::InProgress, Self::Blocked];

    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Done | Self::WontDo | Self::Became)
    }

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::WontDo => "wont_do",
            Self::Became => "became",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.code() == code)
    }
}

/// An item's state. Only the closed states carry a close time and who closed it, and only the blocked state carries
/// a blocker and a reason, so an open item with any of them can't exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemState {
    Open,
    InProgress,
    /// `by` is the item it waits for, of the same heap, if one was picked.
    ///
    /// `reason` is why it waits, in words, when what it waits for isn't an item of the heap.
    Blocked {
        by: Option<ItemId>,
        reason: Option<BlockReason>,
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
    /// As [`ItemState::Done`]. Only a note becomes, once something was made from it.
    Became {
        closed_at: Timestamp,
        closed_by: Option<UserId>,
    },
}

/// A stored state, close time, closer and blocker that don't fit together.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "state {kind:?} doesn't match closed_at {closed_at:?}, closed_by {closed_by:?}, blocked_by {blocked_by:?} and blocked_reason {blocked_reason:?}"
)]
pub struct InconsistentState {
    pub kind: StateKind,
    pub closed_at: Option<Timestamp>,
    pub closed_by: Option<UserId>,
    pub blocked_by: Option<ItemId>,
    pub blocked_reason: Option<BlockReason>,
}

impl ItemState {
    /// Builds a state from its stored parts. `closed_at` must be set exactly when `kind` is closed, `closed_by`
    /// only when it's closed, and `blocked_by` and `blocked_reason` only when it's blocked.
    pub fn from_parts(
        kind: StateKind,
        closed_at: Option<Timestamp>,
        closed_by: Option<UserId>,
        blocked_by: Option<ItemId>,
        blocked_reason: Option<BlockReason>,
    ) -> Result<Self, InconsistentState> {
        match (kind, closed_at, closed_by, blocked_by, blocked_reason) {
            (StateKind::Open, None, None, None, None) => Ok(Self::Open),
            (StateKind::InProgress, None, None, None, None) => Ok(Self::InProgress),
            (StateKind::Blocked, None, None, by, reason) => Ok(Self::Blocked { by, reason }),
            (StateKind::Done, Some(closed_at), closed_by, None, None) => Ok(Self::Done { closed_at, closed_by }),
            (StateKind::WontDo, Some(closed_at), closed_by, None, None) => Ok(Self::WontDo { closed_at, closed_by }),
            (StateKind::Became, Some(closed_at), closed_by, None, None) => Ok(Self::Became { closed_at, closed_by }),
            (kind, closed_at, closed_by, blocked_by, blocked_reason) => Err(InconsistentState {
                kind,
                closed_at,
                closed_by,
                blocked_by,
                blocked_reason,
            }),
        }
    }

    pub const fn kind(&self) -> StateKind {
        match self {
            Self::Open => StateKind::Open,
            Self::InProgress => StateKind::InProgress,
            Self::Blocked { .. } => StateKind::Blocked,
            Self::Done { .. } => StateKind::Done,
            Self::WontDo { .. } => StateKind::WontDo,
            Self::Became { .. } => StateKind::Became,
        }
    }

    /// `None` unless the state is closed.
    pub const fn closed_at(&self) -> Option<Timestamp> {
        match self {
            Self::Done { closed_at, .. } | Self::WontDo { closed_at, .. } | Self::Became { closed_at, .. } => {
                Some(*closed_at)
            }
            Self::Open | Self::InProgress | Self::Blocked { .. } => None,
        }
    }

    /// Who closed the item. `None` unless the state is closed, and for an item closed before this was recorded.
    pub const fn closed_by(&self) -> Option<UserId> {
        match self {
            Self::Done { closed_by, .. } | Self::WontDo { closed_by, .. } | Self::Became { closed_by, .. } => {
                *closed_by
            }
            Self::Open | Self::InProgress | Self::Blocked { .. } => None,
        }
    }

    /// The item a blocked item waits for. `None` for every other state.
    pub const fn blocked_by(&self) -> Option<ItemId> {
        match self {
            Self::Blocked { by, .. } => *by,
            Self::Open | Self::InProgress | Self::Done { .. } | Self::WontDo { .. } | Self::Became { .. } => None,
        }
    }

    /// Why a blocked item is blocked, in words. `None` for every other state, and for a blocked item without one.
    pub const fn blocked_reason(&self) -> Option<&BlockReason> {
        match self {
            Self::Blocked { reason, .. } => reason.as_ref(),
            Self::Open | Self::InProgress | Self::Done { .. } | Self::WontDo { .. } | Self::Became { .. } => None,
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
    /// A reminder's due is its date, so it can't be cleared.
    #[error("item {item_id} is a reminder, so it needs a date")]
    ReminderNeedsDate { item_id: ItemId },
    /// A note becomes other items only through the became move, once something was made from it.
    #[error("item {item_id} can only become other items through the became move, not by changing its state")]
    BecameNeedsSplit { item_id: ItemId },
    /// Only a note can become other items.
    #[error("item {item_id} is a {kind}, and only a note can become other items", kind = .kind.code())]
    BecameNeedsNote { item_id: ItemId, kind: Kind },
}

/// Whose age a new item made from another counts from: nothing is inherited silently, so a split says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Age {
    /// The origin's: ranking counts the new item's age from when the origin joined the list.
    Keep,
    /// None: ranking counts it from now.
    New,
}

impl Age {
    /// The `arrives_at` of an item made from `origin` with this age: the origin's `arrives_at`, or its creation when
    /// it has none, for [`Age::Keep`]; `None` for [`Age::New`], so its age counts from its own creation.
    pub fn arrives_at(self, origin: &Item) -> Option<Timestamp> {
        match self {
            Self::Keep => Some(origin.arrives_at.unwrap_or(origin.created_at)),
            Self::New => None,
        }
    }
}

/// The item a new item is made from, and which age it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewOrigin {
    pub item_id: ItemId,
    pub age: Age,
}

/// The fields of a new item. Everything except the title and the need is optional.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    pub title: Title,
    pub description: Option<Description>,
    /// A new item is always as written.
    pub kind: Kind,
    pub need: Need,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    /// A reminder has one, firm: its date. The caller makes it firm.
    pub due: Option<Due>,
    /// The first millisecond of a reminder's date in the request's time zone; `None` for the other kinds.
    pub arrives_at: Option<Timestamp>,
    /// Tags of the item's heap. The service checks they belong to it.
    pub tags: BTreeSet<TagId>,
    /// An option for each custom parameter that is set. The service checks they are in use in the heap.
    pub parameters: BTreeMap<ParameterId, OptionId>,
    /// The item it is made from. The service checks it is an open note of the same heap, and sets `arrives_at` from
    /// the age.
    pub origin: Option<NewOrigin>,
    /// The item the new one starts blocked by. The service checks it with the other blockers.
    pub blocked_by: Option<ItemId>,
}

/// A partial edit. An outer `None` leaves the field unchanged; `Some(None)` clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemPatch {
    pub title: Option<Title>,
    pub description: Option<Option<Description>>,
    /// Need can be changed but never cleared, so it has no inner `Option`.
    pub need: Option<Need>,
    /// A note or a reminder can become a task; every other change is refused. Applied before `stage`.
    pub kind: Option<Kind>,
    pub stage: Option<Stage>,
    pub effort: Option<Option<Size>>,
    pub impact: Option<Option<Size>>,
    pub due: Option<Option<Due>>,
    /// The first millisecond of the new due's date in the request's time zone, sent with `due`. A reminder takes it
    /// as the time it joins the list; other kinds ignore it.
    pub due_starts_at: Option<Timestamp>,
    /// Replaces the whole set of tags.
    pub tags: Option<BTreeSet<TagId>>,
    /// Sets (`Some`) or clears (`None`) the value of each listed custom parameter; others keep theirs.
    pub parameters: BTreeMap<ParameterId, Option<OptionId>>,
    pub state: Option<StateKind>,
    /// The item this one waits for. Only a blocked item can have one; leaving the blocked state clears it. The
    /// service checks the blocker is in the same heap and that no cycle forms.
    pub blocked_by: Option<Option<ItemId>>,
    /// Why the item is blocked, in words. Only a blocked item can have one; leaving the blocked state clears it.
    pub blocked_reason: Option<Option<BlockReason>>,
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
    /// The heap it is in now. A blocker moved away is in another heap than the items it blocked.
    pub heap_id: HeapId,
    pub key: ItemKey,
    pub title: Title,
    pub kind: Kind,
    pub state: ItemState,
}

impl From<&Item> for ItemSummary {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id,
            heap_id: item.heap_id,
            key: item.key,
            title: item.title.clone(),
            kind: item.kind,
            state: item.state.clone(),
        }
    }
}

/// How a blocked item's blocker stopped holding it. The item stays blocked and waits for the person, who unblocks it,
/// closes it or picks another blocker (product.md, Items §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockerGone {
    /// The blocker was closed as done. Closing a blocker as done frees what it blocks in its heap, so this is an item
    /// blocked before that rule, or one the blocker left behind in another heap before it closed.
    Done,
    /// The blocker was closed as won't do.
    WontDo,
    /// The blocker was closed as became: other items carry on its work, and the person decides what this one waits for.
    Became,
    /// The blocker was moved to another heap, and is still open there.
    Moved,
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
    /// The first millisecond of the reminder's date in the time zone of the request that set it; the item joins the
    /// list from then. `None` for an item that was never a reminder. When the reminder becomes a task, the time it
    /// joined the list: its date, or the change if that came first, so its age counts from then.
    pub arrives_at: Option<Timestamp>,
    pub tags: BTreeSet<TagId>,
    /// An option for each custom parameter that is set. Unset parameters are left out, and rank as their
    /// lowest-priority option. A closed item may keep an option or a parameter that was archived since; reopening it
    /// brings them up to date (`parameter::current_values`).
    pub parameters: BTreeMap<ParameterId, OptionId>,
    pub created_at: Timestamp,
    /// Who added it. `None` only for an item added before creators were stored whose `item_created` audit entry
    /// couldn't be found to fill it in.
    pub created_by: Option<UserId>,
    /// The item this one was made from: the note it was split from (`NewItem::origin`), or the item of the run that
    /// added it. Set when it is made and never changed; it stays when either item moves. `None` for an item a person
    /// added without one.
    pub origin_id: Option<ItemId>,
    /// Whether it was split from `origin_id` (`NewItem::origin`, `item add --from`): one of the items that note became.
    /// An item a run adds without an origin links back to the run's item too, but isn't one of them.
    pub made_in_split: bool,
    /// The run that added it. `None` for an item a person added. Set together with `origin_id` by
    /// [`Item::added_in_run`].
    pub created_run_id: Option<RunId>,
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
            state: match fields.blocked_by {
                Some(by) => ItemState::Blocked {
                    by: Some(by),
                    reason: None,
                },
                None => ItemState::Open,
            },
            kind: fields.kind,
            stage: Stage::AsWritten,
            need: fields.need,
            effort: fields.effort,
            impact: fields.impact,
            due: fields.due,
            arrives_at: fields.arrives_at,
            tags: fields.tags,
            parameters: fields.parameters,
            created_at: now,
            created_by: Some(created_by),
            origin_id: fields.origin.map(|origin| origin.item_id),
            made_in_split: fields.origin.is_some(),
            created_run_id: None,
            version: Version::INITIAL,
        }
    }

    /// This new item as added by `run`, held on the item `origin`: it links back to that item and records the run, and
    /// starts with its context written, since a run adds an item only with one. Origin and run are set together,
    /// here only, except that a person's split sets the origin alone. In a run the item is made from the run's item, so
    /// a split's origin is that item too.
    #[must_use]
    pub fn added_in_run(self, origin: ItemId, run: RunId) -> Self {
        Self {
            stage: Stage::WithContext,
            origin_id: Some(origin),
            created_run_id: Some(run),
            ..self
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

    /// Whether this is a reminder whose date hasn't come at `as_of`. Such an item is out of the ranked list, the
    /// agents' queue and the counts; from its date on it is an ordinary open item.
    pub fn is_to_come(&self, as_of: Timestamp) -> bool {
        self.kind == Kind::Reminder && self.arrives_at.is_some_and(|arrives_at| arrives_at > as_of)
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

    /// Why the item is blocked, in words, if it's blocked and one was given.
    pub const fn blocked_reason(&self) -> Option<&BlockReason> {
        self.state.blocked_reason()
    }

    /// How `blocker` stopped holding this item: it is closed, or in another heap. `None` while it holds it, and when
    /// `blocker` isn't the item this one is blocked by.
    pub fn blocker_gone(&self, blocker: &ItemSummary) -> Option<BlockerGone> {
        if self.blocked_by() != Some(blocker.id) {
            return None;
        }
        match blocker.state {
            ItemState::Done { .. } => Some(BlockerGone::Done),
            ItemState::WontDo { .. } => Some(BlockerGone::WontDo),
            ItemState::Became { .. } => Some(BlockerGone::Became),
            ItemState::Open | ItemState::InProgress | ItemState::Blocked { .. } => {
                (blocker.heap_id != self.heap_id).then_some(BlockerGone::Moved)
            }
        }
    }

    /// The note after it became other items, closed by `actor` at `now`, or `None` when it already became. The
    /// returned item keeps the old version; the store bumps it on write. The caller checks something was made from it.
    pub fn became(&self, actor: UserId, now: Timestamp) -> Result<Option<Self>, ItemError> {
        if self.kind != Kind::Note {
            return Err(ItemError::BecameNeedsNote {
                item_id: self.id,
                kind: self.kind,
            });
        }
        match self.state {
            ItemState::Became { .. } => Ok(None),
            ItemState::Done { .. } | ItemState::WontDo { .. } => Err(ItemError::Closed { item_id: self.id }),
            ItemState::Open | ItemState::InProgress | ItemState::Blocked { .. } => Ok(Some(Self {
                state: ItemState::Became {
                    closed_at: now,
                    closed_by: Some(actor),
                },
                ..self.clone()
            })),
        }
    }

    /// The item freed by its blocker closing as done: open, and nothing else changed. The returned item keeps the
    /// old version; the store bumps it on write.
    pub fn freed(&self) -> Self {
        Self {
            state: ItemState::Open,
            ..self.clone()
        }
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
            None => self.state.clone(),
        };
        if state.kind().is_closed() && patch.edits_fields() {
            return Err(ItemError::Closed { item_id: self.id });
        }
        match (&mut state, patch.blocked_by) {
            (_, None) => {}
            (ItemState::Blocked { by, .. }, Some(blocker)) => *by = blocker,
            // Clearing the blocker of an item that has none is a no-op, whatever its state.
            (_, Some(None)) => {}
            (_, Some(Some(_))) => return Err(ItemError::NotBlocked { item_id: self.id }),
        }
        match (&mut state, &patch.blocked_reason) {
            (_, None) => {}
            (ItemState::Blocked { reason, .. }, Some(new)) => reason.clone_from(new),
            // Clearing the reason of an item that has none is a no-op, whatever its state.
            (_, Some(None)) => {}
            (_, Some(Some(_))) => return Err(ItemError::NotBlocked { item_id: self.id }),
        }

        let kind = match (self.kind, patch.kind) {
            (current, None) => current,
            (current, Some(target)) if current == target => current,
            (Kind::Note | Kind::Reminder, Some(Kind::Task)) => Kind::Task,
            (from @ (Kind::Note | Kind::Task | Kind::Reminder), Some(to)) => {
                return Err(ItemError::KindChangeNotAllowed {
                    item_id: self.id,
                    from,
                    to,
                });
            }
        };
        // A reminder turned into a task starts as written (Rules, Kinds and moves).
        let stage = match (self.kind, kind) {
            (Kind::Reminder, Kind::Task) => patch.stage.unwrap_or(Stage::AsWritten),
            _ => patch.stage.unwrap_or(self.stage),
        };
        if !kind.allows(stage) {
            return Err(ItemError::StageNeedsTask { item_id: self.id });
        }
        let (due, arrives_at) = match (kind, patch.due) {
            (Kind::Reminder, Some(None)) => return Err(ItemError::ReminderNeedsDate { item_id: self.id }),
            (Kind::Reminder, Some(Some(due))) => {
                (Some(Due { firm: true, ..due }), patch.due_starts_at.or(self.arrives_at))
            }
            // A reminder turned into a task joins the list now, so it ages from now at the latest, not from a date
            // that may be months ahead.
            (Kind::Task, due) if self.kind == Kind::Reminder => (
                due.unwrap_or(self.due),
                self.arrives_at.map(|arrives_at| arrives_at.min(now)),
            ),
            (_, due) => (due.unwrap_or(self.due), self.arrives_at),
        };

        let edited = Self {
            kind,
            stage,
            title: patch.title.clone().unwrap_or_else(|| self.title.clone()),
            description: patch.description.clone().unwrap_or_else(|| self.description.clone()),
            need: patch.need.unwrap_or(self.need),
            effort: patch.effort.unwrap_or(self.effort),
            impact: patch.impact.unwrap_or(self.impact),
            due,
            arrives_at,
            tags: patch.tags.clone().unwrap_or_else(|| self.tags.clone()),
            parameters: self.with_values(&patch.parameters),
            state,
            ..self.clone()
        };
        Ok((edited != *self).then_some(edited))
    }

    /// The item as it is after it moves to `heap_id` under `key`, carrying `tags` and `parameters` of that heap (see
    /// [`crate::item_move::carry_over`]). Everything else stays, and so does the version, which the store bumps on
    /// write. An item that waits for another item still waits, with nobody picked: its blocker is in the heap it
    /// left. Its reason stays: it is about the world outside the heap.
    /// The items that waited for this one keep it as their blocker, now gone ([`Item::blocker_gone`]).
    pub fn moved_to(
        &self,
        heap_id: HeapId,
        key: ItemKey,
        tags: BTreeSet<TagId>,
        parameters: BTreeMap<ParameterId, OptionId>,
    ) -> Self {
        let state = match &self.state {
            ItemState::Blocked { reason, .. } => ItemState::Blocked {
                by: None,
                reason: reason.clone(),
            },
            other => other.clone(),
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
            return Ok(self.state.clone());
        }
        if target == StateKind::Became {
            return Err(ItemError::BecameNeedsSplit { item_id: self.id });
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
            StateKind::Blocked => ItemState::Blocked { by: None, reason: None },
            StateKind::Done => ItemState::Done {
                closed_at: now,
                closed_by: Some(actor),
            },
            StateKind::WontDo => ItemState::WontDo {
                closed_at: now,
                closed_by: Some(actor),
            },
            StateKind::Became => return Err(ItemError::BecameNeedsSplit { item_id: self.id }),
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

    fn sample_new() -> NewItem {
        NewItem {
            kind: Kind::Note,
            title: Title::parse("Write tests").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
            origin: None,
            blocked_by: None,
        }
    }

    fn sample_item() -> Item {
        Item::new(
            ItemId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            crate::handle::ItemKey::parse("k3x9a").expect("valid"),
            sample_new(),
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
            eq(&ItemState::Done {
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

        expect_that!(reopened.state, eq(&ItemState::Open));
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

        expect_that!(edited.state, eq(&ItemState::InProgress));
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
            ItemState::from_parts(StateKind::Open, Some(at(1)), None, None, None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Done, None, None, None, None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Open, None, None, Some(other), None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Blocked, None, None, Some(other), None),
            ok(eq(&ItemState::Blocked {
                by: Some(other),
                reason: None
            }))
        );
    }

    #[gtest]
    fn only_a_closed_stored_state_has_a_closer() {
        for kind in [StateKind::Open, StateKind::InProgress, StateKind::Blocked] {
            expect_that!(
                ItemState::from_parts(kind, None, Some(actor()), None, None),
                err(anything()),
                "{kind:?}"
            );
        }
        expect_that!(
            ItemState::from_parts(StateKind::Done, None, Some(actor()), None, None),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::WontDo, Some(at(1)), Some(actor()), None, None),
            ok(eq(&ItemState::WontDo {
                closed_at: at(1),
                closed_by: Some(actor())
            }))
        );
        // Items closed before closers were recorded.
        expect_that!(
            ItemState::from_parts(StateKind::Done, Some(at(1)), None, None, None),
            ok(eq(&ItemState::Done {
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

    /// The sample item, blocked by [`blocker`], and the blocker as `state`, in `heap_id`.
    fn blocked_and_blocker(state: ItemState, heap_id: HeapId) -> (Item, ItemSummary) {
        let mut blocked = sample_item();
        blocked.state = ItemState::Blocked {
            by: Some(blocker()),
            reason: None,
        };
        let summary = ItemSummary {
            id: blocker(),
            heap_id,
            key: crate::handle::ItemKey::parse("b7b7b").expect("valid"),
            title: Title::parse("Pick a host").expect("valid"),
            kind: Kind::Note,
            state,
        };
        (blocked, summary)
    }

    #[gtest]
    fn a_blocker_closed_or_moved_away_is_gone() {
        let here = HeapId::from_uuid(Uuid::nil());
        let elsewhere = HeapId::from_uuid(Uuid::from_u128(9));
        let done = ItemState::Done {
            closed_at: at(1),
            closed_by: None,
        };
        let wont_do = ItemState::WontDo {
            closed_at: at(1),
            closed_by: None,
        };
        let cases = [
            (ItemState::Open, here, None),
            (ItemState::InProgress, here, None),
            (ItemState::Blocked { by: None, reason: None }, here, None),
            (done.clone(), here, Some(BlockerGone::Done)),
            (wont_do.clone(), here, Some(BlockerGone::WontDo)),
            (ItemState::Open, elsewhere, Some(BlockerGone::Moved)),
            // Closed says more than moved.
            (wont_do, elsewhere, Some(BlockerGone::WontDo)),
        ];
        for (state, heap_id, expected) in cases {
            let (blocked, summary) = blocked_and_blocker(state.clone(), heap_id);
            expect_that!(blocked.blocker_gone(&summary), eq(expected), "{state:?} in {heap_id:?}");
        }
    }

    #[gtest]
    fn only_the_items_own_blocker_can_be_gone() {
        let (mut item, summary) = blocked_and_blocker(
            ItemState::WontDo {
                closed_at: at(1),
                closed_by: None,
            },
            HeapId::from_uuid(Uuid::nil()),
        );
        item.state = ItemState::Blocked { by: None, reason: None };
        expect_that!(item.blocker_gone(&summary), none());
        item.state = ItemState::Open;
        expect_that!(item.blocker_gone(&summary), none());
    }

    #[gtest]
    fn a_freed_item_is_open_and_otherwise_the_same() {
        let (blocked, _) = blocked_and_blocker(ItemState::Open, HeapId::from_uuid(Uuid::nil()));
        let freed = blocked.freed();
        expect_that!(freed.state, eq(&ItemState::Open));
        expect_that!(
            Item {
                state: blocked.state.clone(),
                ..freed
            },
            eq(&blocked)
        );
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

        expect_that!(open.state, eq(&ItemState::Open));
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

    fn reason(text: &str) -> BlockReason {
        BlockReason::parse(text).expect("valid")
    }

    fn block_patch(by: Option<ItemId>, text: Option<&str>) -> ItemPatch {
        ItemPatch {
            state: Some(StateKind::Blocked),
            blocked_by: by.map(Some),
            blocked_reason: text.map(|text| Some(reason(text))),
            ..ItemPatch::default()
        }
    }

    fn blocked_for(by: Option<ItemId>, text: Option<&str>) -> Item {
        sample_item()
            .apply_patch(&block_patch(by, text), actor(), at(1))
            .expect("allowed")
            .expect("changed")
    }

    #[gtest]
    fn a_block_reason_is_trimmed_and_between_1_and_300_characters() {
        expect_that!(
            BlockReason::parse("  waits for a quote  "),
            ok(eq(&reason("waits for a quote")))
        );
        expect_that!(BlockReason::parse("   "), err(anything()));
        expect_that!(BlockReason::parse(&"x".repeat(300)), ok(anything()));
        expect_that!(BlockReason::parse(&"x".repeat(301)), err(anything()));
    }

    #[gtest]
    fn blocking_with_a_reason_sets_it_with_or_without_a_blocker() {
        let alone = blocked_for(None, Some("Waits for the vendor"));
        expect_that!(
            alone.blocked_reason().map(BlockReason::as_str),
            some(eq("Waits for the vendor"))
        );
        expect_that!(alone.blocked_by(), none());

        let both = blocked_for(Some(blocker()), Some("Waits for the vendor"));
        expect_that!(
            both.blocked_reason().map(BlockReason::as_str),
            some(eq("Waits for the vendor"))
        );
        expect_that!(both.blocked_by(), some(eq(blocker())));
    }

    #[gtest]
    fn a_reason_on_an_item_that_stays_unblocked_is_refused() {
        let item = sample_item();
        let patch = ItemPatch {
            blocked_reason: Some(Some(reason("x"))),
            ..ItemPatch::default()
        };
        expect_that!(
            item.apply_patch(&patch, actor(), at(1)),
            err(eq(&ItemError::NotBlocked { item_id: item.id }))
        );
        let clear = ItemPatch {
            blocked_reason: Some(None),
            ..ItemPatch::default()
        };
        expect_that!(item.apply_patch(&clear, actor(), at(1)), ok(none()));
    }

    #[gtest]
    fn a_reason_alone_can_change_an_item_already_blocked() {
        let blocked = blocked_for(Some(blocker()), Some("old"));
        let patch = ItemPatch {
            blocked_reason: Some(Some(reason("new"))),
            ..ItemPatch::default()
        };
        let edited = blocked
            .apply_patch(&patch, actor(), at(2))
            .expect("allowed")
            .expect("changed");
        expect_that!(edited.blocked_reason().map(BlockReason::as_str), some(eq("new")));
        expect_that!(edited.blocked_by(), some(eq(blocker())));

        let cleared = edited
            .apply_patch(
                &ItemPatch {
                    blocked_reason: Some(None),
                    ..ItemPatch::default()
                },
                actor(),
                at(3),
            )
            .expect("allowed")
            .expect("changed");
        expect_that!(cleared.blocked_reason(), none());
        expect_that!(cleared.state.kind(), eq(StateKind::Blocked));
    }

    #[gtest]
    fn blocking_again_replaces_the_reason_and_keeps_the_blocker() {
        let blocked = blocked_for(Some(blocker()), Some("old"));
        let again = blocked
            .apply_patch(&block_patch(None, Some("new")), actor(), at(2))
            .expect("allowed")
            .expect("changed");
        expect_that!(again.blocked_reason().map(BlockReason::as_str), some(eq("new")));
        expect_that!(again.blocked_by(), some(eq(blocker())));
    }

    #[gtest]
    fn setting_the_blocker_keeps_the_reason() {
        let blocked = blocked_for(None, Some("old"));
        let again = blocked
            .apply_patch(&block_patch(Some(blocker()), None), actor(), at(2))
            .expect("allowed")
            .expect("changed");
        expect_that!(again.blocked_reason().map(BlockReason::as_str), some(eq("old")));
        expect_that!(again.blocked_by(), some(eq(blocker())));
    }

    #[gtest]
    fn every_exit_from_blocked_clears_the_reason() {
        let blocked = blocked_for(Some(blocker()), Some("old"));
        for target in [
            StateKind::Open,
            StateKind::InProgress,
            StateKind::Done,
            StateKind::WontDo,
        ] {
            let patch = ItemPatch {
                state: Some(target),
                ..ItemPatch::default()
            };
            let left = blocked
                .apply_patch(&patch, actor(), at(2))
                .expect("allowed")
                .expect("changed");
            expect_that!(left.blocked_reason(), none(), "{target:?}");
        }
        expect_that!(blocked.freed().blocked_reason(), none());
    }

    #[gtest]
    fn moving_keeps_the_reason_and_clears_the_blocker() {
        let blocked = blocked_for(Some(blocker()), Some("old"));
        let moved = blocked.moved_to(
            HeapId::from_uuid(Uuid::from_u128(9)),
            crate::handle::ItemKey::parse("c8c8c").expect("valid"),
            BTreeSet::new(),
            BTreeMap::new(),
        );
        expect_that!(moved.state.kind(), eq(StateKind::Blocked));
        expect_that!(moved.blocked_by(), none());
        expect_that!(moved.blocked_reason().map(BlockReason::as_str), some(eq("old")));
    }

    #[gtest]
    fn a_stored_reason_on_a_state_that_isnt_blocked_is_rejected() {
        for kind in [StateKind::Open, StateKind::InProgress] {
            expect_that!(
                ItemState::from_parts(kind, None, None, None, Some(reason("x"))),
                err(anything()),
                "{kind:?}"
            );
        }
        expect_that!(
            ItemState::from_parts(StateKind::Done, Some(at(1)), None, None, Some(reason("x"))),
            err(anything())
        );
        expect_that!(
            ItemState::from_parts(StateKind::Blocked, None, None, None, Some(reason("x"))),
            ok(eq(&ItemState::Blocked {
                by: None,
                reason: Some(reason("x"))
            }))
        );
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

    /// A reminder for day 10, which joins the list at the start of that day (day 9's end is `at(10)` minus 1 ms
    /// in these tests, which don't need real zones).
    fn sample_reminder() -> Item {
        let mut reminder = sample_item();
        reminder.kind = Kind::Reminder;
        reminder.due = Some(Due { at: at(11), firm: true });
        reminder.arrives_at = Some(at(10));
        reminder
    }

    #[gtest]
    fn a_reminder_is_to_come_until_its_date() {
        let reminder = sample_reminder();

        expect_that!(reminder.is_to_come(at(9)), eq(true));
        expect_that!(reminder.is_to_come(at(10)), eq(false));
        expect_that!(reminder.is_to_come(at(11)), eq(false));
        expect_that!(sample_item().is_to_come(at(0)), eq(false));
    }

    #[gtest]
    fn a_task_that_was_a_reminder_is_never_to_come() {
        let task = sample_reminder()
            .apply_patch(&make_task_patch(), actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(task.is_to_come(at(1)), eq(false));
    }

    #[gtest]
    fn every_kind_move_is_allowed_or_refused() {
        let allowed = [(Kind::Note, Kind::Task), (Kind::Reminder, Kind::Task)];
        for from in Kind::ALL {
            for to in Kind::ALL {
                let mut item = sample_reminder();
                item.kind = from;
                let patch = ItemPatch {
                    kind: Some(to),
                    ..ItemPatch::default()
                };

                let result = item.apply_patch(&patch, actor(), at(1));

                if from == to {
                    expect_that!(result, ok(none()), "{from:?} to {to:?}");
                } else if allowed.contains(&(from, to)) {
                    expect_that!(result, ok(some(anything())), "{from:?} to {to:?}");
                } else {
                    expect_that!(
                        result,
                        err(eq(&ItemError::KindChangeNotAllowed {
                            item_id: item.id,
                            from,
                            to
                        })),
                        "{from:?} to {to:?}"
                    );
                }
            }
        }
    }

    #[gtest]
    fn a_reminder_becomes_a_task_in_place_as_written_keeping_its_due() {
        let mut reminder = sample_reminder();
        reminder.stage = Stage::WithContext;

        let task = reminder
            .apply_patch(&make_task_patch(), actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(task.kind, eq(Kind::Task));
        expect_that!(task.stage, eq(Stage::AsWritten));
        expect_that!(task.key, eq(reminder.key));
        expect_that!(task.due, eq(reminder.due));
        expect_that!(task.arrives_at, some(eq(at(1))));
    }

    #[gtest]
    fn a_reminder_made_a_task_before_its_date_ages_from_the_change() {
        let task = sample_reminder()
            .apply_patch(&make_task_patch(), actor(), at(5))
            .expect("allowed")
            .expect("changed");

        expect_that!(task.arrives_at, some(eq(at(5))));
    }

    #[gtest]
    fn a_reminder_made_a_task_after_its_date_keeps_its_arrival() {
        let reminder = sample_reminder();

        let task = reminder
            .apply_patch(&make_task_patch(), actor(), at(12))
            .expect("allowed")
            .expect("changed");

        expect_that!(task.arrives_at, eq(reminder.arrives_at));
    }

    #[gtest]
    fn a_reminder_allows_the_stages_a_note_does() {
        expect_that!(Kind::Reminder.allows(Stage::AsWritten), eq(true));
        expect_that!(Kind::Reminder.allows(Stage::WithContext), eq(true));
        expect_that!(Kind::Reminder.allows(Stage::Defined), eq(false));
    }

    #[gtest]
    fn a_new_date_makes_a_reminders_due_firm_and_moves_its_arrival() {
        let patch = ItemPatch {
            due: Some(Some(Due {
                at: at(21),
                firm: false,
            })),
            due_starts_at: Some(at(20)),
            ..ItemPatch::default()
        };

        let moved = sample_reminder()
            .apply_patch(&patch, actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(moved.due, some(eq(Due { at: at(21), firm: true })));
        expect_that!(moved.arrives_at, some(eq(at(20))));
    }

    #[gtest]
    fn a_reminders_date_cant_be_cleared() {
        let patch = ItemPatch {
            due: Some(None),
            ..ItemPatch::default()
        };
        let reminder = sample_reminder();

        expect_that!(
            reminder.apply_patch(&patch, actor(), at(1)),
            err(eq(&ItemError::ReminderNeedsDate { item_id: reminder.id }))
        );
    }

    #[gtest]
    fn a_due_date_on_a_note_doesnt_set_an_arrival() {
        let patch = ItemPatch {
            due: Some(Some(Due {
                at: at(21),
                firm: false,
            })),
            due_starts_at: Some(at(20)),
            ..ItemPatch::default()
        };

        let note = sample_item()
            .apply_patch(&patch, actor(), at(1))
            .expect("allowed")
            .expect("changed");

        expect_that!(note.arrives_at, none());
        expect_that!(note.due.map(|due| due.firm), some(eq(false)));
    }

    #[gtest]
    fn state_codes_round_trip_including_became() {
        for kind in StateKind::ALL {
            expect_that!(StateKind::from_code(kind.code()), some(eq(kind)));
        }
        expect_that!(StateKind::Became.code(), eq("became"));
        expect_that!(StateKind::Became.is_closed(), eq(true));
    }

    #[gtest]
    fn a_became_state_needs_a_close_time_and_no_blocker() {
        expect_that!(
            ItemState::from_parts(StateKind::Became, Some(at(1)), Some(actor()), None, None),
            ok(eq(&ItemState::Became {
                closed_at: at(1),
                closed_by: Some(actor())
            }))
        );
        expect_that!(
            ItemState::from_parts(StateKind::Became, None, None, None, None),
            err(anything())
        );
    }

    #[gtest]
    fn a_note_becomes_once_and_repeating_changes_nothing() {
        let note = sample_item();

        let became = note.became(actor(), at(2)).expect("a note").expect("changed");

        expect_that!(
            became.state,
            eq(&ItemState::Became {
                closed_at: at(2),
                closed_by: Some(actor())
            })
        );
        expect_that!(became.became(actor(), at(3)), ok(none()));
    }

    #[gtest]
    fn only_an_open_note_becomes() {
        let mut task = sample_item();
        task.kind = Kind::Task;
        let done = sample_item()
            .apply_patch(&closing_patch(), actor(), at(1))
            .expect("ok")
            .expect("changed");

        expect_that!(
            task.became(actor(), at(1)),
            err(eq(&ItemError::BecameNeedsNote {
                item_id: task.id,
                kind: Kind::Task
            }))
        );
        expect_that!(
            done.became(actor(), at(2)),
            err(eq(&ItemError::Closed { item_id: done.id }))
        );
    }

    #[gtest]
    fn a_patch_cannot_close_an_item_as_became() {
        let patch = ItemPatch {
            state: Some(StateKind::Became),
            ..ItemPatch::default()
        };
        let item = sample_item();

        expect_that!(
            item.apply_patch(&patch, actor(), at(1)),
            err(eq(&ItemError::BecameNeedsSplit { item_id: item.id }))
        );
    }

    #[gtest]
    fn a_became_item_reopens_within_the_week_only() {
        let became = sample_item().became(actor(), at(1)).expect("note").expect("changed");
        let reopen = ItemPatch {
            state: Some(StateKind::Open),
            ..ItemPatch::default()
        };

        expect_that!(
            became.apply_patch(&reopen, actor(), at(8)),
            ok(some(field!(Item.state, eq(&ItemState::Open))))
        );
        expect_that!(
            became.apply_patch(&reopen, actor(), at(9)),
            err(eq(&ItemError::ReopenWindowExpired {
                item_id: became.id,
                closed_at: at(1)
            }))
        );
    }

    #[gtest]
    fn a_blocker_that_became_is_gone() {
        let here = HeapId::from_uuid(Uuid::nil());
        let (blocked, summary) = blocked_and_blocker(
            ItemState::Became {
                closed_at: at(1),
                closed_by: None,
            },
            here,
        );

        expect_that!(blocked.blocker_gone(&summary), some(eq(BlockerGone::Became)));
    }

    #[gtest]
    fn an_item_made_with_a_blocker_starts_blocked() {
        let mut fields = sample_new();
        fields.blocked_by = Some(blocker());

        let item = Item::new(
            ItemId::from_uuid(Uuid::from_u128(5)),
            HeapId::from_uuid(Uuid::nil()),
            crate::handle::ItemKey::parse("k3x9b").expect("valid"),
            fields,
            actor(),
            at(0),
        );

        expect_that!(
            item.state,
            eq(&ItemState::Blocked {
                by: Some(blocker()),
                reason: None
            })
        );
    }
}
