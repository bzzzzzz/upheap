//! The question commands: their args and flows, and what they share.

pub mod ask;

use api_types::common::ItemDto;
use serde::Serialize;

use crate::CliError;
use crate::client::{ClientError, Fetched};
use crate::commands::heap_api::{HeapApi, is_not_found};

impl HeapApi<'_> {
    /// `POST` of `body` to the questions of the item with `key`, which the user typed as `typed`. A `404` is the item
    /// not being there, or the heap not being the caller's, which the server doesn't tell apart. A closed item's
    /// refusal names the item as `team/heap/key`, so the hint built from it can be pasted.
    pub async fn post_question(
        &self,
        typed: &str,
        key: &str,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post(&self.path(&["items", key, "questions"]), body, self.token)
            .await
        {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_not_found(&source) => Err(CliError::NoSuchItem {
                name: typed.to_owned(),
                heap: self.heap.to_string(),
                source,
            }),
            Err(source @ ClientError::Problem { .. }) if is_closed(&source) => Err(CliError::ItemRefused {
                name: format!("{}/{key}", self.heap),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }
}

fn is_closed(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if problem.code == "item_closed")
}
