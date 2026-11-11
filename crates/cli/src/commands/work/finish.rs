//! `upheap work finish [--cost-usd <n>] [--failure <text>]`: end the run, and record what it cost.
//!
//! What a loop calls when the agent tool exits. A run that already ended (it said reached, or the turn was taken
//! back) keeps its end; only the cost is recorded.

use api_types::work::{FinishRequestDto, RunDto, RunEndDto};
use clap::Args;

use super::{current_run, missing_agents};
use crate::commands::SharedArgs;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct FinishArgs {
    /// What the run cost, in US dollars
    #[arg(long, value_name = "USD")]
    pub cost_usd: Option<f64>,
    /// Why the run failed, if it ends without reaching the goal or asking a question
    #[arg(long, value_name = "TEXT")]
    pub failure: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Ends the run. Prints `Run ended: reached.`, `Run ended: asked.` or `Run ended: failed: <failure>.`, and the cost
/// when it is set, `($0.31)`; `--quiet` prints nothing and `--json` the run. Needs `UPHEAP_RUN`.
pub async fn run(args: FinishArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let run = current_run(app, "finish")?.to_owned();
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let body = FinishRequestDto {
        cost_usd: args.cost_usd,
        failure: args.failure,
    };
    let finished = client
        .post::<RunDto>(&["work", "runs", &run, "finish"], &body, &token)
        .await
        .map_err(|err| missing_agents(err, Some(&run)))?;
    match out.mode() {
        Mode::Json => out.json(&finished.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&ended_text(&finished.value))?,
    }
    Ok(())
}

fn ended_text(run: &RunDto) -> String {
    let mut text = match run.end {
        Some(RunEndDto::Reached) => "Run ended: reached.".to_owned(),
        Some(RunEndDto::Asked) => "Run ended: asked.".to_owned(),
        Some(RunEndDto::Failed) => match run.failure.as_deref() {
            Some(failure) => format!("Run ended: failed: {}.", failure.trim_end_matches('.')),
            None => "Run ended: failed.".to_owned(),
        },
        Some(RunEndDto::TakenBack) => "Run ended: taken back.".to_owned(),
        Some(RunEndDto::Unknown) | None => "Run ended.".to_owned(),
    };
    if let Some(cost) = run.cost_usd {
        text.push_str(&format!(" (${cost:.2})"));
    }
    text
}
