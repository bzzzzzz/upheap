//! Heaps: reading, renaming, archiving and unarchiving them.

use domain::permission::HeapAction;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// An action the caller can take on a heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = HeapAction)]
pub enum HeapActionDto {
    Rename,
    Archive,
    Unarchive,
    ViewHistory,
}

impl From<HeapAction> for HeapActionDto {
    fn from(action: HeapAction) -> Self {
        match action {
            HeapAction::Rename => Self::Rename,
            HeapAction::Archive => Self::Archive,
            HeapAction::Unarchive => Self::Unarchive,
            HeapAction::ViewHistory => Self::ViewHistory,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Heap)]
pub struct HeapDto {
    pub id: Uuid,
    pub team_id: Uuid,
    /// Unique within the team; the heap's path is `/teams/{team}/heaps/{slug}`.
    pub slug: String,
    pub name: String,
    /// When it was archived, in UTC milliseconds; `null` while it is active. An archived heap is read-only.
    pub archived_at: Option<i64>,
    /// Items the heap's list shows by default, before filters: open and in progress ones, as the list's
    /// `unfiltered_count` with the default view.
    pub open_count: u32,
    /// Send back in `If-Match` when renaming.
    pub version: u32,
    pub allowed_actions: Vec<HeapActionDto>,
}

/// A change to a heap. Leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[schema(as = HeapPatchRequest)]
pub struct HeapPatchDto {
    /// 1 to 100 characters.
    pub name: Option<String>,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Links with the old
    /// slug stop working.
    pub slug: Option<String>,
}
