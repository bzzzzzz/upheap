//! A heap's custom parameters and its team's, their options and their tag links. Parameters and options are
//! archived, never deleted.

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
    /// A heap parameter: make it the team's, merging the team's same-named heap parameters into it.
    MakeTeam,
    /// A team parameter in a heap: link the heap's tags to it.
    LinkTags,
    /// A team parameter in a heap: stop the heap using it.
    StopUsing,
    ViewHistory,
}

impl From<ParameterAction> for ParameterActionDto {
    fn from(action: ParameterAction) -> Self {
        match action {
            ParameterAction::Edit => Self::Edit,
            ParameterAction::Archive => Self::Archive,
            ParameterAction::MakeTeam => Self::MakeTeam,
            ParameterAction::LinkTags => Self::LinkTags,
            ParameterAction::StopUsing => Self::StopUsing,
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
    /// Make the heap use one of its team's parameters.
    UseTeamParameter,
}

impl From<ParameterListAction> for ParameterListActionDto {
    fn from(action: ParameterListAction) -> Self {
        match action {
            ParameterListAction::Create => Self::Create,
            ParameterListAction::UseTeamParameter => Self::UseTeamParameter,
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

/// Who owns a parameter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ParameterKind)]
pub enum ParameterKindDto {
    /// One heap's parameter.
    // The default, for a server older than team parameters, which has only these.
    #[default]
    Heap,
    /// A parameter of the heap's team, which the heap uses.
    Team,
    /// An owner this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Parameter)]
pub struct ParameterDto {
    pub id: Uuid,
    #[serde(default)]
    #[schema(required = true)]
    pub kind: ParameterKindDto,
    /// The heap the parameter is listed for: its owner for a heap parameter, and the heap that uses it for a team
    /// parameter. A team parameter's owner is its team, named by its `team` kind.
    pub heap_id: Uuid,
    pub name: String,
    /// Influence on the ranking next to the heap's other weights, 0 to 100. For a team parameter, the heap's weight
    /// for it.
    pub weight: u16,
    /// The options in use, highest priority first.
    pub options: Vec<OptionDto>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOptionDto>,
    /// Every tag whose items the parameter is suggested for in this heap: for a team parameter, the team's links
    /// and the heap's own.
    pub tag_ids: Vec<Uuid>,
    /// A team parameter's links to the team's tags, which suggest it in every heap that uses it. Empty for a heap
    /// parameter.
    #[serde(default)]
    #[schema(required = true)]
    pub team_tag_ids: Vec<Uuid>,
    /// This heap's own links to a team parameter, which suggest it in this heap only. Empty for a heap parameter.
    #[serde(default)]
    #[schema(required = true)]
    pub heap_tag_ids: Vec<Uuid>,
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
    /// Archived parameters, by name. Closed items may still show their values. A team parameter the heap stopped
    /// using is here too.
    pub archived: Vec<ParameterDto>,
    /// The team's other parameters in use, which the heap doesn't use, by name.
    #[serde(default)]
    #[schema(required = true)]
    pub team_available: Vec<TeamParameterDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ParameterListActionDto>,
}

/// A team's parameter, with the heaps that use it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamParameter)]
pub struct TeamParameterDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    /// The options in use, highest priority first.
    pub options: Vec<OptionDto>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOptionDto>,
    /// The team's tags the parameter is suggested for, in every heap that uses it.
    pub tag_ids: Vec<Uuid>,
    /// UTC milliseconds; `null` while the parameter is in use.
    pub archived_at: Option<i64>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    /// The heaps of the team that use the parameter now, by name.
    pub heaps: Vec<HeapSummaryDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ParameterActionDto>,
}

/// A team's parameters.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamParameterList)]
pub struct TeamParameterListDto {
    /// The parameters in use, by name.
    pub parameters: Vec<TeamParameterDto>,
    /// Archived parameters, by name.
    pub archived: Vec<TeamParameterDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ParameterListActionDto>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = CreateTeamParameterRequest)]
pub struct CreateTeamParameterDto {
    /// 1 to 50 characters, unique among the parameters in use of the team and of its heaps ignoring case.
    pub name: String,
    /// Option labels, highest priority first: at most 50, unique ignoring case.
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = TeamParameterPatchRequest)]
pub struct TeamParameterPatchDto {
    /// 1 to 50 characters.
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
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
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ParameterPatchRequest)]
pub struct ParameterPatchDto {
    pub name: Option<String>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: Option<i64>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AddOptionRequest)]
pub struct AddOptionDto {
    /// 1 to 50 characters, unique among the parameter's options in use ignoring case. The new option comes last,
    /// as the lowest priority.
    pub label: String,
}

/// One option of a new arrangement.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OptionArrangement)]
pub struct OptionArrangementDto {
    pub id: Uuid,
    pub label: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArrangeOptionsRequest)]
pub struct ArrangeOptionsDto {
    /// Every option in use, exactly once, in the new order (highest priority first) and with its label.
    pub options: Vec<OptionArrangementDto>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ArchiveOptionRequest)]
pub struct ArchiveOptionDto {
    /// Another option of the same parameter, in use. Open items with the archived option switch to it.
    pub replacement_option_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LinkTagsRequest)]
pub struct LinkTagsDto {
    /// At most 50. Replaces the links: a heap parameter's, with tags of the same heap; a team parameter's, with tags
    /// of the team; a heap's links to a team parameter, with tags of the heap (its own or its team's). `[]` unlinks
    /// them all.
    pub tag_ids: Vec<Uuid>,
}
