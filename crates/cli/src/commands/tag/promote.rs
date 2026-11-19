//! `upheap tag promote <tag>`: make a heap's tag a team tag, which every heap of the team has.

use api_types::tags::{CreateTagDto, TagDto, TagListDto};
use clap::Args;

use super::refusal::TeamTagWrite;
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::find_tag;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct PromoteArgs {
    /// The heap tag: its name, ignoring case, or its ID
    #[arg(value_name = "TAG")]
    pub tag: String,
    /// Confirm the merge a refusal named: the token from its command line
    #[arg(long, value_name = "TOKEN")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Makes a team tag of the tag's name and description. The heap tags of the team with that name merge into it, so
/// the first try is refused with what it would merge. Human output and `--quiet` give the name the server kept, and
/// `--json` the created team tag.
pub async fn run(args: PromoteArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let heap_api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let tags = heap_api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    let tag = find_tag(&args.tag, &tags.value, &heap)?;
    let body = CreateTagDto {
        name: tag.name.clone(),
        description: tag.description.clone(),
    };
    let api = TeamApi {
        client: &client,
        token: &token,
        team: &heap.team,
    };
    let write = TeamTagWrite::Promote {
        tag: &args.tag,
        tag_name: &tag.name,
        heap: &heap,
    };
    let promoted = match api
        .post_confirming::<TagDto>(&["tags"], &body, args.confirm.as_deref())
        .await
    {
        Ok(promoted) => promoted,
        Err(err) => return Err(write.refused(err, &api, app).await),
    };
    match out.mode() {
        Mode::Json => out.json(&promoted.body)?,
        Mode::Quiet => out.line(&promoted.value.name)?,
        Mode::Human => out.line(&format!(
            "Promoted \"{}\" to a team tag of {}.",
            promoted.value.name, heap.team
        ))?,
    }
    Ok(())
}
