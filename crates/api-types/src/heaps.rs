//! Heaps: reading, renaming, archiving and unarchiving them, and writing their context.

use domain::heap::HeapPolicy;
use domain::permission::HeapAction;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::double_option;
use crate::parameters::ParameterDto;

/// An action the caller can take on a heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = HeapAction)]
pub enum HeapActionDto {
    Rename,
    Archive,
    Unarchive,
    /// Write the heap's context, on `PATCH` with `context`. Every member has it while the heap is active.
    EditContext,
    /// Change the heap's weights and its parameters' weights, on `PATCH …/weights`.
    EditWeights,
    /// Set whether agents prepare new notes, on `PATCH` with `agents_prepare`. Offered to admins while the heap is
    /// active.
    SetAgentsPrepare,
    ViewHistory,
}

impl From<HeapAction> for HeapActionDto {
    fn from(action: HeapAction) -> Self {
        match action {
            HeapAction::Rename => Self::Rename,
            HeapAction::Archive => Self::Archive,
            HeapAction::Unarchive => Self::Unarchive,
            HeapAction::EditContext => Self::EditContext,
            HeapAction::EditWeights => Self::EditWeights,
            HeapAction::SetAgentsPrepare => Self::SetAgentsPrepare,
            HeapAction::ViewHistory => Self::ViewHistory,
        }
    }
}

/// How much each of the heap's own ranking factors counts. A factor's share is its weight out of the total of all
/// weights, parameters' included; `0` turns the factor off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = HeapWeights)]
pub struct HeapWeightsDto {
    /// 0 to 100. The weight of need.
    pub need: u16,
    /// 0 to 100. The weight of how long an item has waited.
    pub age: u16,
    /// 0 to 100. The weight of the due date.
    pub deadline: u16,
    /// 0 to 100. The weight of value per effort: effort and impact count together as one factor.
    pub value: u16,
}

impl From<&HeapPolicy> for HeapWeightsDto {
    fn from(policy: &HeapPolicy) -> Self {
        Self {
            need: policy.need_weight.get(),
            age: policy.age_weight.get(),
            deadline: policy.deadline_weight.get(),
            value: policy.value_weight.get(),
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
    /// What the project is, where its knowledge lives, where work is reviewed and what finishing a piece means, as
    /// written: plain text for now, Markdown later. `null` when nobody has written one.
    pub context: Option<String>,
    /// Whether every item added to the heap is sent to an agent to take it one stable state forward: a note to with
    /// context, a task to defined. Off for a new heap.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub agents_prepare: bool,
    /// When it was archived, in UTC milliseconds; `null` while it is active. An archived heap is read-only.
    pub archived_at: Option<i64>,
    /// Items the heap's list shows by default, before filters: open and in progress ones, as the list's
    /// `unfiltered_count` with the default view.
    pub open_count: u32,
    /// The heap's ranking weights. Read-only here; admins set them with `PATCH …/weights`.
    pub weights: HeapWeightsDto,
    /// Send back in `If-Match` when renaming or changing the context or weights.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<HeapActionDto>,
}

/// A change to a heap. Leave a field out to keep it.
#[derive(Default, Serialize, Deserialize, ToSchema)]
#[schema(as = HeapPatchRequest)]
pub struct HeapPatchDto {
    /// 1 to 100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Links with the old
    /// slug stop working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// At most 20,000 characters, trimmed; blank or `null` clears it. Any member may change it; the name and slug
    /// need an admin.
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, nullable)]
    pub context: Option<Option<String>>,
    /// Whether agents prepare new notes. Needs an admin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents_prepare: Option<bool>,
}

/// New weights for the heap and its parameters, saved together. Leave a weight out to keep it.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = HeapWeightsPatchRequest)]
pub struct HeapWeightsPatchDto {
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<i64>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<i64>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<i64>,
    /// 0 to 100. Value per effort.
    #[schema(minimum = 0, maximum = 100)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    /// Parameters of the heap in use whose weight changes, each listed once.
    #[serde(default)]
    pub parameters: Vec<ParameterWeightChangeDto>,
}

/// A new weight for one parameter, if it is still at `version`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ParameterWeightChange)]
pub struct ParameterWeightChangeDto {
    pub id: Uuid,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: i64,
    /// The parameter's `version` as the client last saw it.
    pub version: u32,
}

/// The heap and its parameters in use after a weights save.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = HeapWeightsResult)]
pub struct HeapWeightsResultDto {
    pub heap: HeapDto,
    /// The heap's parameters in use, by name, with their new versions: what `GET …/parameters` lists as
    /// `parameters`.
    pub parameters: Vec<ParameterDto>,
}
