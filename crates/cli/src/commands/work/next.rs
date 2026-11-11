//! `upheap work next [--heap <heap>]...`: the item an agent would work on next, without taking it.
//!
//! No claim and no run: a script that wants to look before it takes uses this, and a person can check that what is
//! "next" follows the heap's ranking.

use api_types::work::WorkNextDto;
use clap::Args;

use super::{HeapsArgs, journey, missing_agents, resolve_heaps, waiting_text};
use crate::commands::SharedArgs;
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct NextArgs {
    #[command(flatten)]
    pub heaps: HeapsArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints the first item of the queue: `k3x9a  Rotate the certs   Work   note → with context`, with `started, ready
/// to go on` when the turn has had runs. Nothing to do prints how many items wait for you. `--quiet` prints the key,
/// or nothing; `--json` the server's answer. Exit 0 either way.
pub async fn run(args: NextArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heaps = resolve_heaps(&args.heaps, app, &client, &token).await?;
    let query = [("heaps", heaps.join(","))];
    let next = client
        .get::<WorkNextDto>(&["work", "next"], &query, &token)
        .await
        .map_err(|err| missing_agents(err, None))?;
    match (out.mode(), &next.value.item) {
        (Mode::Json, _) => out.json(&next.body)?,
        (Mode::Quiet, Some(item)) => out.line(&item.key)?,
        (Mode::Quiet, None) => {}
        (Mode::Human, Some(item)) => {
            let mut row = vec![
                Cell::toned(item.key.clone(), Tone::Strong),
                Cell::new(item.title.clone()),
                Cell::new(item.heap.name.clone()),
                Cell::new(journey(item)),
            ];
            if item.runs > 0 {
                row.push(Cell::toned("started, ready to go on", Tone::Dim));
            }
            for line in out.render_table(&[row]) {
                out.table_line("", &line)?;
            }
        }
        (Mode::Human, None) => match waiting_text(next.value.waiting_count) {
            Some(waiting) => out.line(&format!("Nothing to do. {waiting}"))?,
            None => out.line("Nothing to do.")?,
        },
    }
    Ok(())
}
