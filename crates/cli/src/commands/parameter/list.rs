//! `upheap parameter list`: the heap's parameters in use and their options, from
//! `GET /teams/{team}/heaps/{heap}/parameters`, or with `--team` the team's, from `GET /teams/{team}/parameters`.

use api_types::parameters::{OptionDto, ParameterDto, ParameterKindDto, ParameterListDto, TeamParameterListDto};
use api_types::tags::{TagDto, TagListDto, TeamTagListDto};
use clap::Args;
use uuid::Uuid;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, TeamApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::config::SettingError;
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    /// List this team's parameters instead of a heap's: the team's slug
    #[arg(long, value_name = "TEAM", conflicts_with = "name")]
    pub team: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists the parameters in use with their options, highest priority first, and the tags each is for. In a heap the
/// team's parameters it uses come in the same list, marked `team`, and a note names the team's others it could
/// use. Archived parameters are left out, with a note on stderr. `--quiet` prints the names one per line and
/// `--json` the server's body.
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
        let fetched = api.get::<TeamParameterListDto>(&["parameters"]).await?;
        let list = &fetched.value;
        match out.mode() {
            Mode::Json => out.json(&fetched.body)?,
            Mode::Quiet => {
                for parameter in &list.parameters {
                    out.line(&parameter.name)?;
                }
            }
            Mode::Human => {
                if list.parameters.is_empty() {
                    out.line(&format!(
                        "No team parameters in {team} yet; add one with `upheap parameter add <name> --team {team}`."
                    ))?;
                } else {
                    // The tag names cost a request, so only a parameter linked to tags asks for them.
                    let tags = if list.parameters.iter().any(|parameter| !parameter.tag_ids.is_empty()) {
                        api.get::<TeamTagListDto>(&["tags"]).await?.value.tags
                    } else {
                        Vec::new()
                    };
                    let rows: Vec<Vec<Cell>> = list
                        .parameters
                        .iter()
                        .map(|p| {
                            vec![
                                Cell::new(p.name.clone()),
                                options_cell(&p.options),
                                for_tags_cell(&p.tag_ids, &tags),
                            ]
                        })
                        .collect();
                    for line in out.render_table(&rows) {
                        out.table_line("", &line)?;
                    }
                }
                if let Some(note) = archived_note(list.archived.len()) {
                    out.note(&note)?;
                }
            }
        }
        return Ok(());
    }
    let heap = resolve_heap_handle(heap_name.ok_or(SettingError::NoHeap)?, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let fetched = api.get::<ParameterListDto>(&["parameters"], NO_QUERY).await?;
    let list = &fetched.value;
    match out.mode() {
        Mode::Json => out.json(&fetched.body)?,
        Mode::Quiet => {
            for parameter in &list.parameters {
                out.line(&parameter.name)?;
            }
        }
        Mode::Human => {
            if list.parameters.is_empty() {
                out.line(&format!("No parameters in {heap} yet."))?;
            } else {
                // The tag names cost a request, so only a parameter linked to tags asks for them.
                let tags = if list.parameters.iter().any(|parameter| !parameter.tag_ids.is_empty()) {
                    let listed = api.get::<TagListDto>(&["tags"], NO_QUERY).await?.value;
                    listed.tags.into_iter().chain(listed.team_tags).collect()
                } else {
                    Vec::new()
                };
                let rows: Vec<Vec<Cell>> = list.parameters.iter().map(|p| row(p, &tags)).collect();
                for line in out.render_table(&rows) {
                    out.table_line("", &line)?;
                }
            }
            if let Some(note) = archived_note(list.archived.len()) {
                out.note(&note)?;
            }
            if let Some(note) = available_note(&list.team_available.iter().map(|p| p.name.as_str()).collect::<Vec<_>>())
            {
                out.note(&note)?;
            }
        }
    }
    Ok(())
}

/// The options in use, or, dim, that there are none.
fn options_cell(options: &[OptionDto]) -> Cell {
    if options.is_empty() {
        Cell::toned("no options", Tone::Dim)
    } else {
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        Cell::new(labels.join(", "))
    }
}

/// `for` and the names of the tags among `tags` that `ids` has, or `None` when there are none.
fn for_tags(ids: &[Uuid], tags: &[TagDto]) -> Option<String> {
    let names: Vec<&str> = ids
        .iter()
        .filter_map(|id| tags.iter().find(|tag| tag.id == *id))
        .map(|tag| tag.name.as_str())
        .collect();
    (!names.is_empty()).then(|| format!("for {}", names.join(", ")))
}

fn for_tags_cell(ids: &[Uuid], tags: &[TagDto]) -> Cell {
    for_tags(ids, tags).map_or_else(Cell::default, |text| Cell::toned(text, Tone::Dim))
}

/// A parameter's name, its options in use, and, dim, the tags it is for, after `team` for a team parameter. A team
/// parameter's tags are its team's and the heap's own.
fn row(parameter: &ParameterDto, tags: &[TagDto]) -> Vec<Cell> {
    let tags = if parameter.kind == ParameterKindDto::Team {
        let text = match for_tags(&parameter.tag_ids, tags) {
            Some(names) => format!("team, {names}"),
            None => "team".to_owned(),
        };
        Cell::toned(text, Tone::Dim)
    } else {
        for_tags_cell(&parameter.tag_ids, tags)
    };
    vec![
        Cell::new(parameter.name.clone()),
        options_cell(&parameter.options),
        tags,
    ]
}

/// The footer that says archived parameters are left out, when there are any.
fn archived_note(archived: usize) -> Option<String> {
    match archived {
        0 => None,
        1 => Some("1 archived parameter isn't shown; --json lists it.".to_owned()),
        n => Some(format!("{n} archived parameters aren't shown; --json lists them.")),
    }
}

/// The footer that names the team's parameters the heap could use, when there are any.
fn available_note(names: &[&str]) -> Option<String> {
    if names.is_empty() {
        None
    } else {
        Some(format!(
            "Team parameters this heap could use: {}; start with `upheap parameter use <name>`.",
            names.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_available_note_names_the_team_parameters_to_use() {
        expect_that!(available_note(&[]), none());
        expect_that!(
            available_note(&["Tier", "Region"]),
            some(eq(
                "Team parameters this heap could use: Tier, Region; start with `upheap parameter use <name>`."
            ))
        );
    }

    #[gtest]
    fn the_footer_counts_the_archived_parameters_left_out() {
        expect_that!(archived_note(0), none());
        expect_that!(
            archived_note(1),
            some(eq("1 archived parameter isn't shown; --json lists it."))
        );
        expect_that!(
            archived_note(3),
            some(eq("3 archived parameters aren't shown; --json lists them."))
        );
    }
}
