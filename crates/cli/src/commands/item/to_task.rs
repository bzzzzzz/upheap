//! `upheap item to-task <item>`: make a note or a reminder a task, in place.
//!
//! The server owns the rule that a task never goes back. The CLI only notices an item that is already a task, which
//! it has in hand from the read that gives it the version, and then makes no write.

use api_types::common::KindDto;
use api_types::items::ItemPatchDto;

use super::common::locate_item;
use super::state::StateArgs;
use crate::commands::heap_api::HeapApi;
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

/// Makes the item a task, or says it is one already.
pub async fn run(args: StateArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
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
    if item.value.kind == KindDto::Task {
        match out.mode() {
            Mode::Json => out.json(&item.body)?,
            Mode::Quiet => {}
            Mode::Human => out.line(&format!("{} is already a task.", item.value.key))?,
        }
        return Ok(());
    }
    let patch = ItemPatchDto {
        kind: Some(KindDto::Task),
        ..ItemPatchDto::default()
    };
    let made = api.patch_item(&at.key, item.value.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&made.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("Made {} a task: {}.", made.value.key, made.value.title))?,
    }
    Ok(())
}
