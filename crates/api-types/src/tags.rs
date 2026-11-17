//! A heap's tags and a team's: listing, creating, renaming and describing them. Tags are never deleted.

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

/// Who owns a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagKind)]
pub enum TagKindDto {
    /// One heap's tag.
    Heap,
    /// A team's tag, which every heap of the team has.
    Team,
    /// An owner this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Tag)]
pub struct TagDto {
    pub id: Uuid,
    pub kind: TagKindDto,
    /// The heap that owns the tag; `null` for a team tag.
    pub heap_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<TagActionDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TagList)]
pub struct TagListDto {
    pub heap: HeapSummaryDto,
    /// The heap's own tags, by name.
    pub tags: Vec<TagDto>,
    /// The heap's team's tags, by name. Every heap of the team has them too.
    #[serde(default)]
    pub team_tags: Vec<TagDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<TagListActionDto>,
}

/// A team's tags.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamTagList)]
pub struct TeamTagListDto {
    /// By name.
    pub tags: Vec<TagDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<TagListActionDto>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = CreateTagRequest)]
pub struct CreateTagDto {
    /// 1 to 50 characters, unique among the tags of the heap, or of the team, ignoring case.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Default, Serialize, Deserialize, ToSchema)]
#[schema(as = TagPatchRequest)]
pub struct TagPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
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

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn tag_patch_leaves_out_what_is_unset_and_writes_clearing_as_null() -> googletest::Result<()> {
        let rename = TagPatchDto {
            name: Some("Platform".to_owned()),
            ..TagPatchDto::default()
        };
        let clear = TagPatchDto {
            description: Some(None),
            ..TagPatchDto::default()
        };

        verify_that!(serde_json::to_string(&rename)?, eq(r#"{"name":"Platform"}"#))?;
        verify_that!(serde_json::to_string(&clear)?, eq(r#"{"description":null}"#))
    }

    #[gtest]
    fn create_tag_leaves_out_a_missing_description() -> googletest::Result<()> {
        let tag = CreateTagDto {
            name: "Ops".to_owned(),
            description: None,
        };

        verify_that!(serde_json::to_string(&tag)?, eq(r#"{"name":"Ops"}"#))
    }
}
