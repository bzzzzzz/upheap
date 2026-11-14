//! What the item commands share: the calls on one item, finding the item a command names, due times from flags,
//! and the words and cells that show an item.

use api_types::common::DueInputDto;
use api_types::common::{ItemDto, KindDto, SizeDto, StateDto};
use api_types::due_choices::DueChoicesDto;
use api_types::items::ItemToStepDto;
use api_types::parameters::ParameterListDto;
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::client::{ApiClient, ClientError, Fetched, NO_QUERY};
use crate::commands::UsageError;
use crate::commands::heap_api::{HeapApi, is_not_found, named_heap, resolve_heap_handle};
use crate::lookup::{DueWhen, HeapHandle, HeapName, ItemName, due_on, find_option, find_parameter, match_due_choice};
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

    /// `POST` of `body` to `action` of the item with `key`, which is at `version`. A refusal is reported like
    /// [`patch_item`](Self::patch_item)'s.
    pub async fn post_item_action(
        &self,
        key: &str,
        action: &str,
        version: u32,
        body: &(impl Serialize + ?Sized),
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post_at_version(&self.path(&["items", key, action]), body, version, self.token)
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

    /// `POST` to the item with `key`'s `became`: close the note as became. It says what the note should end up as, so
    /// it takes no version. A `404` is reported like [`get_item`](Self::get_item)'s.
    pub async fn became_item(&self, typed: &str, key: &str) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post(
                &self.path(&["items", key, "became"]),
                &serde_json::json!({}),
                self.token,
            )
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

    /// `POST` of `body` to the item with `key`'s `to-step`: make the note a step of the task `body` names, and close it
    /// as became. It takes no version. A `404` is reported like [`get_item`](Self::get_item)'s.
    pub async fn to_step_item(
        &self,
        typed: &str,
        key: &str,
        body: &ItemToStepDto,
    ) -> Result<Fetched<ItemDto>, CliError> {
        match self
            .client
            .post(&self.path(&["items", key, "to-step"]), body, self.token)
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

/// The help for `--due`, naming every rough choice the way it's typed, `next-week`, so an agent reading the guide
/// needn't guess one. The labels are the shared rule's, not a copy.
pub(super) fn describe_due_flag() -> String {
    let choices: Vec<String> = domain::due::list_due_choice_labels()
        .into_iter()
        .map(|label| label.replace(' ', "-"))
        .collect();
    format!("When: a date, YYYY-MM-DD, or a rough choice: {}", choices.join(", "))
}

/// `ids` without repeats, in the order they first appear.
pub(in crate::commands) fn dedupe(ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = Vec::with_capacity(ids.len());
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen
}

/// The tag IDs `current` with `added` after them and `removed` taken out. Tags already there keep their order, new
/// ones come in the order given, and none repeats. Shared by the commands that take `--tag` and `--untag`.
pub(in crate::commands) fn merge_tag_ids(
    current: impl IntoIterator<Item = Uuid>,
    added: Vec<Uuid>,
    removed: &[Uuid],
) -> Vec<Uuid> {
    dedupe(
        current
            .into_iter()
            .chain(added)
            .filter(|id| !removed.contains(id))
            .collect(),
    )
}

/// A `--parameter NAME=OPTION` as typed: the parameter and the option, trimmed, each a name or an ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterSetting {
    pub parameter: String,
    pub option: String,
}

/// Reads `NAME=OPTION`, split at the first `=` so an option may hold one. Both sides are trimmed, as the server
/// trims names and labels, and neither may be empty.
pub fn parse_parameter_setting(raw: &str) -> Result<ParameterSetting, String> {
    let bad = || format!("--parameter takes NAME=OPTION, such as --parameter Area=Backend; got `{raw}`");
    let (parameter, option) = raw.split_once('=').ok_or_else(bad)?;
    let (parameter, option) = (parameter.trim(), option.trim());
    if parameter.is_empty() || option.is_empty() {
        return Err(bad());
    }
    Ok(ParameterSetting {
        parameter: parameter.to_owned(),
        option: option.to_owned(),
    })
}

/// The patch map for the `--parameter` and `--no-parameter` flags: each parameter's ID to its new option, or to
/// `None` to clear it. A parameter named twice with different values, or both set and cleared, is a usage error.
pub(super) fn resolve_parameter_values(
    set: &[ParameterSetting],
    clear: &[String],
    parameters: &ParameterListDto,
    heap: &HeapHandle,
) -> Result<BTreeMap<Uuid, Option<Uuid>>, CliError> {
    let mut values = BTreeMap::new();
    for setting in set {
        let parameter = find_parameter(&setting.parameter, parameters, heap)?;
        let option = find_option(&setting.option, parameter, heap)?;
        // The same option given twice is kept once, as a repeated --tag is.
        if values
            .insert(parameter.id, Some(option.id))
            .is_some_and(|before| before != Some(option.id))
        {
            return Err(UsageError::ParameterTwice {
                flag: "--parameter",
                name: parameter.name.clone(),
            }
            .into());
        }
    }
    for typed in clear {
        let parameter = find_parameter(typed.trim(), parameters, heap)?;
        match values.insert(parameter.id, None) {
            None => {}
            Some(Some(_)) => {
                return Err(UsageError::ParameterAndNoParameter {
                    name: parameter.name.clone(),
                }
                .into());
            }
            Some(None) => {
                return Err(UsageError::ParameterTwice {
                    flag: "--no-parameter",
                    name: parameter.name.clone(),
                }
                .into());
            }
        }
    }
    Ok(values)
}

/// The patch map for the parameter flags, from one read of the heap's parameters; `None` without a flag, so
/// nothing is read.
pub(super) async fn parameter_values(
    set: &[ParameterSetting],
    clear: &[String],
    api: &HeapApi<'_>,
) -> Result<Option<BTreeMap<Uuid, Option<Uuid>>>, CliError> {
    if set.is_empty() && clear.is_empty() {
        return Ok(None);
    }
    let parameters = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
    resolve_parameter_values(set, clear, &parameters.value, api.heap).map(Some)
}

pub(super) fn state_text(state: StateDto) -> &'static str {
    match state {
        StateDto::Open => "open",
        StateDto::InProgress => "in progress",
        StateDto::Blocked => "blocked",
        StateDto::Done => "done",
        StateDto::WontDo => "won't do",
        StateDto::Became => "became",
        StateDto::Unknown => "unknown",
    }
}

