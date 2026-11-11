//! `upheap item move <item> <heap>`: move an item to another heap of its team.
//!
//! The server owns every rule: which heaps are allowed, the key, and what tags and values come along. The CLI only
//! notices an item that is in the heap already, which it has in hand from the read that gives it the version, and then
//! makes no write; and it words what the server dropped by comparing the item before and after.

use std::collections::HashSet;

use api_types::common::ItemDto;
use api_types::heaps::HeapDto;
use api_types::items::ItemMoveDto;
use clap::Args;

use super::common::locate_item;
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::{HeapName, ItemName};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct MoveArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// The heap to move it to: team/heap, or a heap slug only one of your teams has. It must be in the item's team
    #[arg(value_name = "HEAP")]
    pub to: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Moves the item to the heap, or says it is there already.
pub async fn run(args: MoveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let target_name = HeapName::parse(&args.to)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let target = resolve_heap_handle(target_name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&args.item, &at.key).await?;
    if target == heap {
        match out.mode() {
            Mode::Json => out.json(&item.body)?,
            Mode::Quiet => out.line(&item.value.key)?,
            Mode::Human => out.line(&format!("{} is already in {target}.", item.value.key))?,
        }
        return Ok(());
    }
    // The body names the heap by ID, which its path doesn't give.
    let target_api = HeapApi {
        client: &client,
        token: &token,
        heap: &target,
    };
    let target_heap = target_api.get::<HeapDto>(&[], NO_QUERY).await?;
    let body = ItemMoveDto {
        heap_id: target_heap.value.id,
    };
    let moved = api.post_item_action(&at.key, "move", item.value.version, &body).await?;
    match out.mode() {
        Mode::Json => out.json(&moved.body)?,
        Mode::Quiet => out.line(&moved.value.key)?,
        Mode::Human => {
            for line in moved_lines(&item.value, &moved.value, &target.to_string()) {
                out.line(&line)?;
            }
        }
    }
    Ok(())
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
