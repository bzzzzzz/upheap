//! `upheap item move <item>... <heap>`: move one or several items to another heap of their team.
//!
//! The server owns every rule: which heaps are allowed, the key, and what tags and values come along. The CLI only
//! notices an item that is in the heap already, which it has in hand from the read that gives it the version, and then
//! makes no write; and it words what the server dropped by comparing the item before and after.
//!
//! One item is read and moved as always, and its failure is the command's. Several items go through the same read and
//! the same single move one after the other, not through the batch endpoint, so each is refused for the reasons and in
//! the words a single move is. A refusal is told and the rest carry on; the command then exits with the code its first
//! failure would have given alone.

use std::collections::HashSet;

use api_types::common::ItemDto;
use api_types::heaps::HeapDto;
use api_types::items::ItemMoveDto;
use api_types::me::MeDto;
use clap::Args;
use serde_json::{Value, json};
use uuid::Uuid;

use super::common::locate_item;
use crate::client::ApiClient;
use crate::client::Fetched;
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::{HeapHandle, HeapName, ItemName, resolve_heap};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct MoveArgs {
    /// The items: each a key, team/heap/key, or a link to it. With several, each moves on its own and the command
    /// says which didn't; `--quiet` prints the new keys of those that did, and `--json` an array with, for each item,
    /// the moved item or `{"item": "<as given>", "error": {...}}`
    #[arg(value_name = "ITEM", num_args = 1.., required = true)]
    pub items: Vec<String>,
    /// The heap to move them to: team/heap, or a heap slug only one of your teams has. It must be in the items' team
    #[arg(value_name = "HEAP")]
    pub to: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// What a move of one item came to: the item as it is now, and as it was when it moved, or none when it was in the
/// heap already.
struct Moved {
    item: Fetched<ItemDto>,
    before: Option<ItemDto>,
    target: String,
}

impl Moved {
    /// What the human output says, line by line.
    fn lines(&self) -> Vec<String> {
        match &self.before {
            None => vec![format!("{} is already in {}.", self.item.value.key, self.target)],
            Some(before) => moved_lines(before, &self.item.value, &self.target),
        }
    }
}

/// What the items share: the connection, and the target once it is known.
struct Session {
    client: ApiClient,
    token: String,
    target: Option<HeapHandle>,
    target_id: Option<Uuid>,
}

/// Moves the items to the heap, or says they are there already.
pub async fn run(args: MoveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let mut session = None;
    if let [item] = args.items.as_slice() {
        let moved = move_one(&args, item, app, &mut session).await?;
        match out.mode() {
            Mode::Json => out.json(&moved.item.body)?,
            Mode::Quiet => out.line(&moved.item.value.key)?,
            Mode::Human => {
                for line in moved.lines() {
                    out.line(&line)?;
                }
            }
        }
        return Ok(());
    }
    let mut results = Vec::new();
    for item in &args.items {
        results.push(move_one(&args, item, app, &mut session).await);
    }
    let mut moved_count = 0;
    let mut json_items = Vec::new();
    for (item, result) in args.items.iter().zip(&results) {
        match result {
            Ok(moved) => {
                moved_count += 1;
                match out.mode() {
                    Mode::Json => {
                        json_items.push(serde_json::from_str::<Value>(&moved.item.body).map_err(std::io::Error::other)?)
                    }
                    Mode::Quiet => out.line(&moved.item.value.key)?,
                    Mode::Human => {
                        let mut lines = moved.lines().into_iter();
                        if let Some(first) = lines.next() {
                            out.line(&first)?;
                        }
                        for line in lines {
                            out.line(&format!("  {line}"))?;
                        }
                    }
                }
            }
            Err(err) => match out.mode() {
                Mode::Json => json_items.push(json!({ "item": item, "error": error_json(err) })),
                Mode::Quiet => {}
                Mode::Human => out.line(&format!("Not moved {item}: {}", err.with_causes()))?,
            },
        }
    }
    match out.mode() {
        Mode::Json => out.json(&Value::Array(json_items).to_string())?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("Moved {moved_count} of {} items.", args.items.len()))?,
    }
    match results.into_iter().find_map(Result::err) {
        Some(first) => Err(CliError::Reported {
            exit_code: first.exit_code(),
        }),
        None => Ok(()),
    }
}

/// The problem the server refused with, or the message when the failure isn't one.
fn error_json(err: &CliError) -> Value {
    err.problem_body()
        .and_then(|body| serde_json::from_str(body).ok())
        .unwrap_or_else(|| json!({ "message": err.with_causes() }))
}

