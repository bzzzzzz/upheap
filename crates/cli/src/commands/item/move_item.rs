//! `upheap item move <item>... <heap>`: move one or several items to another heap, of their team or any other team
//! the user may edit items in.
//!
//! The server owns every rule: which heaps are allowed, the key, and what tags and values come along. With one item,
//! the CLI notices an item that is in the heap already, which it has in hand from the read that gives it the version,
//! and then makes no write; and it words what the server dropped by comparing the item before and after.
//!
//! One item is read and moved with its own request, and its failure is the command's. Several are all read first, so
//! one that can't be read fails the command before anything moves, and then go in one request to the batch endpoint.
//! An item the server refuses is told on stderr as a single move's refusal would be, the rest carry on, and the
//! command exits with the code the first refusal would have given alone.

use std::collections::HashSet;

use api_types::common::ItemDto;
use api_types::heaps::HeapDto;
use api_types::items::{ItemMoveDto, ItemVersionDto, ItemsMoveDto, ItemsMovedDto};
use api_types::me::MeDto;
use clap::Args;

use super::common::{is_stale_or_closed, locate_item};
use crate::client::ApiClient;
use crate::client::ClientError;
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
    /// says which didn't; `--quiet` prints the new keys of those that did, and `--json` the server's answer, the items
    /// that moved and those that didn't
    #[arg(value_name = "ITEM", num_args = 1.., required = true)]
    pub items: Vec<String>,
    /// The heap to move them to: team/heap, or a heap slug only one of your teams has
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

