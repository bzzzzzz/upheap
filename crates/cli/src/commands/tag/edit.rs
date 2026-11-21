//! `upheap tag edit <tag>`: rename a tag or change its description or colour.

use api_types::tags::{TagDto, TagKindDto, TagListDto, TagPatchDto, TeamTagListDto};
use clap::Args;
use domain::tag::TagColor;

use super::color_parser;
use super::refusal::{EditScope, TeamTagWrite};
use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::{find_tag, find_team_tag};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The tag: its name, ignoring case, or its ID
    #[arg(value_name = "TAG")]
    pub tag: String,
    /// A new name, 1 to 50 characters, not used by another tag of the heap, or for a team tag of the team, ignoring
    /// case
    #[arg(long, id = "new_name", value_name = "NAME")]
    pub name: Option<String>,
    /// A new description. `-` reads all of stdin
    #[arg(long, value_name = "TEXT", conflicts_with = "no_description")]
    pub description: Option<String>,
    /// Clear the description
    #[arg(long)]
    pub no_description: bool,
    /// A new colour
    #[arg(long, value_name = "COLOR", value_parser = color_parser())]
    pub color: Option<TagColor>,
    /// Find the tag among this team's tags instead of a heap's: the team's slug
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    /// For a team tag, confirm the merge a refusal named: the token from its command line
    #[arg(long, value_name = "TOKEN")]
    pub confirm: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

impl EditArgs {
    /// Whether any field flag is given.
    fn has_changes(&self) -> bool {
        self.name.is_some() || self.description.is_some() || self.no_description || self.color.is_some()
    }
}

/// Changes the fields given and leaves the rest. A team tag changes in every heap of its team. Human output says
/// whether the tag was renamed, updated or left as it was; `--quiet` prints nothing and `--json` the edited tag.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if !args.has_changes() {
        if out.mode() == Mode::Human {
            out.line("Nothing to change: give --name, --description, --no-description or --color.")?;
        }
        return Ok(());
    }
    let heap_name = match &args.team {
        Some(_) => None,
        None => Some(named_heap(args.heap.name.as_deref(), app)?),
    };
    let description = match (&args.description, args.no_description) {
        (Some(raw), _) => Some(Some(read_description(raw, app).await?)),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let patch = TagPatchDto {
        name: args.name.clone(),
        description,
        color: args.color.map(Into::into),
    };
    if let Some(team) = &args.team {
        let api = TeamApi {
            client: &client,
            token: &token,
            team,
        };
        let tags = api.get::<TeamTagListDto>(&["tags"]).await?;
        let tag = find_team_tag(&args.tag, &tags.value, team)?;
        return edit_team_tag(&args, tag, EditScope::Team(team), &patch, &api, app, out).await;
    }
    let name = heap_name.ok_or(crate::config::SettingError::NoHeap)?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    let tag = find_tag(&args.tag, &tags.value, &heap)?;
    if tag.kind == TagKindDto::Team {
        let team_api = TeamApi {
            client: &client,
            token: &token,
            team: &heap.team,
        };
        return edit_team_tag(&args, tag, EditScope::Heap(&heap), &patch, &team_api, app, out).await;
    }
    let id = tag.id.to_string();
    let edited = api.patch::<TagDto>(&["tags", &id], tag.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&edit_line(tag, &edited.value))?,
    }
    Ok(())
}

/// The edit of a team tag, which the server may refuse until a merge is confirmed.
async fn edit_team_tag(
    args: &EditArgs,
    tag: &TagDto,
    scope: EditScope<'_>,
    patch: &TagPatchDto,
    api: &TeamApi<'_>,
    app: &App,
    out: &mut Output,
) -> Result<(), CliError> {
    let write = TeamTagWrite::Edit {
        tag: &args.tag,
        before: &tag.name,
        team: api.team,
        scope,
        name: args.name.as_deref(),
        description: args.description.as_deref(),
        no_description: args.no_description,
        color: args.color,
    };
    let id = tag.id.to_string();
    let edited = match api
        .patch_confirming::<TagDto>(&["tags", &id], tag.version, patch, args.confirm.as_deref())
        .await
    {
        Ok(edited) => edited,
        Err(err) => return Err(write.refused(err, api, app).await),
    };
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&team_edit_line(tag, &edited.value, api.team))?,
    }
    Ok(())
}

/// What the edit did, from the tag as read and as the server returned it.
fn edit_line(before: &TagDto, after: &TagDto) -> String {
    if after.version == before.version {
        format!("No change to tag \"{}\".", after.name)
    } else if after.name != before.name {
        format!("Renamed tag \"{}\" to \"{}\".", before.name, after.name)
    } else {
        format!("Updated tag \"{}\".", after.name)
    }
}

/// [`edit_line`] for a team tag, which changes in every heap of `team`.
fn team_edit_line(before: &TagDto, after: &TagDto, team: &str) -> String {
    if after.version == before.version {
        format!("No change to tag \"{}\".", after.name)
    } else if after.name != before.name {
        format!(
            "Renamed team tag \"{}\" to \"{}\" in every heap of {team}.",
            before.name, after.name
        )
    } else {
        format!("Updated team tag \"{}\" in every heap of {team}.", after.name)
    }
}
