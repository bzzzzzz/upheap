//! `upheap parameter use <parameter>`: make the heap use one of its team's parameters.

use clap::Args;

use super::change_use;
use crate::commands::{HeapArgs, SharedArgs};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct UseArgs {
    /// The team parameter: its name, ignoring case, or its ID
    #[arg(value_name = "PARAM")]
    pub parameter: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Makes the heap use the parameter, which the server does with the weight it had, or 10 the first time. One the
/// heap uses already is left as it is. Human output says what the weight is and how to change it; `--quiet` prints
/// nothing and `--json` the heap's parameters.
pub async fn run(args: UseArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let used = change_use("use", &args.parameter, &args.heap, &args.shared, app).await?;
    match out.mode() {
        Mode::Json => out.json(&used.list.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let weight = used
                .list
                .value
                .parameters
                .iter()
                .find(|parameter| parameter.name == used.name)
                .map_or(String::new(), |parameter| format!(" with weight {}", parameter.weight));
            out.line(&format!(
                "{} uses team parameter \"{}\"{weight}; change the weight with `upheap heap weights --parameter \"{}=N\"`.",
                used.heap, used.name, used.name
            ))?;
        }
    }
    Ok(())
}
