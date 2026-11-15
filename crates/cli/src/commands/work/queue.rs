//! `upheap work queue [--heap <heap>]...`: the agents' whole queue, in the order `next` and `claim` take it.
//!
//! Read-only, like `next`: it shows what agents will work on after the first item, and why each item is where it is.

use api_types::common::StateDto;
use api_types::work::WorkQueueDto;
use clap::Args;

use super::{HeapsArgs, journey, missing_agents, resolve_heaps, waiting_text};
use crate::commands::SharedArgs;
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct QueueArgs {
    #[command(flatten)]
    pub heaps: HeapsArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Prints one row per item of the queue: its place, `k3x9a  Rotate the certs  Work  note → with context`, and why it
/// is there: `#2 in heap, priority 3, started`, or `#1 in progress, …` for an item the heap's list pins. Nothing to
/// do prints how many items wait for you. `--quiet` prints the keys in order; `--json` the server's answer. Exit 0
/// either way.
pub async fn run(args: QueueArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heaps = resolve_heaps(&args.heaps, app, &client, &token).await?;
    let query = [("heaps", heaps.join(","))];
    let queue = client
        .get::<WorkQueueDto>(&["work", "queue"], &query, &token)
        .await
        .map_err(|err| missing_agents(err, None))?;
    match out.mode() {
        Mode::Json => out.json(&queue.body)?,
        Mode::Quiet => {
            for queued in &queue.value.items {
                out.line(&queued.item.key)?;
            }
        }
        Mode::Human if queue.value.items.is_empty() => match waiting_text(queue.value.waiting_count) {
            Some(waiting) => out.line(&format!("Nothing to do. {waiting}"))?,
            None => out.line("Nothing to do.")?,
        },
        Mode::Human => {
            let rows: Vec<Vec<Cell>> = queue
                .value
                .items
                .iter()
                .enumerate()
                .map(|(index, queued)| {
                    let item = &queued.item;
                    let group = if queued.state == StateDto::InProgress {
                        "in progress"
                    } else {
                        "in heap"
                    };
                    let started = if item.started { ", started" } else { "" };
                    let limited = if queued.limited {
                        ", cut off by the usage limit"
                    } else {
                        ""
                    };
                    vec![
                        Cell::new((index + 1).to_string()),
                        Cell::toned(item.key.clone(), Tone::Strong),
                        Cell::new(item.title.clone()),
                        Cell::new(item.heap.name.clone()),
                        Cell::new(journey(item)),
                        Cell::toned(
                            format!(
                                "#{} {group}, priority {}{started}{limited}",
                                queued.heap_position, queued.heap_priority
                            ),
                            Tone::Dim,
                        ),
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
