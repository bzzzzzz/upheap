//! The error body every endpoint answers with, but the OAuth client's token endpoints.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    /// at, such as `my-team` for `My-Team`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The key of the item the error is about, when it is one other than the item asked for: the item a run holds,
    /// for `run_wrong_item`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_key: Option<String>,
    /// The highest need code a run may give, for `run_need_above_origin`: the need of the note it splits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highest: Option<String>,
    /// Present on unexpected errors, so the operator can find the details in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}
