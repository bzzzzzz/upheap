//! `upheap parameter list`: the heap's parameters in use and their options, from
//! `GET /teams/{team}/heaps/{heap}/parameters`.

use api_types::parameters::{ParameterDto, ParameterListDto};
use api_types::tags::{TagDto, TagListDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists the parameters in use with their options, highest priority first, and the tags each is for. Archived
/// parameters are left out, with a note on stderr. `--quiet` prints the names one per line and `--json` the
/// server's body.
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
                    api.get::<TagListDto>(&["tags"], NO_QUERY).await?.value.tags
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
        }
    }
    Ok(())
}

/// A parameter's name, its options in use, and, dim, the tags it is for.
fn row(parameter: &ParameterDto, tags: &[TagDto]) -> Vec<Cell> {
    let options = if parameter.options.is_empty() {
        Cell::toned("no options", Tone::Dim)
    } else {
        let labels: Vec<&str> = parameter.options.iter().map(|option| option.label.as_str()).collect();
        Cell::new(labels.join(", "))
    };
    let names: Vec<&str> = parameter
        .tag_ids
        .iter()
        .filter_map(|id| tags.iter().find(|tag| tag.id == *id))
        .map(|tag| tag.name.as_str())
        .collect();
    let tags = if names.is_empty() {
        Cell::default()
    } else {
        Cell::toned(format!("for {}", names.join(", ")), Tone::Dim)
    };
    vec![Cell::new(parameter.name.clone()), options, tags]
}

/// The footer that says archived parameters are left out, when there are any.
fn archived_note(archived: usize) -> Option<String> {
    match archived {
        0 => None,
        1 => Some("1 archived parameter isn't shown; --json lists it.".to_owned()),
        n => Some(format!("{n} archived parameters aren't shown; --json lists them.")),
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

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
