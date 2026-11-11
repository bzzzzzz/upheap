//! `upheap item add <title>`: add an item to a heap, and say where it landed.

use api_types::common::{ItemDto, KindDto, NeedDto, SizeDto};
use api_types::items::{CreateItemDto, ItemPatchDto, RankedListDto, RankedQuery};
use api_types::tags::TagListDto;
use clap::Args;
use uuid::Uuid;

use super::common::{dedupe, resolve_due};
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, parse_api_value};
use crate::lookup::{LookupError, resolve_tags};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// What needs doing
    #[arg(value_name = "TITLE")]
    pub title: String,
    /// How much it's needed: idea, nice_to_have, expected, critical. nice_to_have when left out
    #[arg(long, value_name = "NEED", value_parser = parse_api_value::<NeedDto>)]
    pub need: Option<NeedDto>,
    /// More about it. `-` reads all of stdin
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    /// How much work: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>)]
    pub effort: Option<SizeDto>,
    /// How much it matters: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>)]
    pub impact: Option<SizeDto>,
    /// When: a date, YYYY-MM-DD, or a rough choice such as next-week
    #[arg(long, value_name = "WHEN")]
    pub due: Option<String>,
    /// A tag of the heap, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    /// Add it as a task, not a note
    #[arg(long)]
    pub task: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the item. Human output says where it landed, from one read of the list after the add; `--quiet` prints the
/// new item's key and `--json` the created item, with no read.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let description = match &args.description {
        Some(raw) => Some(read_description(raw, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let tag_ids = if args.tag.is_empty() {
        None
    } else {
        let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
        Some(dedupe(
            resolve_tags(&args.tag, &tags.value, &heap).map_err(LookupError::offering_add)?,
        ))
    };
    let due = match &args.due {
        Some(raw) => Some(resolve_due(raw, &client, &token).await?),
        None => None,
    };
    let body = CreateItemDto {
        title: args.title,
        description,
        kind: args.task.then_some(KindDto::Task),
        need: args.need.unwrap_or(NeedDto::NiceToHave),
        effort: args.effort,
        impact: args.impact,
        due,
        tag_ids,
        parameter_values: None,
    };
    let mut added = api.post::<ItemDto>(&["items"], &body).await?;
    if needs_task_patch(args.task, &added.value) {
        // A server from before `kind` on create ignored it and added a note.
        let patch = ItemPatchDto {
            kind: Some(KindDto::Task),
            ..ItemPatchDto::default()
        };
        let key = added.value.key.clone();
        added = match api.patch_item(&key, added.value.version, &patch).await {
            Ok(task) => task,
            Err(err) => {
                // With --json, stderr carries only the problem body, which scripts parse.
                if out.mode() != Mode::Json {
                    out.note(&format!(
                        "{key} was added as a note; make it a task with `upheap item to-task {key}`"
                    ))?;
                }
                return Err(err);
            }
        };
    }
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&added.value.key)?,
        Mode::Human => {
            // The item is added whether or not we can say where it went.
            let landed = api.get::<RankedListDto>(&["items"], &RankedQuery::default()).await;
            let landing = landed
                .as_ref()
                .ok()
                .and_then(|list| find_landing(&list.value, added.value.id));
            out.line(&added_line(&added.value, landing.as_ref()))?;
            if let Err(err) = landed {
                out.note(&format!("warning: couldn't read where it landed: {err}"))?;
            }
        }
    }
    Ok(())
}

/// Whether the server added a note although `--task` asked for a task, which only a server older than `kind` on
/// create does.
fn needs_task_patch(task: bool, added: &ItemDto) -> bool {
    task && added.kind != KindDto::Task
}

/// Where an item sits in the list: its place in a group of `of` items.
#[derive(Debug, PartialEq, Eq)]
struct Landing {
    position: u32,
    of: usize,
    group: Group,
}

#[derive(Debug, PartialEq, Eq)]
enum Group {
    Pinned,
    Ranked,
    Blocked,
}

