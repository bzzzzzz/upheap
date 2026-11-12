//! `upheap work reached --result <text> [--link <url>]...`: the run says it reached the turn's goal.
//!
//! It takes no item: the run knows it. The server checks the item as it is now; when it misses a condition the
//! refusal names it, and the run, still live, can fix the item and say reached again.

use api_types::turns::{GoalDto, ReachDto};
use api_types::work::{ReachedRequestDto, RunDto};
use clap::Args;

use super::{current_run, goal_phrase, missing_agents};
use crate::commands::SharedArgs;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ReachedArgs {
    /// What the run did, in a line or two
    #[arg(long, value_name = "TEXT")]
    pub result: String,
    /// A link to what it made, such as a pull request. Repeat for more, up to 10
    #[arg(long = "link", value_name = "URL")]
    pub links: Vec<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Says the run reached its goal. Prints `Reached: with context. k3x9a is yours again.`, or `... k3x9a goes on, all the
/// way.` when the turn was sent all the way and didn't reach done; `--quiet` prints nothing and
/// `--json` the run. Needs `UPHEAP_RUN`.
pub async fn run(args: ReachedArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let run = current_run(app, "reached")?.to_owned();
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let body = ReachedRequestDto {
        result: args.result,
        links: args.links,
    };
    let reached = client
        .post::<RunDto>(&["work", "runs", &run, "reached"], &body, &token)
        .await
        .map_err(|err| missing_agents(err, Some(&run)))?;
    match out.mode() {
        Mode::Json => out.json(&reached.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let item = &reached.value.item;
            // The server's rule for opening the next turn (`Turn::follow_on` in the domain crate), which the
            // response doesn't carry: change the two together.
            let ending = if item.reach == ReachDto::AllTheWay && item.goal != GoalDto::Done {
                "goes on, all the way"
            } else {
                "is yours again"
            };
            out.line(&format!("Reached: {}. {} {ending}.", goal_phrase(item.goal), item.key))?;
        }
    }
    Ok(())
}
