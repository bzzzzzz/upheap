//! `upheap heap add <name>`: add a heap to a team, or make one from a note.

use api_types::heaps::HeapDto;
use api_types::me::MeDto;
use api_types::teams::CreateHeapDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::heap_api::read_context;
use crate::commands::item::add::item_id_of;
use crate::commands::item::common::locate_item;
use crate::lookup::{HeapHandle, ItemName, find_personal_team};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The heap's name, 1 to 100 characters
    #[arg(id = "heap_name", value_name = "NAME")]
    pub name: String,
    /// The team it goes to, by slug. Left out, your personal team
    #[arg(long, value_name = "TEAM", conflicts_with = "from")]
    pub team: Option<String>,
    /// Make the heap from this note, which is too big for tasks: its key, team/heap/key or link. The heap goes to the
    /// note's team. Add its tasks and notes with `item add --from`, then close the note with `item became`
    #[arg(long, value_name = "ITEM")]
    pub from: Option<String>,
    /// The heap's context, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub context_file: Option<String>,
    /// The name in the heap's links and in team/heap: lowercase letters, digits and single dashes, starting with a
    /// letter, 2 to 39 characters. Left out, the server makes one from the name
    #[arg(long, value_name = "SLUG")]
    pub slug: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the heap with the name and slug as typed: the server checks them, and makes the slug when none is given,
/// so the output names the heap by the `team/heap` the server kept. With `--from` the heap is made from that note, in
/// the note's team. `--quiet` prints the handle alone and `--json` the created heap. The default heap stays what it
/// was.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let from_name = args.from.as_deref().map(ItemName::parse).transpose()?;
    let context = match &args.context_file {
        Some(path) => Some(read_context(path, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let (team, origin_id, from_key) = match (from_name, args.from.as_deref()) {
        (Some(name), Some(typed)) => {
            let at = locate_item(typed, name.clone(), None, app)?;
            let note_heap = at.heap.resolve(&client, &token).await?;
            let id = item_id_of(name, typed, &note_heap, &client, &token).await?;
            (note_heap.team, Some(id), Some(at.key))
        }
        _ => {
            let team = match args.team {
                Some(team) => team,
                None => {
                    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
                    find_personal_team(&me.value)?.to_owned()
                }
            };
            (team, None, None)
        }
    };
    let body = CreateHeapDto {
        name: args.name,
        slug: args.slug,
        context,
        origin_id,
    };
    let added = client
        .post::<HeapDto>(&["teams", &team, "heaps"], &body, &token)
        .await?;
    let handle = HeapHandle {
        team,
        heap: added.value.slug,
    };
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&handle.to_string())?,
        Mode::Human => match from_key {
            Some(key) => out.line(&format!("Added heap {handle}, made from {key}."))?,
            None => out.line(&format!("Added heap {handle}."))?,
        },
    }
    Ok(())
}
