//! The error body every endpoint answers with, but the OAuth client's token endpoints.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::HeapSummaryDto;
use crate::oauth::RevokeReasonDto;

/// An error response body (RFC 9457 problem details).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Problem {
    /// Stable machine-readable error code, for example `version_conflict`.
    pub code: String,
    /// Short human-readable summary. May change; don't match on it.
    pub title: String,
    pub status: u16,
    /// The input field the error is about, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A value of `field` that would be accepted, when the server can make one: the handle `invalid_handle` hints
    /// at, such as `my-team` for `My-Team`, or a free username for a taken one on `handle_taken`, such as `sam-2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The key of the item the error is about, when it is one other than the item asked for: the item a run holds,
    /// for `run_wrong_item`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_key: Option<String>,
    /// The highest need code a run may give, for `run_need_above_origin`: the need of the note it splits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest: Option<String>,
    /// The heap tags a team tag write would merge, for `tag_merge_unconfirmed`: repeat the write with `token` as
    /// `confirm_reach` to merge them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_reach: Option<MergeReachDto>,
    /// The heap parameters making one a team parameter would merge, for `parameter_merge_unconfirmed`: repeat the
    /// write with `token` as `confirm_reach` to go on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_merge_reach: Option<ParameterMergeReachDto>,
    /// Why a token was revoked, on `token_revoked`, when the server knows: `password_changed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<RevokeReasonDto>,
    /// Present on unexpected errors, so the operator can find the details in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// What making or renaming a team tag into a name would merge: the heap tags of the team's heaps with that name.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = MergeReach)]
pub struct MergeReachDto {
    /// Names exactly these heap tags and item counts; it changes when either does.
    pub token: String,
    /// By heap name, then heap ID.
    pub heaps: Vec<MergeReachHeapDto>,
}

/// A heap tag a merge would take in.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = MergeReachHeap)]
pub struct MergeReachHeapDto {
    pub heap: HeapSummaryDto,
    pub archived: bool,
    pub tag_id: Uuid,
    /// The heap tag's name as written.
    pub tag_name: String,
    /// Items with an active link to the tag, open or closed.
    pub items: u32,
}

/// What making a heap parameter a team parameter would do: the parameter itself, and the same-named heap parameters of
/// the team that merge into it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ParameterMergeReach)]
pub struct ParameterMergeReachDto {
    /// Names exactly these parameters, item counts and added options; it changes when any does.
    pub token: String,
    /// The parameter it starts from first, then the others by heap name, then heap ID.
    pub entries: Vec<ParameterMergeReachEntryDto>,
}

/// A heap parameter in a merge.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ParameterMergeReachEntry)]
pub struct ParameterMergeReachEntryDto {
    pub heap: HeapSummaryDto,
    pub archived: bool,
    pub parameter_id: Uuid,
    /// The parameter's name as written.
    pub parameter_name: String,
    /// Items with a value for the parameter, open or closed.
    pub items: u32,
    /// The labels of its options the team parameter doesn't have yet. Empty for the one the merge starts from.
    pub added_options: Vec<String>,
    /// Its options in use that match an option the team parameter has archived: open items on them get the option in
    /// use that one was replaced by. Empty for the one the merge starts from.
    pub switched: Vec<ParameterMergeSwitchDto>,
    /// Whether it is the one the merge starts from.
    pub starting: bool,
}

/// An option in use of a merged parameter whose open items move to another option of the team parameter.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ParameterMergeSwitch)]
pub struct ParameterMergeSwitchDto {
    /// The option's label as the merged parameter wrote it.
    pub from: String,
    /// The label of the team parameter's option in use that open items get.
    pub to: String,
}
