//! `upheap tag add <name>`: add a tag to a heap.

use api_types::tags::{CreateTagDto, TagDto};
use clap::Args;

use crate::commands::heap_api::{HeapApi, named_heap, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The tag's name, 1 to 50 characters, not used by another tag of the heap ignoring case
    #[arg(id = "tag_name", value_name = "NAME")]
    pub name: String,
    /// What the tag is for. `-` reads all of stdin
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the tag, with the name as typed: the server checks it. Human output and `--quiet` give the name the
/// server kept, and `--json` the created tag.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let description = match &args.description {
        Some(raw) => Some(read_description(raw, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let body = CreateTagDto {
        name: args.name,
        description,
    };
    let added = api.post::<TagDto>(&["tags"], &body).await?;
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&added.value.name)?,
        Mode::Human => out.line(&format!("Added tag \"{}\".", added.value.name))?,
    }
    Ok(())
}
