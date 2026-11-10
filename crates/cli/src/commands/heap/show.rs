//! `upheap heap show [<heap>]`: a heap's name and its context.

use api_types::heaps::HeapDto;
use api_types::me::MeDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::commands::heap_api::{HeapApi, indented_context, named_heap};
use crate::lookup::resolve_heap;
use crate::output::{Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// team/heap, or a heap slug only one of your teams has. Without it, the default heap
    #[arg(value_name = "HEAP")]
    pub heap: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints the heap's name, its team and its context. `--json` prints the heap as the API returns it, and `--quiet`
/// nothing.
pub async fn run(args: ShowArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
    let handle = resolve_heap(name, &me.value)?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &handle,
    };
    let heap = api.get::<HeapDto>(&[], NO_QUERY).await?;
    match out.mode() {
        Mode::Json => out.json(&heap.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let team = me
                .value
                .teams
                .iter()
                .find(|team| team.slug == handle.team)
                .map_or(handle.team.as_str(), |team| team.name.as_str());
            render(&heap.value, team, &handle.to_string(), out)?;
        }
    }
    Ok(())
}

fn render(heap: &HeapDto, team: &str, handle: &str, out: &mut Output) -> std::io::Result<()> {
    let archived = if heap.archived_at.is_some() { " · archived" } else { "" };
    out.line_toned(&format!("{} · {team}{archived}", heap.name), Tone::Strong)?;
    out.line("")?;
    out.line_toned("Context", Tone::Strong)?;
    match heap.context.as_deref().filter(|text| !text.is_empty()) {
        Some(context) => {
            for line in indented_context(context) {
                out.line(&line)?;
            }
        }
        None => out.line_toned(
            &format!("  None. Write one with: upheap heap edit {handle} --context-file <file>"),
            Tone::Dim,
        )?,
    }
    Ok(())
}
