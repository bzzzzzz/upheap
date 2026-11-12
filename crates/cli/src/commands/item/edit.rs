//! `upheap item edit <item>`: change an item's fields.

use api_types::common::{ItemDto, NeedDto, SizeDto, StageDto};
use api_types::items::ItemPatchDto;
use api_types::tags::TagListDto;
use clap::Args;
use uuid::Uuid;

use super::common::{
    ParameterSetting, dedupe, describe_due_flag, locate_item, parameter_values, parse_parameter_setting, resolve_due,
};
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, read_context, read_description};
use crate::commands::{HeapArgs, SharedArgs, UsageError, parse_api_value};
use crate::lookup::{ItemName, LookupError, resolve_tags};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// A new title
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,
    /// How much it's needed: idea, nice_to_have, expected, critical
    #[arg(long, value_name = "NEED", value_parser = parse_api_value::<NeedDto>)]
    pub need: Option<NeedDto>,
    /// A new context for the item (its description). `-` reads all of stdin. For longer text, use --context-file
    #[arg(long, value_name = "TEXT", conflicts_with_all = ["no_description", "context_file"])]
    pub description: Option<String>,
    /// The item's context, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE", conflicts_with = "no_description")]
    pub context_file: Option<String>,
    /// How far it's been prepared: as_written, with_context, defined. Only a task can be defined
    #[arg(long, value_name = "STAGE", value_parser = parse_stage)]
    pub stage: Option<StageDto>,
    /// How much work: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>, conflicts_with = "no_effort")]
    pub effort: Option<SizeDto>,
    /// How much it matters: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>, conflicts_with = "no_impact")]
    pub impact: Option<SizeDto>,
    #[arg(long, value_name = "WHEN", conflicts_with = "no_due", help = describe_due_flag())]
    pub due: Option<String>,
    /// Add a tag of the heap, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    /// Take off a tag, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub untag: Vec<String>,
    /// Set a parameter of the heap to one of its options, as NAME=OPTION, each by name or ID. Repeat for more
    #[arg(long, value_name = "NAME=OPTION", value_parser = parse_parameter_setting)]
    pub parameter: Vec<ParameterSetting>,
    /// Clear a parameter's value, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub no_parameter: Vec<String>,
    /// Clear the due time
    #[arg(long)]
    pub no_due: bool,
    /// Clear the effort
    #[arg(long)]
    pub no_effort: bool,
    /// Clear the impact
    #[arg(long)]
    pub no_impact: bool,
    /// Clear the item's context (its description)
    #[arg(long, visible_alias = "no-context")]
    pub no_description: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

impl EditArgs {
    /// Whether any field flag is given.
    fn has_changes(&self) -> bool {
        self.title.is_some()
            || self.need.is_some()
            || self.description.is_some()
            || self.context_file.is_some()
            || self.stage.is_some()
            || self.effort.is_some()
            || self.impact.is_some()
            || self.due.is_some()
            || !self.tag.is_empty()
            || !self.untag.is_empty()
            || !self.parameter.is_empty()
            || !self.no_parameter.is_empty()
            || self.no_due
            || self.no_effort
            || self.no_impact
            || self.no_description
    }
}

