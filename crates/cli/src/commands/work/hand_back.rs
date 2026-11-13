//! `upheap work hand-back --reason <text>`: the run found nothing to do for the turn's goal and gives the turn back.
//!
//! It takes no item: the run knows it. The item's stage doesn't change and nothing opens after it; the item is the
//! person's again, and the run isn't counted as a failure.

use api_types::work::{HandBackRequestDto, RunDto};
use clap::Args;

use super::{current_run, missing_agents};
use crate::commands::SharedArgs;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct HandBackArgs {
    /// Why there was nothing to do, in a line or two
    #[arg(long, value_name = "TEXT")]
    pub reason: String,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Hands the turn back. Prints `Handed back: k3x9a is yours again.`; `--quiet` prints nothing and `--json` the run.
/// Needs `UPHEAP_RUN`.
pub async fn run(args: HandBackArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let run = current_run(app, "hand-back")?.to_owned();
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let body = HandBackRequestDto { reason: args.reason };
    let handed = client
        .post::<RunDto>(&["work", "runs", &run, "hand-back"], &body, &token)
        .await
        .map_err(|err| missing_agents(err, Some(&run)))?;
    match out.mode() {
        Mode::Json => out.json(&handed.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("Handed back: {} is yours again.", handed.value.item.key))?,
    }
    Ok(())
}