/// Where the item `id` is in the list, if the view shows it.
fn find_landing(list: &RankedListDto, id: Uuid) -> Option<Landing> {
    [
        (Group::Pinned, &list.pinned),
        (Group::Ranked, &list.ranked),
        (Group::Blocked, &list.blocked),
    ]
    .into_iter()
    .find_map(|(group, entries)| {
        entries.iter().find(|entry| entry.item.id == id).map(|entry| Landing {
            position: entry.position,
            of: entries.len(),
            group,
        })
    })
}

fn added_line(item: &ItemDto, landing: Option<&Landing>) -> String {
    let what = if item.kind == KindDto::Task { "task " } else { "" };
    let head = format!("Added {what}\"{}\" ({})", item.title, item.key);
    let Some(landing) = landing else {
        return format!("{head}.");
    };
    let suffix = match landing.group {
        Group::Pinned => " pinned",
        Group::Ranked => "",
        Group::Blocked => " blocked",
    };
    format!("{head}, now {} of {}{suffix}.", ordinal(landing.position), landing.of)
}

/// `n` as an English ordinal: 1st, 2nd, 3rd, 4th, 11th, 21st, 112th.
fn ordinal(n: u32) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn sample_item(kind: &str) -> googletest::Result<ItemDto> {
        Ok(serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(), "key": "k3x9a", "heap_id": Uuid::new_v4(), "title": "Rotate certs",
            "description": null, "kind": kind, "stage": "as_written", "stage_choices": [], "state": "open", "blocked_by": null, "need": "expected",
            "effort": null, "impact": null, "due": null, "tags": [], "parameters": [], "created_at": 0,
            "created_label": "today", "created_by": null, "closed_at": null, "closed_label": null,
            "closed_by": null, "closed_group": null, "steps": [], "step_progress": null, "version": 1, "allowed_actions": [],
        }))?)
    }

    #[gtest]
    fn the_line_says_where_the_item_landed_and_in_which_group() -> googletest::Result<()> {
        let item = sample_item("note")?;
        let at = |position, of, group| Landing { position, of, group };

        expect_that!(
            added_line(&item, Some(&at(4, 31, Group::Ranked))),
            eq("Added \"Rotate certs\" (k3x9a), now 4th of 31.")
        );
        expect_that!(
            added_line(&item, Some(&at(1, 2, Group::Pinned))),
            eq("Added \"Rotate certs\" (k3x9a), now 1st of 2 pinned.")
        );
        expect_that!(
            added_line(&item, Some(&at(2, 2, Group::Blocked))),
            eq("Added \"Rotate certs\" (k3x9a), now 2nd of 2 blocked.")
        );
        expect_that!(added_line(&item, None), eq("Added \"Rotate certs\" (k3x9a)."));
        Ok(())
    }

    #[gtest]
    fn the_line_says_task_for_a_task() -> googletest::Result<()> {
        let item = sample_item("task")?;

        expect_that!(
            added_line(
                &item,
                Some(&Landing {
                    position: 4,
                    of: 31,
                    group: Group::Ranked
                })
            ),
            eq("Added task \"Rotate certs\" (k3x9a), now 4th of 31.")
        );
        Ok(())
    }

    #[gtest]
    fn a_note_is_patched_only_when_a_task_was_asked_for() -> googletest::Result<()> {
        let note = sample_item("note")?;
        let task = sample_item("task")?;

        expect_that!(needs_task_patch(true, &note), eq(true));
        expect_that!(needs_task_patch(true, &task), eq(false));
        expect_that!(needs_task_patch(false, &note), eq(false));
        Ok(())
    }

    #[gtest]
    fn ordinals_follow_english() {
        let words: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111, 112]
            .into_iter()
            .map(ordinal)
            .collect();

        expect_that!(
            words,
            elements_are![
                eq("1st"),
                eq("2nd"),
                eq("3rd"),
                eq("4th"),
                eq("11th"),
                eq("12th"),
                eq("13th"),
                eq("21st"),
                eq("22nd"),
                eq("23rd"),
                eq("101st"),
                eq("111th"),
                eq("112th")
            ]
        );
    }
}
