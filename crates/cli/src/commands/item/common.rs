//! What the item commands share: the calls on one item, finding the item a command names, due times from flags,
//! and the words and cells that show an item.

use api_types::common::DueInputDto;
use api_types::common::{ItemDto, SizeDto, StateDto};
use api_types::due_choices::DueChoicesDto;
use serde::Serialize;
use uuid::Uuid;

use crate::client::{ApiClient, ClientError, Fetched, NO_QUERY};
use crate::commands::UsageError;
use crate::commands::heap_api::{HeapApi, is_not_found, named_heap, resolve_heap_handle};
use crate::lookup::{DueWhen, HeapHandle, HeapName, ItemName, due_on, match_due_choice};
use crate::output::{Cell, Tone};
use crate::{App, CliError};

impl HeapApi<'_> {
    /// The item with `key`, which the user typed as `typed`. A `404` is the item not being there, or the heap not
    /// being the caller's, which the server doesn't tell apart.
    pub async fn get_item(&self, typed: &str, key: &str) -> Result<Fetched<ItemDto>, CliError> {
        match self.client.get(&self.path(&["items", key]), NO_QUERY, self.token).await {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_not_found(&source) => Err(CliError::NoSuchItem {
                name: typed.to_owned(),
                heap: self.heap.to_string(),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }

    /// `PATCH` of `body` to the item with `key`, which is at `version`. A stale or closed item's refusal names the
    /// item as `team/heap/key`, so the hint built from it works whatever heap is the default and is safe to paste.
    pub async fn patch_item(
        &self,
        key: &str,
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .patch(&self.path(&["items", key]), body, version, self.token)
            .await
        {
            Ok(item) => Ok(item),
            Err(source @ ClientError::Problem { .. }) if is_stale_or_closed(&source) => Err(CliError::ItemRefused {
                name: format!("{}/{key}", self.heap),
                source,
            }),
            Err(err) => Err(err.into()),
        }
    }
}

fn is_stale_or_closed(err: &ClientError) -> bool {
    matches!(err, ClientError::Problem { problem, .. } if matches!(problem.code.as_str(), "version_conflict" | "item_closed"))
}

/// Where an item is: the heap it named itself, or the heap to look up for its key.
pub(in crate::commands) enum HeapSpot<'a> {
    Known(HeapHandle),
    Named(HeapName<'a>),
}

impl HeapSpot<'_> {
    pub async fn resolve(self, client: &ApiClient, token: &str) -> Result<HeapHandle, CliError> {
        match self {
            Self::Known(handle) => Ok(handle),
            Self::Named(name) => resolve_heap_handle(name, client, token).await,
        }
    }
}

/// The item a command named, found without asking the server.
pub(in crate::commands) struct ItemLocation<'a> {
    pub key: String,
    pub heap: HeapSpot<'a>,
}

/// Where the item `typed` is. A key is in the current heap; a path or link names its own heap, which beats the
/// environment and the config file, but not a different `--heap`.
pub(in crate::commands) fn locate_item<'a>(
    typed: &str,
    item: ItemName,
    heap_flag: Option<&'a str>,
    app: &'a App,
) -> Result<ItemLocation<'a>, CliError> {
    match item {
        ItemName::Key(key) => Ok(ItemLocation {
            key,
            heap: HeapSpot::Named(named_heap(heap_flag, app)?),
        }),
        ItemName::Full { heap, key } => {
            if let Some(flag) = heap_flag
                && is_other_heap(&heap, HeapName::parse(flag)?)
            {
                return Err(UsageError::ItemInOtherHeap {
                    item: typed.to_owned(),
                    item_heap: heap.to_string(),
                    flag_heap: flag.to_owned(),
                }
                .into());
            }
            Ok(ItemLocation {
                key,
                heap: HeapSpot::Known(heap),
            })
        }
    }
}

/// Whether `named` isn't `heap`. A bare slug is the same heap if the slugs match.
fn is_other_heap(heap: &HeapHandle, named: HeapName<'_>) -> bool {
    match named {
        HeapName::Full { team, heap: slug } => team != heap.team || slug != heap.heap,
        HeapName::Bare(slug) => slug != heap.heap,
    }
}

/// The due time `raw` stands for: a date is a firm deadline; anything else is a rough choice, looked up in the
/// server's `GET /due-choices`.
pub(super) async fn resolve_due(raw: &str, client: &ApiClient, token: &str) -> Result<DueInputDto, CliError> {
    match DueWhen::parse(raw) {
        DueWhen::Date(date) => Ok(due_on(&date)),
        DueWhen::Rough(typed) => {
            let choices = client.get::<DueChoicesDto>(&["due-choices"], NO_QUERY, token).await?;
            Ok(match_due_choice(&typed, &choices.value)?)
        }
    }
}

/// `ids` without repeats, in the order they first appear.
pub(super) fn dedupe(ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = Vec::with_capacity(ids.len());
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen
}

pub(super) fn state_text(state: StateDto) -> &'static str {
    match state {
        StateDto::Open => "open",
        StateDto::InProgress => "in progress",
        StateDto::Blocked => "blocked",
        StateDto::Done => "done",
        StateDto::WontDo => "won't do",
    }
}

/// Effort and impact as `s/m`, with `-` for one that's unset.
pub(super) fn sizes_text(effort: Option<SizeDto>, impact: Option<SizeDto>) -> String {
    let size = |size: Option<SizeDto>| match size {
        Some(SizeDto::S) => "s",
        Some(SizeDto::M) => "m",
        Some(SizeDto::L) => "l",
        Some(SizeDto::Xl) => "xl",
        None => "-",
    };
    format!("{}/{}", size(effort), size(impact))
}

pub(super) fn due_cell(item: &ItemDto) -> Cell {
    match &item.due {
        Some(due) if due.overdue => Cell::toned(due.label.clone(), Tone::Alert),
        Some(due) => Cell::new(due.label.clone()),
        None => Cell::default(),
    }
}

pub(super) fn tags_text(item: &ItemDto) -> String {
    item.tags
        .iter()
        .map(|tag| tag.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_heap_flag_is_the_items_heap_when_it_names_it() {
        let heap = HeapHandle {
            team: "ada".to_owned(),
            heap: "personal".to_owned(),
        };

        expect_that!(
            is_other_heap(
                &heap,
                HeapName::Full {
                    team: "ada",
                    heap: "personal"
                }
            ),
            eq(false)
        );
        expect_that!(is_other_heap(&heap, HeapName::Bare("personal")), eq(false));
        expect_that!(
            is_other_heap(
                &heap,
                HeapName::Full {
                    team: "infra",
                    heap: "personal"
                }
            ),
            eq(true)
        );
        expect_that!(is_other_heap(&heap, HeapName::Bare("backlog")), eq(true));
    }

    #[gtest]
    fn repeated_ids_are_kept_once_in_first_seen_order() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));

        expect_that!(dedupe(vec![b, a, b, a]), elements_are![eq(&b), eq(&a)]);
    }
}
