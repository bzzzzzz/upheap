//! `upheap comment list <item>`: the comments on an item, oldest first.
//!
//! One `GET` of the item, which carries its comments; the command prints only those.

use clap::Args;

use super::comment_lines;
use crate::commands::heap_api::HeapApi;
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints the comments. `--quiet` prints their names, one per line, and `--json` the item as the server sent it.
pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&args.item, &at.key).await?;
    let item_key = &item.value.key;
    match out.mode() {
        Mode::Json => out.json(&item.body)?,
        Mode::Quiet => {
            for comment in item.value.comments.iter().flatten() {
                out.line(&format!("{item_key}.c{}", comment.number))?;
            }
        }
        Mode::Human => {
            if item.value.comments.as_deref().is_none_or(<[_]>::is_empty) {
                out.line(&format!("No comments on {item_key}."))?;
            }
            for line in comment_lines(&item.value) {
                out.line(&line)?;
            }
        }
    }
    Ok(())
}
