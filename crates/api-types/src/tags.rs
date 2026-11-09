//! A heap's tags: listing, creating, renaming and describing them. Tags are never deleted.

use domain::ValidationError;
use domain::permission::{TagAction, TagListAction};
use domain::tag::{NewTag, TagDescription, TagName, TagPatch};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{HeapSummaryDto, double_option};

/// An action the caller can take on a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagAction)]
pub enum TagActionDto {
    Edit,
}

impl From<TagAction> for TagActionDto {
    fn from(action: TagAction) -> Self {
        match action {
            TagAction::Edit => Self::Edit,
        }
    }
}

/// An action the caller can take on a heap's tag list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagListAction)]
pub enum TagListActionDto {
    Create,
}

impl From<TagListAction> for TagListActionDto {
    fn from(action: TagListAction) -> Self {
        match action {
            TagListAction::Create => Self::Create,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Tag)]
pub struct TagDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    pub allowed_actions: Vec<TagActionDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TagList)]
pub struct TagListDto {
    pub heap: HeapSummaryDto,
    /// By name.
    pub tags: Vec<TagDto>,
    pub allowed_actions: Vec<TagListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = CreateTagRequest)]
pub struct CreateTagDto {
    /// 1 to 50 characters, unique in the heap ignoring case.
    pub name: String,
    pub description: Option<String>,
}

impl CreateTagDto {
    /// The new tag's fields. Fails on the first invalid field.
    pub fn to_new_tag(&self) -> Result<NewTag, ValidationError> {
        Ok(NewTag {
            name: TagName::parse(&self.name)?,
            description: self
                .description
                .as_deref()
                .map(TagDescription::parse)
                .transpose()?
                .flatten(),
        })
    }
}

/// A partial edit. Leave a field out to keep it; send `null` to clear the description.
#[derive(Deserialize, ToSchema)]
#[schema(as = TagPatchRequest)]
pub struct TagPatchDto {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
}

impl TagPatchDto {
    /// The edit. Fails on the first invalid field.
    pub fn to_patch(&self) -> Result<TagPatch, ValidationError> {
        Ok(TagPatch {
            name: self.name.as_deref().map(TagName::parse).transpose()?,
            description: match &self.description {
                None => None,
                Some(None) => Some(None),
                Some(Some(raw)) => Some(TagDescription::parse(raw)?),
            },
        })
    }
}
