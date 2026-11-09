//! `upheap tag list`: the heap's tags, by name, from `GET /teams/{team}/heaps/{heap}/tags`.

use api_types::tags::TagListDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Cell, Mode, Output, Tone, shorten};
use crate::{App, CliError};

/// Descriptions longer than this are cut short in the table.
const DESCRIPTION_WIDTH: usize = 60;

#[derive(Args, Debug)]
pub struct ListArgs {
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists the tags with the first line of each description. `--quiet` prints the names one per line and `--json` the
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
            let rows: Vec<Vec<Cell>> = tags
                .iter()
                .map(|tag| {
                    let description = tag.description.as_deref().map(first_line).unwrap_or_default();
                    vec![
                        Cell::new(tag.name.clone()),
                        Cell::toned(shorten(description, DESCRIPTION_WIDTH), Tone::Dim),
                    ]
                })
                .collect();
            for line in out.render_table(&rows) {
                out.table_line("", &line)?;
            }
        }
    }
    Ok(())
}

/// The text up to its first newline, less a `\r` before it.
fn first_line(text: &str) -> &str {
    let line = text.split('\n').next().unwrap_or_default();
    line.strip_suffix('\r').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn only_the_first_line_of_a_description_is_shown() {
        expect_that!(first_line("Servers\r\nand the network"), eq("Servers"));
        expect_that!(first_line("One line"), eq("One line"));
        expect_that!(first_line(""), eq(""));
        expect_that!(first_line("\nSecond"), eq(""));
    }
}
