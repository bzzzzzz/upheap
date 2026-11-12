//! `upheap item add <title>`: add an item to a heap, and say where it landed, or when a reminder joins the list.

use api_types::common::{ItemDto, KindDto, NeedDto, SizeDto};
use api_types::items::{CreateItemDto, ItemPatchDto, RankedListDto, RankedQuery};
use api_types::steps::AddStepDto;
use api_types::tags::TagListDto;
use clap::Args;
use domain::item::Title;
use domain::step::MAX_STEPS;
use uuid::Uuid;

use super::common::{
    ParameterSetting, dedupe, describe_due_flag, parameter_values, parse_parameter_setting, resolve_due,
};
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, read_context, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs, UsageError, parse_api_value};
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
    /// More about it. `-` reads all of stdin. For longer text, use --context-file
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    /// The item's context, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE", conflicts_with = "description")]
    pub context_file: Option<String>,
    /// How much work: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>)]
    pub effort: Option<SizeDto>,
    /// How much it matters: s, m, l, xl
    #[arg(long, value_name = "SIZE", value_parser = parse_api_value::<SizeDto>)]
    pub impact: Option<SizeDto>,
    #[arg(long, value_name = "WHEN", help = describe_due_flag())]
    pub due: Option<String>,
    /// A tag of the heap, by name or ID. Repeat for more
    #[arg(long, value_name = "NAME")]
    pub tag: Vec<String>,
    /// Set a parameter of the heap to one of its options, as NAME=OPTION, each by name or ID. Repeat for more
    #[arg(long, value_name = "NAME=OPTION", value_parser = parse_parameter_setting)]
    pub parameter: Vec<ParameterSetting>,
    /// Add it as a task, not a note
    #[arg(long, conflicts_with = "reminder")]
    pub task: bool,
    /// A step of the task, 1 to 300 characters, in order. Repeat for more; needs --task
    #[arg(long, value_name = "TITLE", requires = "task", value_parser = parse_step_title)]
    pub step: Vec<String>,
    /// Add it as a reminder for this date: out of the list until then. A date, YYYY-MM-DD, or a rough choice such
    /// as next-week
    #[arg(long, value_name = "WHEN", conflicts_with_all = ["due", "task"])]
    pub reminder: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// A step title as the server takes it, so a bad one is refused before anything is added.
fn parse_step_title(raw: &str) -> Result<String, String> {
    Title::parse(raw).map(|_| raw.to_owned()).map_err(|err| err.to_string())
}

