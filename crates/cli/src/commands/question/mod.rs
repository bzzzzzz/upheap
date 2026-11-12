//! The question commands: their args and flows, and what they share.

pub mod ask;
pub mod reply;

use api_types::common::ItemDto;
use serde::Serialize;

use crate::CliError;
use crate::client::{ClientError, Fetched};
use crate::commands::heap_api::{HeapApi, is_not_found};
use crate::error::QuestionRefusal;

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

impl HeapApi<'_> {
    /// `PATCH` of `body` to question `number` of the item with `key`, which is at `version`. A `404` is the question
    /// not being there. A closed item or withdrawn question is the question taking no reply, which reopening the item
    /// doesn't change, so it has no reopen hint. A stale or answered refusal names the item as `team/heap/key`.
    pub async fn patch_question(
        &self,
        key: &str,
        number: u32,
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        let number_text = number.to_string();
        match self
            .client
            .patch(
                &self.path(&["items", key, "questions", &number_text]),
                body,
                version,
                self.token,
            )
            .await
        {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_not_found(&source) => {
                Err(self.no_such_question(key, number))
            }
            Err(source @ ClientError::Problem { .. }) if is_withdrawn(&source) => Err(CliError::QuestionRefused {
                name: format!("{key}.q{number}"),
                refusal: QuestionRefusal::Withdrawn,
            }),
            Err(source @ ClientError::Problem { .. }) if is_refused_reply(&source) => Err(CliError::ItemRefused {
                name: format!("{}/{key}", self.heap),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// The failure for a question number the item `key` doesn't have.
    pub fn no_such_question(&self, key: &str, number: u32) -> CliError {
        CliError::NoSuchQuestion {
            name: format!("{key}.q{number}"),
            heap: self.heap.to_string(),
        }
    }
}

fn is_withdrawn(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. }
        if matches!(problem.code.as_str(), "item_closed" | "question_withdrawn"))
}

fn is_refused_reply(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. }
        if matches!(problem.code.as_str(), "version_conflict" | "question_answered"))
}

fn is_closed(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if problem.code == "item_closed")
}
