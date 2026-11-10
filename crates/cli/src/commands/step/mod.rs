//! The step commands: their args and flows, and what they share.

pub mod add;
pub mod edit;
pub mod state;

use api_types::common::{ItemDto, StepDto};
use serde::Serialize;

use crate::CliError;
use crate::client::{ClientError, Fetched};
use crate::commands::heap_api::{HeapApi, is_not_found};

impl HeapApi<'_> {
    /// `POST` of `body` to the steps of the item with `key`, which the user typed as `typed`. A `404` is the item
    /// not being there, or the heap not being the caller's, which the server doesn't tell apart. A closed item's
    /// refusal names the item as `team/heap/key`, so the hint built from it can be pasted.
    pub async fn post_step(
        &self,
        typed: &str,
        key: &str,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post(&self.path(&["items", key, "steps"]), body, self.token)
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

    /// `PATCH` of `body` to step `number` of the item with `key`, which is at `version`. A `404` is the step not
    /// being there. A stale step's or closed item's refusal names the item as `team/heap/key`.
    pub async fn patch_step(
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
                &self.path(&["items", key, "steps", &number_text]),
                body,
                version,
                self.token,
            )
            .await
        {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_not_found(&source) => Err(self.no_such_step(key, number)),
            Err(source @ ClientError::Problem { .. }) if is_stale_or_closed(&source) => Err(CliError::ItemRefused {
                name: format!("{}/{key}", self.heap),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// The failure for a step number the item `key` doesn't have.
    pub fn no_such_step(&self, key: &str, number: u32) -> CliError {
        CliError::NoSuchStep {
            name: format!("{key}.{number}"),
            heap: self.heap.to_string(),
        }
    }
}

fn is_closed(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if problem.code == "item_closed")
}

fn is_stale_or_closed(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if matches!(problem.code.as_str(), "version_conflict" | "item_closed"))
}

/// Step `number` of `item`, if it has one. Only a single-item response carries steps; a list row has `null`.
pub(super) fn find_step(item: &ItemDto, number: u32) -> Option<&StepDto> {
    item.steps.iter().flatten().find(|step| step.number == number)
}

/// A step's name: its item's key and its number, as `k3x9a.3`.
pub(super) fn step_name(item: &ItemDto, step: &StepDto) -> String {
    format!("{}.{}", item.key, step.number)
}
