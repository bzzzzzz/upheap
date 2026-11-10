//! `upheap heap edit [<heap>]`: write or clear a heap's context.

use api_types::heaps::{HeapDto, HeapPatchDto};
use api_types::me::MeDto;
use clap::{ArgGroup, Args};

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::heap_api::{HeapApi, named_heap, read_context};
use crate::lookup::resolve_heap;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
#[command(group(ArgGroup::new("change").required(true).args(["context_file", "no_context"])))]
pub struct EditArgs {
    /// team/heap, or a heap slug only one of your teams has. Without it, the default heap
    #[arg(value_name = "HEAP")]
    pub heap: Option<String>,
    /// The heap's context, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub context_file: Option<String>,
    /// Clear the heap's context
    #[arg(long)]
    pub no_context: bool,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Sets the context from the file as read (the server trims it, so an empty file clears it), or clears it. Human
/// output says whether anything changed; `--quiet` prints nothing and `--json` the heap.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.as_deref(), app)?;
    let context = match &args.context_file {
        Some(path) => Some(read_context(path, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
    let handle = resolve_heap(name, &me.value)?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &handle,
    };
    let current = api.get::<HeapDto>(&[], NO_QUERY).await?;
    let patch = HeapPatchDto {
        context: Some(context),
        ..HeapPatchDto::default()
    };
    let edited = api.patch::<HeapDto>(&[], current.value.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human if edited.value.version == current.value.version => {
            out.line(&format!("No change to {handle}."))?;
        }
        Mode::Human => out.line(&format!("Updated the context of {handle}."))?,
    }
    Ok(())
}
