//! `upheap step move <step> (--after <step> | --first)`: move a step to another place in its task.
//!
//! The server owns the rules, among them that the step to go after is one of the item's. The CLI reads the step
//! for its version, as the other step commands do, and sends the place in a patch.

use api_types::steps::StepPatchDto;
use clap::{ArgGroup, Args};

use super::add::StepRef;
use super::{find_step, step_name};
use crate::commands::heap_api::HeapApi;
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::StepName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
#[command(group = ArgGroup::new("place").required(true).args(["after", "first"]))]
pub struct MoveArgs {
    /// The step: its item's key and its number, such as k3x9a.3. The item may be team/heap/key or a link
    #[arg(value_name = "STEP")]
    pub step: String,
    /// Put it right after this step, named like k3x9a.2 or by its number, 2
    #[arg(long, value_name = "STEP")]
    pub after: Option<String>,
    /// Put it before every other step
    #[arg(long)]
    pub first: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Moves the step. Human output says where it went; `--quiet` prints its name and `--json` the item.
pub async fn run(args: MoveArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = StepName::parse(&args.step)?;
    let after = args.after.as_deref().map(StepRef::parse).transpose()?;
    let at = locate_item(&name.item_typed, name.item.clone(), args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let after = after
        .map(|after| after.number_in(&name.item_typed, &at.key, &heap))
        .transpose()?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&name.item_typed, &at.key).await?;
    let step = find_step(&item.value, name.number).ok_or_else(|| api.no_such_step(&at.key, name.number))?;
    let patch = StepPatchDto {
        after: Some(after),
        ..StepPatchDto::default()
    };
    let moved = api.patch_step(&at.key, step.number, step.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&moved.body)?,
        Mode::Quiet => out.line(&step_name(&moved.value, step))?,
        Mode::Human => {
            let now = find_step(&moved.value, name.number).unwrap_or(step);
            let label = step_name(&moved.value, now);
            match after {
                _ if now.version == step.version => out.line(&format!("No change to {label}: it is there already."))?,
                Some(after) => out.line(&format!("Moved {label} after {}.{after}.", moved.value.key))?,
                None => out.line(&format!("Moved {label} to the top."))?,
            }
        }
    }
    Ok(())
}
