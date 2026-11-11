//! `upheap work claim [--heap <heap>]...`: take the next item and start a run on it.
//!
//! The plumbing under `work run`, public so a script can drive its own loop. The run belongs to the caller; the
//! command prints how to use it, and everything the agent then does is written with `UPHEAP_RUN` set.

use api_types::work::{ClaimDto, ClaimRequestDto};
use clap::Args;

use super::{HeapsArgs, journey, missing_agents, resolve_heaps};
use crate::commands::{SharedArgs, UsageError};
use crate::output::{Cell, Mode, Output, Tone};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ClaimArgs {
    #[command(flatten)]
    pub heaps: HeapsArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Claims the next item. A claimed run prints `Run <id>  k3x9a  Rotate the certs   Work   note → with context` and,
/// under it, `export UPHEAP_RUN=<id>`. `--quiet` prints the run ID, or nothing when nothing was claimed; `--json` the
/// server's answer. Exit 0 either way. Refused inside a run, since it would start another.
pub async fn run(args: ClaimArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if app.env.run.is_some() {
        return Err(UsageError::ClaimInRun.into());
    }
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heaps = resolve_heaps(&args.heaps, app, &client, &token).await?;
    let claimed = client
        .post::<ClaimDto>(&["work", "runs"], &ClaimRequestDto { heaps }, &token)
        .await
        .map_err(|err| missing_agents(err, None))?;
    match (out.mode(), &claimed.value.run) {
        (Mode::Json, _) => out.json(&claimed.body)?,
        (Mode::Quiet, Some(run)) => out.line(&run.id.to_string())?,
        (Mode::Quiet, None) => {}
        (Mode::Human, Some(run)) => {
            let item = &run.item;
            let row = vec![
                Cell::toned(format!("Run {}", run.id), Tone::Dim),
                Cell::toned(item.key.clone(), Tone::Strong),
                Cell::new(item.title.clone()),
                Cell::new(item.heap.name.clone()),
                Cell::new(journey(item)),
            ];
            for line in out.render_table(&[row]) {
                out.table_line("", &line)?;
            }
            out.line(&format!("export UPHEAP_RUN={}", run.id))?;
        }
        (Mode::Human, None) => match super::waiting_text(claimed.value.waiting_count) {
            Some(waiting) => out.line(&format!("Nothing to do. {waiting}"))?,
            None => out.line("Nothing to do.")?,
        },
    }
    Ok(())
}