/// ` · with an agent` when an agent has the item, to follow its state as in `open · with an agent`; empty otherwise.
pub(super) fn with_agent_text(item: &ItemDto) -> &'static str {
    if item.with_agent { " · with an agent" } else { "" }
}

pub(super) fn kind_text(kind: KindDto) -> &'static str {
    match kind {
        KindDto::Note => "note",
        KindDto::Task => "task",
        KindDto::Reminder => "reminder",
        KindDto::Unknown => "unknown",
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
    fn a_parameter_setting_splits_at_the_first_equals_and_trims() {
        expect_that!(
            parse_parameter_setting("Area=Backend"),
            ok(eq(&setting("Area", "Backend")))
        );
        expect_that!(
            parse_parameter_setting("Area = Back end"),
            ok(eq(&setting("Area", "Back end")))
        );
        expect_that!(
            parse_parameter_setting("Area=Front=End"),
            ok(eq(&setting("Area", "Front=End")))
        );
    }

    #[gtest]
    fn a_parameter_setting_needs_both_sides() {
        for raw in ["Area", "=X", "Area=", " = ", ""] {
            expect_that!(
                parse_parameter_setting(raw),
                err(eq(&format!(
                    "--parameter takes NAME=OPTION, such as --parameter Area=Backend; got `{raw}`"
                )))
            );
        }
    }

    fn parameters() -> googletest::Result<ParameterListDto> {
        let option = |id: u128, label: &str| serde_json::json!({ "id": Uuid::from_u128(id), "label": label });
        Ok(serde_json::from_value(serde_json::json!({
            "heap": { "id": Uuid::new_v4(), "team_id": Uuid::new_v4(), "slug": "personal", "name": "Personal", "archived": false },
            "parameters": [{
                "id": Uuid::from_u128(1), "heap_id": Uuid::new_v4(), "name": "Area", "weight": 10,
                "options": [option(11, "Backend"), option(12, "Frontend")], "archived_options": [],
                "tag_ids": [], "archived_at": null, "version": 1, "allowed_actions": [],
            }, {
                "id": Uuid::from_u128(2), "heap_id": Uuid::new_v4(), "name": "Customer", "weight": 10,
                "options": [option(21, "Acme")], "archived_options": [],
                "tag_ids": [], "archived_at": null, "version": 1, "allowed_actions": [],
            }],
            "archived": [], "allowed_actions": [],
        }))?)
    }

    fn personal() -> HeapHandle {
        HeapHandle {
            team: "ada".to_owned(),
            heap: "personal".to_owned(),
        }
    }

    fn setting(parameter: &str, option: &str) -> ParameterSetting {
        ParameterSetting {
            parameter: parameter.to_owned(),
            option: option.to_owned(),
        }
    }

    #[gtest]
    fn parameter_flags_become_a_map_of_options_and_cleared_values() -> googletest::Result<()> {
        let values = resolve_parameter_values(
            &[setting("area", "frontend")],
            &["Customer".to_owned()],
            &parameters()?,
            &personal(),
        )?;

        verify_that!(
            values,
            unordered_elements_are![
                (eq(&Uuid::from_u128(1)), some(eq(&Uuid::from_u128(12)))),
                (eq(&Uuid::from_u128(2)), none())
            ]
        )
    }

    #[gtest]
    fn a_parameter_named_twice_is_a_usage_error() -> googletest::Result<()> {
        let parameters = parameters()?;
        let same_twice = resolve_parameter_values(
            &[setting("Area", "Backend"), setting("area", "backend")],
            &[],
            &parameters,
            &personal(),
        );
        let twice = resolve_parameter_values(
            &[
                setting("Area", "Backend"),
                setting(&Uuid::from_u128(1).to_string(), "Frontend"),
            ],
            &[],
            &parameters,
            &personal(),
        );
        let both = resolve_parameter_values(
            &[setting("Area", "Backend")],
            &["area".to_owned()],
            &parameters,
            &personal(),
        );

        expect_that!(same_twice.map(|values| values.len()), ok(eq(&1)));
        expect_that!(
            twice.map_err(|e| e.to_string()),
            err(eq("--parameter names Area twice; give it once"))
        );
        expect_that!(
            both.map_err(|e| e.to_string()),
            err(eq("--parameter and --no-parameter both name Area; give it once"))
        );
        Ok(())
    }

    #[gtest]
    fn repeated_ids_are_kept_once_in_first_seen_order() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));

        expect_that!(dedupe(vec![b, a, b, a]), elements_are![eq(&b), eq(&a)]);
    }
}
