//! `upheap heap list`: every heap the caller can use, by team, from `GET /me`.

use api_types::me::MeDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::lookup::{HeapHandle, HeapName, resolve_heap};
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ListArgs {
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Lists each team with its heaps, marking archived heaps and the default one with `*`. `--quiet` prints the
/// heaps' `team/heap` handles one per line.
pub async fn run(args: ListArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
    let teams = &me.value.teams;
    match out.mode() {
        Mode::Json => out.json(&me.body)?,
        Mode::Quiet => {
            for team in teams {
                for heap in &team.heaps {
                    out.line(&format!("{}/{}", team.slug, heap.heap.slug))?;
                }
            }
        }
        Mode::Human => {
            // A default that names no heap of the caller's marks nothing; commands that use it say why.
            let default = app
                .heap_name(None)
                .and_then(|name| HeapName::parse(name).ok())
                .and_then(|name| resolve_heap(name, &me.value).ok());
            render(&me.value, default.as_ref(), out)?;
        }
    }
    Ok(())
}

fn render(me: &MeDto, default: Option<&HeapHandle>, out: &mut Output) -> std::io::Result<()> {
    for (index, team) in me.teams.iter().enumerate() {
        if index > 0 {
            out.line("")?;
        }
        let personal = if team.personal { ", personal" } else { "" };
        let heading = format!("{} ({}{personal})", team.name, team.slug);
        out.line_toned(&heading, Tone::Strong)?;
        if team.heaps.is_empty() {
            out.line_toned("  no heaps", Tone::Dim)?;
            continue;
        }
        let is_default = |slug: &str| default.is_some_and(|d| d.team == team.slug && d.heap == slug);
        let rows: Vec<Vec<Cell>> = team
            .heaps
            .iter()
            .map(|heap| {
                let tone = if is_default(&heap.heap.slug) {
                    Tone::Strong
                } else {
                    Tone::Plain
                };
                vec![
                    Cell::toned(format!("{}/{}", team.slug, heap.heap.slug), tone),
                    Cell::new(heap.heap.name.clone()),
                    Cell::toned(format!("{} open", heap.open_count), Tone::Dim),
                    Cell::toned(if heap.heap.archived { "archived" } else { "" }, Tone::Dim),
                ]
            })
            .collect();
        for (heap, line) in team.heaps.iter().zip(out.render_table(&rows)) {
            let mark = if is_default(&heap.heap.slug) { "* " } else { "  " };
            out.table_line(mark, &line)?;
        }
    }
    Ok(())
}
