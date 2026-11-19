//! `upheap tag add <name>`: add a tag to a heap, or with `--team` to a team.

use api_types::tags::{CreateTagDto, TagDto};
use clap::Args;

use super::refusal::TeamTagWrite;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::config::SettingError;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The tag's name, 1 to 50 characters, not used by another tag of the heap, or with --team of the team, ignoring
    /// case
    #[arg(id = "tag_name", value_name = "NAME")]
    pub name: String,
    /// What the tag is for. `-` reads all of stdin
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,
    /// Add a team tag, which every heap of the team has: the team's slug. Heap tags of the team with this name merge
    /// into it
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    /// With --team, confirm the merge a refusal named: the token from its command line
    #[arg(long, value_name = "TOKEN", requires = "team")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the tag, with the name as typed: the server checks it. Human output and `--quiet` give the name the
/// server kept, and `--json` the created tag.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let heap_name = match &args.team {
        Some(_) => None,
        None => Some(named_heap(args.heap.name.as_deref(), app)?),
    };
    let description = match &args.description {
        Some(raw) => Some(read_description(raw, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let body = CreateTagDto {
        name: args.name.clone(),
        description,
    };
    let (added, line) = match (&args.team, heap_name) {
        (Some(team), _) => {
            let api = TeamApi {
                client: &client,
                token: &token,
                team,
            };
            let write = TeamTagWrite::Add {
                team,
                name: &args.name,
                description: args.description.as_deref(),
            };
            let confirm = args.confirm.as_deref();
            let added = match api.post_confirming::<TagDto>(&["tags"], &body, confirm).await {
                Ok(added) => added,
                Err(err) => return Err(write.refused(err, &api, app).await),
            };
            let line = format!("Added team tag \"{}\" to {team}.", added.value.name);
            (added, line)
        }
        (None, name) => {
            let heap = resolve_heap_handle(name.ok_or(SettingError::NoHeap)?, &client, &token).await?;
            let api = HeapApi {
                client: &client,
                token: &token,
                heap: &heap,
            };
            let added = api.post::<TagDto>(&["tags"], &body).await?;
            let line = format!("Added tag \"{}\".", added.value.name);
            (added, line)
        }
    };
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&added.value.name)?,
        Mode::Human => out.line(&line)?,
    }
    Ok(())
}
