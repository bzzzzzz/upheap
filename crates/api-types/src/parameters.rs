//! A heap's custom parameters, their options and their tag links. Parameters and options are archived, never
//! deleted.

use domain::permission::{ParameterAction, ParameterListAction};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::HeapSummaryDto;

/// An action the caller can take on a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ParameterAction)]
pub enum ParameterActionDto {
    /// Rename it, change its weight, change its options and link it to tags.
    Edit,
    Archive,
    ViewHistory,
}

impl From<ParameterAction> for ParameterActionDto {
    fn from(action: ParameterAction) -> Self {
        match action {
            ParameterAction::Edit => Self::Edit,
            ParameterAction::Archive => Self::Archive,
            ParameterAction::ViewHistory => Self::ViewHistory,
        }
    }
}

/// An action the caller can take on a heap's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ParameterListAction)]
pub enum ParameterListActionDto {
    Create,
}

impl From<ParameterListAction> for ParameterListActionDto {
    fn from(action: ParameterListAction) -> Self {
        match action {
            ParameterListAction::Create => Self::Create,
        }
    }
}

/// An option in use.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ParameterOption)]
pub struct OptionDto {
    pub id: Uuid,
    pub label: String,
}

/// An archived option.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ArchivedOption)]
pub struct ArchivedOptionDto {
    pub id: Uuid,
    pub label: String,
    /// UTC milliseconds.
    pub archived_at: i64,
    /// The option open items switched to. It may be archived too, with a replacement of its own.
    pub replaced_by_option_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Parameter)]
pub struct ParameterDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub name: String,
    /// Influence on the ranking next to the heap's other weights, 0 to 100.
    pub weight: u16,
    /// The options in use, highest priority first.
    pub options: Vec<OptionDto>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOptionDto>,
    /// The tags whose items the parameter is suggested for.
    pub tag_ids: Vec<Uuid>,
    /// UTC milliseconds; `null` while the parameter is in use.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ParameterActionDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ParameterList)]
pub struct ParameterListDto {
    pub heap: HeapSummaryDto,
    /// The parameters in use, by name.
    pub parameters: Vec<ParameterDto>,
    /// Archived parameters, by name. Closed items may still show their values.
    pub archived: Vec<ParameterDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ParameterListActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CreateParameterRequest)]
pub struct CreateParameterDto {
    /// 1 to 50 characters, unique among the heap's parameters in use ignoring case.
    pub name: String,
    /// 0 to 100. Defaults to 10, the same as the heap's own weights.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: Option<i64>,
    /// Option labels, highest priority first: at most 50, unique ignoring case.
    #[serde(default)]
    pub options: Vec<String>,
    /// Tags of the same heap to suggest the parameter for.
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}

/// A partial edit. Leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ParameterPatchRequest)]
pub struct ParameterPatchDto {
    pub name: Option<String>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AddOptionRequest)]
pub struct AddOptionDto {
    /// 1 to 50 characters, unique among the parameter's options in use ignoring case. The new option comes last,
    /// as the lowest priority.
    pub label: String,
}

/// One option of a new arrangement.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OptionArrangement)]
pub struct OptionArrangementDto {
    pub id: Uuid,
    pub label: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArrangeOptionsRequest)]
pub struct ArrangeOptionsDto {
    /// Every option in use, exactly once, in the new order (highest priority first) and with its label.
    pub options: Vec<OptionArrangementDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArchiveOptionRequest)]
pub struct ArchiveOptionDto {
    /// Another option of the same parameter, in use. Open items with the archived option switch to it.
    pub replacement_option_id: Option<Uuid>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LinkTagsRequest)]
pub struct LinkTagsDto {
    /// Tags of the same heap, at most 50. Replaces the parameter's tags; `[]` unlinks them all.
    pub tag_ids: Vec<Uuid>,
}