/// Moves the item `typed` names to the heap `args` names, or finds it there already.
async fn move_one(args: &MoveArgs, typed: &str, app: &App, session: &mut Option<Session>) -> Result<Moved, CliError> {
    let name = ItemName::parse(typed)?;
    let at = locate_item(typed, name, args.heap.name.as_deref(), app)?;
    let target_name = HeapName::parse(&args.to)?;
    if session.is_none() {
        let client = app.client(app.server(&args.shared)?)?;
        let token = app.access_token(&client).await?;
        *session = Some(Session {
            client,
            token,
            target: None,
            target_id: None,
        });
    }
    let Some(Session {
        client,
        token,
        target,
        target_id,
    }) = session
    else {
        unreachable!("the session was made above");
    };
    let heap = at.heap.resolve(client, token).await?;
    if target.is_none() {
        *target = Some(resolve_target(target_name, &heap, client, token).await?);
    }
    let Some(target) = target.as_ref() else {
        unreachable!("the target was resolved above");
    };
    let api = HeapApi {
        client,
        token,
        heap: &heap,
    };
    let item = api.get_item(typed, &at.key).await?;
    if *target == heap {
        return Ok(Moved {
            item,
            before: None,
            target: target.to_string(),
        });
    }
    // The body names the heap by ID, which its path doesn't give.
    let heap_id = match *target_id {
        Some(id) => id,
        None => {
            let target_api = HeapApi {
                client,
                token,
                heap: target,
            };
            let id = target_api.get::<HeapDto>(&[], NO_QUERY).await?.value.id;
            *target_id = Some(id);
            id
        }
    };
    let body = ItemMoveDto { heap_id };
    let after = api.post_item_action(&at.key, "move", item.value.version, &body).await?;
    Ok(Moved {
        item: after,
        before: Some(item.value),
        target: target.to_string(),
    })
}

/// The heap to move to. A bare slug is looked for in the item's team first, the only team that can be a target; when
/// that team has no such heap it is resolved among all the user's teams as usual, so the server's `heap_not_in_team`
/// message still comes back for another team's heap.
async fn resolve_target(
    name: HeapName<'_>,
    item_heap: &HeapHandle,
    client: &ApiClient,
    token: &str,
) -> Result<HeapHandle, CliError> {
    if let HeapName::Bare(slug) = name {
        let me = client.get::<MeDto>(&["me"], NO_QUERY, token).await?;
        let in_team = HeapName::Full {
            team: &item_heap.team,
            heap: slug,
        };
        if let Ok(handle) = resolve_heap(in_team, &me.value) {
            return Ok(handle);
        }
    }
    resolve_heap_handle(name, client, token).await
}

/// What the move did, line by line: that it moved, a key that changed, and the tags and values left behind.
fn moved_lines(before: &ItemDto, after: &ItemDto, target: &str) -> Vec<String> {
    let mut lines = vec![format!("Moved {}: {} to {target}.", before.key, before.title)];
    if after.key != before.key {
        lines.push(format!(
            "Its key is now {}: the heap already had {}.",
            after.key, before.key
        ));
    }
    let tags = dropped_tags(before, after);
    if !tags.is_empty() {
        lines.push(format!("Dropped tags the heap doesn't have: {}.", tags.join(", ")));
    }
    let values = dropped_values(before, after);
    if !values.is_empty() {
        lines.push(format!(
            "Dropped values the heap has no match for: {}.",
            values.join(", ")
        ));
    }
    lines
}

/// The names of the tags `before` had that `after` has no tag of the same name for, ignoring case. For the message
/// only: the server decides what is kept.
fn dropped_tags(before: &ItemDto, after: &ItemDto) -> Vec<String> {
    let kept: HashSet<String> = after.tags.iter().map(|tag| tag.name.to_lowercase()).collect();
    before
        .tags
        .iter()
        .filter(|tag| !kept.contains(&tag.name.to_lowercase()))
        .map(|tag| tag.name.clone())
        .collect()
}

/// `Parameter: Option` for each value `before` had that `after` has no value for under a parameter of the same
/// name, ignoring case. For the message only: the server decides what is kept.
fn dropped_values(before: &ItemDto, after: &ItemDto) -> Vec<String> {
    let kept: HashSet<String> = after
        .parameters
        .iter()
        .filter(|parameter| parameter.option.is_some())
        .map(|parameter| parameter.name.to_lowercase())
        .collect();
    before
        .parameters
        .iter()
        .filter_map(|parameter| {
            let option = parameter.option.as_ref()?;
            (!kept.contains(&parameter.name.to_lowercase())).then(|| format!("{}: {}", parameter.name, option.label))
        })
        .collect()
}
