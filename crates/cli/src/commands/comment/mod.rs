//! The comment commands: their args and flows, and what they share.

pub mod add;
pub mod list;

use api_types::common::ItemDto;
use serde::Serialize;

use crate::CliError;
use crate::client::{ClientError, Fetched};
use crate::commands::heap_api::{HeapApi, is_not_found};

impl HeapApi<'_> {
    /// `POST` of `body` to the comments of the item with `key`, which the user typed as `typed`. A `404` is the item
    /// not being there, or the heap not being the caller's, which the server doesn't tell apart.
    pub async fn post_comment(
        &self,
        typed: &str,
        key: &str,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post(&self.path(&["items", key, "comments"]), body, self.token)
            .await
        {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_not_found(&source) => Err(CliError::NoSuchItem {
                name: typed.to_owned(),
                heap: self.heap.to_string(),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }
}

/// The comments of `item` as lines, oldest first: a head line `c3  Ada, 14:02` with the author and the server's
/// time label, then the body's lines indented four spaces. Empty when it has none, or when `item` is a list row,
/// which doesn't carry them. `comment list` and `item show` print these.
///
/// A line break in a body stays one; a tab becomes four spaces, as the output would otherwise show it as U+FFFD. The
/// caller prints the lines with [`Output::line`](crate::output::Output::line), which makes control characters
/// harmless.
pub(crate) fn comment_lines(item: &ItemDto) -> Vec<String> {
    let mut lines = Vec::new();
    for comment in item.comments.iter().flatten() {
        let author = comment
            .created_by
            .as_ref()
            .map_or("Someone", |user| user.display_name.as_str());
        lines.push(format!("c{}  {author}, {}", comment.number, comment.created_label));
        for line in comment.body.lines() {
            lines.push(if line.trim().is_empty() {
                String::new()
            } else {
                format!("    {}", line.replace('\t', "    "))
            });
        }
    }
    lines
}
