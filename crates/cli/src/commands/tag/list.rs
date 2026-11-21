//! `upheap tag list`: the heap's tags and its team's, by name, or with `--team` the team's alone.

use api_types::tags::{TagDto, TagListDto, TeamTagListDto};
use clap::Args;
use domain::tag::TagColor;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, indented_context, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::config::SettingError;
use crate::output::{Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    /// List this team's tags alone, instead of a heap's and its team's: the team's slug
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists the tags, each name followed by its colour and its whole description, indented, so a reader can tell what each tag is for
/// without `--json`. In a heap the team's tags come in the same list by name, marked `team`, after the colour. `--quiet` prints the
/// names one per line and `--json` the server's body.
pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let heap_name = match &args.team {
        Some(_) => None,
        None => Some(named_heap(args.heap.name.as_deref(), app)?),
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    if let Some(team) = &args.team {
        let api = TeamApi {
            client: &client,
            token: &token,
            team,
        };
        let fetched = api.get::<TeamTagListDto>(&["tags"]).await?;
        let tags: Vec<(&TagDto, bool)> = fetched.value.tags.iter().map(|tag| (tag, false)).collect();
        let empty = format!("No team tags in {team} yet; add one with `upheap tag add <name> --team {team}`.");
        return print(&tags, &fetched.body, &empty, out);
    }
    let heap = resolve_heap_handle(heap_name.ok_or(SettingError::NoHeap)?, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let fetched = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    let mut tags: Vec<(&TagDto, bool)> = fetched
        .value
        .tags
        .iter()
        .map(|tag| (tag, false))
        .chain(fetched.value.team_tags.iter().map(|tag| (tag, true)))
        .collect();
    tags.sort_by_cached_key(|(tag, _)| tag.name.to_lowercase());
    let empty = format!("No tags in {heap} yet; add one with `upheap tag add <name>`.");
    print(&tags, &fetched.body, &empty, out)
}

/// The tags, each with whether it is a team tag to mark, or `empty` when there are none.
fn print(tags: &[(&TagDto, bool)], body: &str, empty: &str, out: &mut Output) -> Result<(), CliError> {
    match out.mode() {
        Mode::Json => out.json(body)?,
        Mode::Quiet => {
            for (tag, _) in tags {
                out.line(&tag.name)?;
            }
        }
        Mode::Human if tags.is_empty() => out.line(empty)?,
        Mode::Human => {
            for (tag, team) in tags {
                // A colour a newer server added is `?`.
                let color = TagColor::try_from(tag.color).map_or("?", TagColor::code);
                let mut parts = vec![
                    (tag.name.as_str(), Tone::Strong),
                    ("  ", Tone::Plain),
                    (color, Tone::Dim),
                ];
                if *team {
                    parts.extend([("  ", Tone::Plain), ("team", Tone::Dim)]);
                }
                out.line_parts(&parts)?;
                for line in indented_context(tag.description.as_deref().unwrap_or_default()) {
                    out.line(&line)?;
                }
            }
        }
    }
    Ok(())
}