/// Adds the item, then its steps one by one, in order. Human output says where it landed, from one read of the list
/// after the add; `--quiet` prints the new item's key and `--json` the created item, with its steps when there are
/// any. A failed step add stops there, so the item keeps the steps sent so far.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if args.step.len() > MAX_STEPS {
        return Err(UsageError::TooManySteps {
            given: args.step.len(),
            max: MAX_STEPS,
        }
        .into());
    }
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let description = match (&args.description, &args.context_file) {
        (Some(raw), _) => Some(read_description(raw, app).await?),
        (None, Some(path)) => Some(read_context(path, app).await?),
        (None, None) => None,
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
    let parameter_values = parameter_values(&args.parameter, &[], &api).await?.map(|values| {
        values
            .into_iter()
            .filter_map(|(id, option)| Some((id, option?)))
            .collect()
    });
    // A reminder's date is sent as its due, which the server makes firm.
    let due = match args.reminder.as_ref().or(args.due.as_ref()) {
        Some(raw) => Some(resolve_due(raw, &client, &token).await?),
        None => None,
    };
    let kind = if args.reminder.is_some() {
        Some(KindDto::Reminder)
    } else {
        args.task.then_some(KindDto::Task)
    };
    let body = CreateItemDto {
        title: args.title,
        description,
        kind,
        need: args.need.unwrap_or(NeedDto::NiceToHave),
        effort: args.effort,
        impact: args.impact,
        due,
        tag_ids,
        parameter_values,
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
    let steps = args.step.len();
    for (sent, title) in args.step.iter().enumerate() {
        let body = AddStepDto {
            title: title.clone(),
            context: None,
            after: None,
        };
        added = match api.post_step(&added.value.key, &added.value.key, &body).await {
            Ok(item) => item,
            Err(err) => {
                // With --json, stderr carries only the problem body, which scripts parse; stdout gets the item as it
                // stands, so a script still learns it was added and with which steps.
                if out.mode() == Mode::Json {
                    out.json(&added.body)?;
                } else {
                    out.note(&partial_steps_note(&added.value.key, sent, steps))?;
                }
                return Err(err);
            }
        };
    }
    if args.reminder.is_some() && added.value.kind != KindDto::Reminder && out.mode() != Mode::Json {
        // A server from before reminders ignored the kind, and added a note with a due date. A note can't become a
        // reminder, so it stays as added.
        out.note(&format!(
            "warning: {} was added as a note with a due date, because this server doesn't know reminders",
            added.value.key
        ))?;
    }
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&added.value.key)?,
        Mode::Human if is_to_come(&added.value, app.env.now) => out.line(&to_come_line(&added.value))?,
        Mode::Human => {
            // The item is added whether or not we can say where it went.
            let landed = api.get::<RankedListDto>(&["items"], &RankedQuery::default()).await;
            let landing = landed
                .as_ref()
                .ok()
                .and_then(|list| find_landing(&list.value, added.value.id));
            out.line(&added_line(&added.value, steps, landing.as_ref()))?;
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

/// Whether `item` is a reminder whose date hasn't come at `now`: it is out of the list.
pub(super) fn is_to_come(item: &ItemDto, now: jiff::Timestamp) -> bool {
    item.kind == KindDto::Reminder
        && item
            .arrives_at
            .is_some_and(|arrives_at| arrives_at > now.as_millisecond())
}

/// What a reminder that isn't in the list yet says when it is added.
fn to_come_line(item: &ItemDto) -> String {
    let date = item.due.as_ref().map_or("its date", |due| due.date.as_str());
    format!(
        "Added reminder \"{}\" ({}); it joins the list on {date}.",
        item.title, item.key
    )
}

/// What stderr says when step number `sent + 1` of `total` couldn't be added, after `sent` were: the item exists, and
/// the command that adds the rest.
fn partial_steps_note(key: &str, sent: usize, total: usize) -> String {
    let added = if sent == 0 {
        format!("none of its {total} steps")
    } else {
        format!("its first {sent} of {total} steps")
    };
    let rest = if sent == 0 { "them" } else { "the rest" };
    format!("{key} was added with {added}; add {rest} with `upheap step add {key} \"<title>\"`")
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

fn added_line(item: &ItemDto, steps: usize, landing: Option<&Landing>) -> String {
    let what = match item.kind {
        KindDto::Task => "task ",
        KindDto::Reminder => "reminder ",
        KindDto::Note | KindDto::Unknown => "",
    };
    let with = match steps {
        0 => String::new(),
        1 => " with 1 step".to_owned(),
        n => format!(" with {n} steps"),
    };
    let head = format!("Added {what}\"{}\" ({}){with}", item.title, item.key);
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
            "arrives_at": null,
        }))?)
    }

    #[gtest]
    fn a_reminder_is_to_come_until_its_arrival() -> googletest::Result<()> {
        let mut item = sample_item("reminder")?;
        item.arrives_at = Some(2_000);
        item.due = serde_json::from_value(serde_json::json!({
            "at": 3_000, "date": "2027-03-12", "firm": true, "label": "12 Mar", "overdue": false,
        }))?;

        expect_that!(is_to_come(&item, jiff::Timestamp::from_millisecond(1_000)?), eq(true));
        expect_that!(is_to_come(&item, jiff::Timestamp::from_millisecond(2_000)?), eq(false));
        expect_that!(
            to_come_line(&item),
            eq("Added reminder \"Rotate certs\" (k3x9a); it joins the list on 2027-03-12.")
        );
        expect_that!(
            added_line(&item, 0, None),
            eq("Added reminder \"Rotate certs\" (k3x9a).")
        );
        Ok(())
    }

    #[gtest]
    fn the_line_says_where_the_item_landed_and_in_which_group() -> googletest::Result<()> {
        let item = sample_item("note")?;
        let at = |position, of, group| Landing { position, of, group };

        expect_that!(
            added_line(&item, 0, Some(&at(4, 31, Group::Ranked))),
            eq("Added \"Rotate certs\" (k3x9a), now 4th of 31.")
        );
        expect_that!(
            added_line(&item, 0, Some(&at(1, 2, Group::Pinned))),
            eq("Added \"Rotate certs\" (k3x9a), now 1st of 2 pinned.")
        );
        expect_that!(
            added_line(&item, 0, Some(&at(2, 2, Group::Blocked))),
            eq("Added \"Rotate certs\" (k3x9a), now 2nd of 2 blocked.")
        );
        expect_that!(added_line(&item, 0, None), eq("Added \"Rotate certs\" (k3x9a)."));
        Ok(())
    }

    #[gtest]
    fn the_line_says_task_for_a_task() -> googletest::Result<()> {
        let item = sample_item("task")?;

        expect_that!(
            added_line(
                &item,
                0,
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
    fn the_line_counts_the_steps_after_the_key() -> googletest::Result<()> {
        let item = sample_item("task")?;
        let landing = Landing {
            position: 4,
            of: 31,
            group: Group::Ranked,
        };

        expect_that!(
            added_line(&item, 2, Some(&landing)),
            eq("Added task \"Rotate certs\" (k3x9a) with 2 steps, now 4th of 31.")
        );
        expect_that!(
            added_line(&item, 1, None),
            eq("Added task \"Rotate certs\" (k3x9a) with 1 step.")
        );
        Ok(())
    }

    #[gtest]
    fn a_failed_step_names_the_item_and_the_command_for_the_rest() {
        expect_that!(
            partial_steps_note("k3x9a", 2, 5),
            eq("k3x9a was added with its first 2 of 5 steps; add the rest with `upheap step add k3x9a \"<title>\"`")
        );
        expect_that!(
            partial_steps_note("k3x9a", 0, 5),
            eq("k3x9a was added with none of its 5 steps; add them with `upheap step add k3x9a \"<title>\"`")
        );
    }

    #[gtest]
    fn a_step_title_follows_the_servers_rule() {
        expect_that!(parse_step_title("Renew"), ok(eq("Renew")));
        expect_that!(parse_step_title("  "), err(anything()));
        expect_that!(parse_step_title(&"x".repeat(301)), err(anything()));
        expect_that!(parse_step_title(&"x".repeat(300)), ok(anything()));
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
