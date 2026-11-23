//! A heap's tags and a team's: listing, creating, renaming and describing them. Tags are never deleted.

use domain::ValidationError;
use domain::permission::{TagAction, TagListAction};
use domain::tag::{NewTag, TagColor, TagDescription, TagName, TagPatch};
use domain::validation::Field;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{HeapSummaryDto, double_option, never_null};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagKind)]
pub enum TagKindDto {
    /// One heap's tag.
    // The default, for a server older than team tags, which has only these.
    #[default]
    Heap,
    /// A team's tag, which every heap of the team has.
    Team,
    /// An owner this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// A tag's colour, one of a fixed palette, in the order a colour is picked in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TagColor)]
pub enum TagColorDto {
    Blue,
    Sky,
    Teal,
    Green,
    Lime,
    Rose,
    Pink,
    // The default, for a server older than tag colours, which leaves the colour out.
    #[default]
    Grey,
    /// A colour this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<TagColor> for TagColorDto {
    fn from(color: TagColor) -> Self {
        match color {
            TagColor::Blue => Self::Blue,
            TagColor::Sky => Self::Sky,
            TagColor::Teal => Self::Teal,
            TagColor::Green => Self::Green,
            TagColor::Lime => Self::Lime,
            TagColor::Rose => Self::Rose,
            TagColor::Pink => Self::Pink,
            TagColor::Grey => Self::Grey,
        }
    }
}

impl TryFrom<TagColorDto> for TagColor {
    type Error = ValidationError;

    /// Fails on a colour this server doesn't know, which a request can carry.
    fn try_from(color: TagColorDto) -> Result<Self, Self::Error> {
        match color {
            TagColorDto::Blue => Ok(Self::Blue),
            TagColorDto::Sky => Ok(Self::Sky),
            TagColorDto::Teal => Ok(Self::Teal),
            TagColorDto::Green => Ok(Self::Green),
            TagColorDto::Lime => Ok(Self::Lime),
            TagColorDto::Rose => Ok(Self::Rose),
            TagColorDto::Pink => Ok(Self::Pink),
            TagColorDto::Grey => Ok(Self::Grey),
            TagColorDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::Color }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Tag)]
pub struct TagDto {
    pub id: Uuid,
    // Defaults so this client reads a server older than team tags, which leaves it out.
    #[serde(default)]
    #[schema(required = true)]
    pub kind: TagKindDto,
    /// The heap that owns the tag; `null` for a team tag.
    pub heap_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    // Defaults so this client reads a server older than tag colours, which leaves it out.
    #[serde(default)]
    #[schema(required = true)]
    pub color: TagColorDto,
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
    // Defaults so this client reads a server older than team tags, which leaves it out.
    #[serde(default)]
    #[schema(required = true)]
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
#[serde(deny_unknown_fields)]
pub struct CreateTagDto {
    /// 1 to 50 characters, unique among the tags of the heap, or of the team, ignoring case.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Left out, the server picks the least used colour of the tags the new tag will sit among.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<TagColorDto>,
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
            color: self.color.map(TagColor::try_from).transpose()?,
        })
    }
}

/// A partial edit. Leave a field out to keep it; send `null` to clear the description.
#[derive(Default, Serialize, Deserialize, ToSchema)]
#[schema(as = TagPatchRequest)]
#[serde(deny_unknown_fields)]
pub struct TagPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
    /// The colour can be changed but not cleared; `null` is refused.
    #[serde(default, deserialize_with = "never_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = TagColorDto, required = false)]
    pub color: Option<TagColorDto>,
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
            color: self.color.map(TagColor::try_from).transpose()?,
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
            color: None,
        };
        let coloured = CreateTagDto {
            color: Some(TagColorDto::Rose),
            ..tag
        };

        verify_that!(
            serde_json::to_string(&coloured)?,
            eq(r#"{"name":"Ops","color":"rose"}"#)
        )
    }

    #[gtest]
    fn a_tag_without_a_colour_reads_as_grey_and_an_unknown_colour_as_unknown() -> googletest::Result<()> {
        let older = r#"{"id": "00000000-0000-0000-0000-000000000003", "heap_id": null, "name": "infra",
                        "description": null, "version": 1, "allowed_actions": []}"#;
        let newer = older.replace(r#""name""#, r#""color": "mauve", "name""#);

        let older: TagDto = serde_json::from_str(older)?;
        let newer: TagDto = serde_json::from_str(&newer)?;

        expect_that!(older.color, eq(TagColorDto::Grey));
        expect_that!(newer.color, eq(TagColorDto::Unknown));
        Ok(())
    }

    #[gtest]
    fn a_request_naming_an_unknown_colour_is_refused() {
        let patch = TagPatchDto {
            color: Some(TagColorDto::Unknown),
            ..TagPatchDto::default()
        };

        expect_that!(
            patch.to_patch(),
            err(eq(&ValidationError::ValueUnknown { field: Field::Color }))
        );
    }

    #[gtest]
    fn a_tag_list_from_a_server_before_team_tags_has_heap_tags_only() -> googletest::Result<()> {
        let body = r#"{
            "heap": {"id": "00000000-0000-0000-0000-000000000001", "team_id": "00000000-0000-0000-0000-000000000002",
                     "slug": "ops", "name": "Ops", "archived": false},
            "tags": [{"id": "00000000-0000-0000-0000-000000000003", "heap_id": "00000000-0000-0000-0000-000000000001",
                      "name": "infra", "description": null, "version": 1, "allowed_actions": []}],
            "allowed_actions": ["create"]
        }"#;

        let list: TagListDto = serde_json::from_str(body)?;

        expect_that!(list.tags, elements_are![field!(TagDto.kind, eq(&TagKindDto::Heap))]);
        expect_that!(list.team_tags, is_empty());
        Ok(())
    }
}
