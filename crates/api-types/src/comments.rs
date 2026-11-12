//! The comment endpoint's request, and a comment as an item response carries it. The endpoint answers with the whole
//! item (`ItemDto`).

use domain::ValidationError;
use domain::comment::CommentBody;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common::UserRefDto;

/// A comment to add to an item.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AddCommentRequest)]
pub struct AddCommentDto {
    /// Markdown, trimmed, 1 to 20,000 characters. Line breaks are kept.
    pub body: String,
}

impl AddCommentDto {
    /// The comment's text. Fails when it is blank or too long.
    pub fn to_body(&self) -> Result<CommentBody, ValidationError> {
        CommentBody::parse(&self.body)
    }
}

/// One comment on an item. A comment is never changed or deleted.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Comment)]
pub struct CommentDto {
    /// The comment's number within its item, given when it was added and never changed or reused. The comment is
    /// named by its item's key and this number, as `k3x9a.c3`.
    pub number: u32,
    /// Markdown, as written.
    pub body: String,
    /// Who wrote it. `null` only if they can't be found.
    pub created_by: Option<UserRefDto>,
    /// UTC milliseconds.
    pub created_at: i64,
    /// When it was added, ready to show in the request's timezone: the time ("14:02") for today, else the date
    /// ("yesterday", "3 Nov").
    pub created_label: String,
}
