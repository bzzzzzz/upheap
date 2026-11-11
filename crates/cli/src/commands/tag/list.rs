//! `upheap tag list`: the heap's tags, by name, from `GET /teams/{team}/heaps/{heap}/tags`.

use api_types::tags::TagListDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, indented_context, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists the tags, each name followed by its whole description, indented, so a reader can tell what each tag is for
/// without `--json`. `--quiet` prints the names one per line and `--json` the server's body.
pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let fetched = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    let tags = &fetched.value.tags;
    match out.mode() {
        Mode::Json => out.json(&fetched.body)?,
        Mode::Quiet => {
            for tag in tags {
                out.line(&tag.name)?;
            }
        }
        Mode::Human if tags.is_empty() => {
            out.line(&format!("No tags in {heap} yet; add one with `upheap tag add <name>`."))?;
        }
        Mode::Human => {
            for tag in tags {
                out.line_toned(&tag.name, Tone::Strong)?;
                for line in indented_context(tag.description.as_deref().unwrap_or_default()) {
                    out.line(&line)?;
                }
            }
        }
    }
    Ok(())
}
