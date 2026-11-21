//! `upheap parameter stop-using <parameter>`: make the heap stop using a team parameter.

use clap::Args;

use super::change_use;
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct StopUsingArgs {
    /// The team parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Makes the heap stop using the parameter, which the server refuses while an open, in-progress or blocked item has
/// a value for it. One the heap doesn't use is left as it is. Human output says what was done; `--quiet` prints
/// nothing and `--json` the heap's parameters.
pub async fn run(args: StopUsingArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let stopped = change_use("stop-using", &args.parameter, &args.heap, &args.shared, app).await?;
    match out.mode() {
        Mode::Json => out.json(&stopped.list.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!(
            "{} no longer uses team parameter \"{}\".",
            stopped.heap, stopped.name
        ))?,
    }
    Ok(())
}