/// Changes the fields given and leaves the rest. Human output says whether anything changed; `--quiet` prints
/// nothing and `--json` the edited item.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    if !args.has_changes() {
        if out.mode() == Mode::Human {
            out.line("Nothing to change: give a field to change, such as --title or --due.")?;
        }
        return Ok(());
    }
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let description = match (&args.description, &args.context_file, args.no_description) {
        (Some(raw), _, _) => Some(Some(read_description(raw, app).await?)),
        (None, Some(path), _) => Some(Some(read_context(path, app).await?)),
        (None, None, true) => Some(None),
        (None, None, false) => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&args.item, &at.key).await?;
    let tag_ids = if args.tag.is_empty() && args.untag.is_empty() {
        None
    } else {
        let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
        let added = resolve_tags(&args.tag, &tags.value, &heap).map_err(LookupError::offering_add)?;
        let removed = resolve_tags(&args.untag, &tags.value, &heap)?;
        if let Some(position) = added.iter().position(|id| removed.contains(id)) {
            return Err(UsageError::TagAndUntag {
                name: args.tag.get(position).cloned().unwrap_or_default(),
            }
            .into());
        }
        Some(merge_tags(&item.value, added, &removed))
    };
    let parameter_values = parameter_values(&args.parameter, &args.no_parameter, &api).await?;
    let due = match (&args.due, args.no_due) {
        (Some(raw), _) => Some(Some(resolve_due(raw, &client, &token).await?)),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let patch = ItemPatchDto {
        title: args.title,
        description,
        need: args.need,
        stage: args.stage,
        effort: clearing(args.effort, args.no_effort),
        impact: clearing(args.impact, args.no_impact),
        due,
        tag_ids,
        parameter_values,
        ..ItemPatchDto::default()
    };
    let edited = api.patch_item(&at.key, item.value.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human if edited.value.version == item.value.version => {
            out.line(&format!("No change to {}.", edited.value.key))?;
        }
        Mode::Human => out.line(&format!("Updated {}: {}.", edited.value.key, edited.value.title))?,
    }
    Ok(())
}

/// Reads `--stage`. The stage type takes any unknown word as a stage a newer server added, which a request can't
/// carry, so only the three it knows are taken.
fn parse_stage(raw: &str) -> Result<StageDto, String> {
    match parse_api_value::<StageDto>(raw)? {
        StageDto::Unknown => Err(format!(
            "unknown stage `{raw}`, expected one of `as_written`, `with_context`, `defined`"
        )),
        stage => Ok(stage),
    }
}

/// A patch field for a value that can be set or cleared: the value, `null` for its `--no-` flag, or left out.
fn clearing<T>(value: Option<T>, clear: bool) -> Option<Option<T>> {
    match (value, clear) {
        (Some(value), _) => Some(Some(value)),
        (None, true) => Some(None),
        (None, false) => None,
    }
}

/// The item's tag IDs with `added` after them and `removed` taken out. Tags it has keep their order, new ones come
/// in the order given, and none repeats.
fn merge_tags(item: &ItemDto, added: Vec<Uuid>, removed: &[Uuid]) -> Vec<Uuid> {
    let current = item.tags.iter().map(|tag| tag.id);
    dedupe(current.chain(added).filter(|id| !removed.contains(id)).collect())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;

    use super::*;

    fn item_with_tags(tags: &[Uuid]) -> googletest::Result<ItemDto> {
        Ok(serde_json::from_value(json!({
            "id": Uuid::new_v4(), "key": "k3x9a", "heap_id": Uuid::new_v4(), "title": "Rotate the staging certs",
            "description": null, "kind": "note", "stage": "as_written", "stage_choices": [], "state": "open", "blocked_by": null, "need": "expected",
            "effort": null, "impact": null, "due": null,
            "tags": tags.iter().map(|id| json!({ "id": id, "name": id.to_string() })).collect::<Vec<_>>(),
            "parameters": [], "created_at": 0, "created_label": "today", "created_by": null, "closed_at": null,
            "closed_label": null, "closed_by": null, "closed_group": null, "steps": [], "step_progress": null, "version": 1, "allowed_actions": [],
        }))?)
    }

    #[gtest]
    fn tags_are_added_after_the_current_ones_and_removed_without_reordering() -> googletest::Result<()> {
        let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let item = item_with_tags(&[a, b])?;

        let merged = merge_tags(&item, vec![c, a], &[b]);

        verify_that!(merged, elements_are![eq(&a), eq(&c)])
    }

    #[gtest]
    fn a_stage_is_one_of_the_three_known() {
        expect_that!(parse_stage("with_context"), ok(eq(&StageDto::WithContext)));
        expect_that!(parse_stage("defined"), ok(eq(&StageDto::Defined)));
        expect_that!(
            parse_stage("reviewed"),
            err(contains_substring("unknown stage `reviewed`"))
        );
    }

    #[gtest]
    fn a_value_is_set_cleared_or_left_alone() {
        expect_that!(clearing(Some(SizeDto::M), false), some(some(eq(SizeDto::M))));
        expect_that!(clearing::<SizeDto>(None, true), some(none()));
        expect_that!(clearing::<SizeDto>(None, false), none());
    }
}