/// Moves the items to the heap, or says they are there already.
pub async fn run(args: MoveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if let [item] = args.items.as_slice() {
        let moved = move_one(&args, item, app).await?;
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
    move_several(&args, app, out).await
}

/// Moves the item `typed` names to the heap `args` names, or finds it there already.
async fn move_one(args: &MoveArgs, typed: &str, app: &App) -> Result<Moved, CliError> {
    let name = ItemName::parse(typed)?;
    let at = locate_item(typed, name, args.heap.name.as_deref(), app)?;
    let target_name = HeapName::parse(&args.to)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let (client, token) = (&client, token.as_str());
    let heap = at.heap.resolve(client, token).await?;
    let target = resolve_target(target_name, &heap, client, token).await?;
    let api = HeapApi {
        client,
        token,
        heap: &heap,
    };
    let item = api.get_item(typed, &at.key).await?;
    if target == heap {
        return Ok(Moved {
            item,
            before: None,
            target: target.to_string(),
        });
    }
    let heap_id = target_id(&target, client, token).await?;
    let body = ItemMoveDto { heap_id };
    let after = api.post_item_action(&at.key, "move", item.value.version, &body).await?;
    Ok(Moved {
        item: after,
        before: Some(item.value),
        target: target.to_string(),
    })
}

/// The ID of the target heap: the body of a move names the heap by ID, which its path doesn't give.
async fn target_id(target: &HeapHandle, client: &ApiClient, token: &str) -> Result<uuid::Uuid, CliError> {
    let api = HeapApi {
        client,
        token,
        heap: target,
    };
    Ok(api.get::<HeapDto>(&[], NO_QUERY).await?.value.id)
}

/// Moves several items with one request. Everything that doesn't depend on an item is settled first, and every item
/// is read before any moves, so a failure of either fails the command with nothing moved. Every item is sent, one in
/// the target heap already too, which the server answers as moved and unchanged, so `--json` lists them all.
async fn move_several(args: &MoveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let target_name = HeapName::parse(&args.to)?;
    let mut located = Vec::new();
    for typed in &args.items {
        let name = ItemName::parse(typed)?;
        located.push((typed, locate_item(typed, name, args.heap.name.as_deref(), app)?));
    }
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let (client, token) = (&client, token.as_str());
    let mut places = Vec::new();
    for (typed, at) in located {
        places.push((typed, at.key, at.heap.resolve(client, token).await?));
    }
    let Some((_, _, first_heap)) = places.first() else {
        unreachable!("clap requires an item");
    };
    let target = resolve_target(target_name, first_heap, client, token).await?;
    let heap_id = target_id(&target, client, token).await?;

    // Each item as it was read, with the heap it was in, in the order given.
    let mut sent = Vec::new();
    for (typed, key, heap) in places {
        let item = HeapApi {
            client,
            token,
            heap: &heap,
        }
        .get_item(typed, &key)
        .await?;
        sent.push((item.value, heap));
    }
    let body = ItemsMoveDto {
        heap_id,
        items: sent
            .iter()
            .map(|(item, _)| ItemVersionDto {
                id: item.id,
                version: item.version,
            })
            .collect(),
    };
    let answer = client
        .post::<ItemsMovedDto>(&["teams", target.team.as_str(), "items", "move"], &body, token)
        .await?;
    if out.mode() == Mode::Json {
        out.json(&answer.body)?;
    }

    let in_run = app.env.run.is_some();
    let target_text = target.to_string();
    let mut first_failure = None;
    let mut moved_count = 0;
    // The server answers both lists in the order of the request, so each item takes the next answer for its ID: an
    // item named twice is moved once and refused once, as it was.
    let mut moved = answer.value.moved.iter().peekable();
    let mut not_moved = answer.value.not_moved.iter().peekable();
    for (before, heap) in &sent {
        if let Some(after) = moved.next_if(|after| after.id == before.id) {
            moved_count += 1;
            // A move changes the item, so one the server left as it was is in the heap already, whichever handle
            // named it.
            let lines = if after.version == before.version {
                vec![format!("{} is already in {target_text}.", after.key)]
            } else {
                moved_lines(before, after, &target_text)
            };
            match out.mode() {
                Mode::Json => {}
                Mode::Quiet => out.line(&after.key)?,
                Mode::Human => {
                    let mut lines = lines.into_iter();
                    if let Some(first) = lines.next() {
                        out.line(&first)?;
                    }
                    for line in lines {
                        out.line(&format!("  {line}"))?;
                    }
                }
            }
        } else if let Some(refused) = not_moved.next_if(|refused| refused.id == before.id) {
            let err = refusal(refused, heap, &before.key)?;
            first_failure.get_or_insert(err.exit_code());
            // Told on stderr in every mode but JSON, whose body on stdout already holds it, as a single move's
            // refusal is told.
            if out.mode() != Mode::Json {
                let mut message = err.to_message_in(in_run).into_iter();
                if let Some(first) = message.next() {
                    let first = first.strip_prefix("error: ").unwrap_or(&first);
                    out.note(&format!("Not moved {}: {first}", before.key))?;
                }
                for line in message {
                    out.note(&format!("  {line}"))?;
                }
            }
        }
    }
    if out.mode() == Mode::Human {
        out.line(&format!("Moved {moved_count} of {} items.", sent.len()))?;
    }
    match first_failure {
        Some(exit_code) => Err(CliError::Reported { exit_code }),
        None => Ok(()),
    }
}

/// The refusal of one item in a batch, as the failure of a single move of it would be.
fn refusal(refused: &api_types::items::ItemNotMovedDto, heap: &HeapHandle, key: &str) -> Result<CliError, CliError> {
    let body = serde_json::to_string(&refused.problem).map_err(std::io::Error::other)?;
    let problem = serde_json::from_str(&body).map_err(std::io::Error::other)?;
    let source = ClientError::Problem {
        problem: Box::new(problem),
        body,
    };
    Ok(if is_stale_or_closed(&source) {
        CliError::ItemRefused {
            name: format!("{heap}/{key}"),
            source,
        }
    } else {
        source.into()
    })
}

/// The heap to move to. A bare slug is looked for in the item's team first; when that team has no such heap it is
/// resolved among all the user's teams as usual.
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
