//! What waits for the signed-in user (`GET /me/waiting`).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{KindDto, QuestionDto, StageDto};

/// What waits for the caller: every open question on an open item in their active heaps, most important first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Waiting)]
pub struct WaitingDto {
    /// How many questions wait. At most 500 are in `rows`; this is the true total.
    pub count: u32,
    /// Ordered by the item's score in its own heap, highest first; ties by heap name, then the item's position in
    /// its heap, then the question's number. Heaps weigh the same for now.
    pub rows: Vec<WaitingRowDto>,
}

/// What kind of thing waits. Only questions for now: more kinds can join.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = WaitingRowKind)]
pub enum WaitingRowKindDto {
    /// An open question or request on an item.
    Question,
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
    pub heap: WaitingHeapDto,
    /// The open question, as the item carries it.
    pub question: QuestionDto,
    /// The title of the step the question names, so a client needn't load the item's steps. `null` when it names
    /// none, or one that no longer exists.
    pub step_title: Option<String>,
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
    /// `null` in a build without the stage (the static, single-user one).
    pub stage: Option<StageDto>,
}

/// The heap a row's item is in, and its team.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WaitingHeap)]
pub struct WaitingHeapDto {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub team_slug: String,
    /// The team's name as `GET /me` shows it.
    pub team_name: String,
}
