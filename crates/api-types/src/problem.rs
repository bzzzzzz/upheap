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
