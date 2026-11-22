//! `upheap work watch <pr-url>`: the run waits on its own pull request and ends, so its worker slot is free.
//!
//! It takes no item: the run knows it. The turn stays with the agent, out of the queue, until `work run` finds the
//! pull request has a new comment or review, a failed check or a merge conflict, or is merged or closed.

use api_types::work::{RunDto, WatchRequestDto};
use clap::Args;

use super::{current_run, missing_agents};
use crate::commands::SharedArgs;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// The run's own pull request: https://<host>/<owner>/<repo>/pull/<number>
    #[arg(value_name = "PR_URL")]
    pub url: String,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Ends the run by watching the pull request. Prints `Watching: k3x9a comes back when ...`; `--quiet` prints nothing
/// and `--json` the run. Needs `UPHEAP_RUN`.
pub async fn run(args: WatchArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let run = current_run(app, "watch")?.to_owned();
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let body = WatchRequestDto { url: args.url };
    let watching = client
        .post::<RunDto>(&["work", "runs", &run, "watch"], &body, &token)
        .await
        .map_err(|err| missing_agents(err, Some(&run)))?;
    match out.mode() {
        Mode::Json => out.json(&watching.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!(
            "Watching: {} comes back when the pull request gets a comment, a failed check or a merge conflict, or is merged or closed.",
            watching.value.item.key
        ))?,
    }
    Ok(())
}
