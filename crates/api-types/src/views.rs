//! A heap's saved views: listing them, saving one, changing it and archiving it. Views are never deleted.

use domain::ValidationError;
use domain::permission::{SavedViewAction, SavedViewListAction};
use domain::saved_view::SavedViewPatch;
use domain::text::Name;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::HeapSummaryDto;
use crate::list_view::{ListViewDto, ListViewInputDto};

/// An action the caller can take on a saved view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SavedViewAction)]
pub enum SavedViewActionDto {
    Edit,
    Archive,
}

impl From<SavedViewAction> for SavedViewActionDto {
    fn from(action: SavedViewAction) -> Self {
        match action {
            SavedViewAction::Edit => Self::Edit,
            SavedViewAction::Archive => Self::Archive,
        }
    }
}

/// An action the caller can take on a heap's saved views.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SavedViewListAction)]
pub enum SavedViewListActionDto {
    Create,
}

impl From<SavedViewListAction> for SavedViewListActionDto {
    fn from(action: SavedViewListAction) -> Self {
        match action {
            SavedViewListAction::Create => Self::Create,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = SavedView)]
pub struct SavedViewDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    /// What the view shows. The fields are the query parameters of `GET /teams/{team}/heaps/{heap}/items`.
    pub view: ListViewDto,
    pub created_by: Uuid,
    /// When the view was archived (UTC milliseconds); `null` while it is in use.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<SavedViewActionDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = SavedViewList)]
pub struct SavedViewListDto {
    pub heap: HeapSummaryDto,
    /// The views in use, by name. Archived views are left out.
    pub views: Vec<SavedViewDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<SavedViewListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CreateSavedViewRequest)]
pub struct CreateSavedViewDto {
    /// 1 to 100 characters. Need not be unique.
    pub name: String,
    #[serde(default)]
    pub view: ListViewInputDto,
}

/// A partial edit. Leave a field out to keep it; a `view` replaces the whole view, with defaults for what it leaves
/// out.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SavedViewPatchRequest)]
pub struct SavedViewPatchDto {
    pub name: Option<String>,
    pub view: Option<ListViewInputDto>,
}

impl SavedViewPatchDto {
    /// The edit. Fails on the first invalid field, or when `sort_parameter` doesn't go with `sort`.
    pub fn to_patch(&self) -> Result<SavedViewPatch, ValidationError> {
        Ok(SavedViewPatch {
            name: self.name.as_deref().map(Name::parse).transpose()?,
            list_view: self.view.as_ref().map(ListViewInputDto::to_view).transpose()?,
        })
    }
}
