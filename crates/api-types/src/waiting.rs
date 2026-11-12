//! What waits for the signed-in user (`GET /me/waiting`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{BlockerGoneDto, ItemActionDto, ItemRefDto, KindDto, QuestionDto, StageDto};
use crate::turns::{GoalDto, TurnDto};

/// What waits for the caller: every open question on an open item, every item with a stuck agent turn, and every
/// blocked item whose blocker is gone, in their active heaps, most important first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Waiting)]
pub struct WaitingDto {
    /// How many rows wait: questions, stuck turns and gone blockers. At most 500 are in `rows`; this is the true total.
    pub count: u32,
    /// Ordered by the item's score in its own heap, highest first; ties by heap name, then the item's position in
    /// its heap, a stuck or blocker row before the item's questions, then the question's number. Heaps weigh the same for now.
    /// Questions on reminders to come have no score and come after the rest, by the reminder's date, soonest first.
    pub rows: Vec<WaitingRowDto>,
}

/// What kind of thing waits. More kinds can join.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = WaitingRowKind)]
pub enum WaitingRowKindDto {
    /// An open question or request on an item.
    Question,
    /// An item whose agent turn is stuck: 3 runs in a row failed.
    Stuck,
    /// A blocked item whose blocker was closed as won't do (or done, before closing as done freed what it blocks), or
    /// moved to another heap.
    BlockerGone,
    /// A kind this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One thing that waits.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingRow)]
pub struct WaitingRowDto {
    pub kind: WaitingRowKindDto,
    pub item: WaitingItemDto,
    pub heap: ItemHeapDto,
    /// The open question, as the item carries it. Set on a `question` row.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub question: Option<QuestionDto>,
    /// The stuck turn. Set on a `stuck` row.
    #[serde(default)]
    #[schema(required = true)]
    pub stuck: Option<WaitingStuckDto>,
    /// The gone blocker. Set on a `blocker_gone` row.
    #[serde(default)]
    #[schema(required = true)]
    pub blocker: Option<WaitingBlockerDto>,
    /// The title of the step the question names, so a client needn't load the item's steps. `null` when it names
    /// none, or one that no longer exists, and on the other rows.
    pub step_title: Option<String>,
}

/// A turn whose runs keep failing.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingStuck)]
pub struct WaitingStuckDto {
    /// How many runs in a row failed.
    pub failed_runs: u32,
    /// Why the latest failed run failed; `null` when that is not known.
    pub reason: Option<String>,
    /// The stable state the turn goes to.
    pub goal: GoalDto,
    /// What the caller may do about it: `retry` and `take_back`, when they may edit items.
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ItemActionDto>,
}

/// A blocker that stopped holding the row's item, which stays blocked until the caller moves it on.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingBlocker)]
pub struct WaitingBlockerDto {
    /// The blocker. Its key is in the heap `gone.moved_to` names when it was moved, else in the row's heap.
    pub item: ItemRefDto,
    pub gone: BlockerGoneDto,
    /// What the caller may do about it: `change_state`, to unblock the item, when they may edit items.
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ItemActionDto>,
}

/// The item a row is on.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingItem)]
pub struct WaitingItemDto {
    pub id: Uuid,
    /// Five characters, unique within the heap.
    #[schema(example = "k3x9a")]
    pub key: String,
    pub title: String,
    pub kind: KindDto,
    /// How far the item has been prepared.
    pub stage: Option<StageDto>,
    /// The item's open turn, if an agent was sent it and it hasn't been taken back.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub turn: Option<TurnDto>,
    /// Whether an agent has the item: always `false` here, as a row's item waits for the caller.
    #[serde(default)]
    #[schema(required = true)]
    pub with_agent: bool,
}

/// The heap a row's item is in, and its team.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemHeap)]
pub struct ItemHeapDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub team_slug: String,
    /// The team's name as `GET /me` shows it.
    pub team_name: String,
}
