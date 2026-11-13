//! `upheap item became <item>`: close a note as became, because items were made from it with `item add --from`.
//!
//! The server owns the rules: that the item is a note, that something was made from it, and, in a run, that every
//! open task made from it is ready, which also reaches the run's turn. The CLI only says what happened.

use api_types::common::StateDto;

use super::state::StateArgs;
use crate::commands::heap_api::HeapApi;
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

use super::common::locate_item;

/// Closes the note as became. In a run, the turn is reached with it.
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
    let note = api.became_item(&args.item, &at.key).await?;
    match out.mode() {
        Mode::Json => out.json(&note.body)?,
        Mode::Quiet => out.line(&note.value.key)?,
        Mode::Human => {
            out.line(&became_line(&note.value))?;
            if app.env.run.is_some() && note.value.state == StateDto::Became {
                out.line("The turn is reached.")?;
            }
        }
    }
    Ok(())
}

/// `k3x9a "Certs are a mess" became 2 items.`
fn became_line(note: &api_types::common::ItemDto) -> String {
    let made = note.became.as_deref().map_or(0, <[_]>::len);
    let noun = if made == 1 { "item" } else { "items" };
    format!("{} \"{}\" became {made} {noun}.", note.key, note.title)
}
