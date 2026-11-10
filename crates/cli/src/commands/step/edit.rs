//! `upheap step edit <step>`: change a step's title or context.

use api_types::steps::StepPatchDto;
use clap::Args;

use super::{find_step, step_name};
use crate::commands::heap_api::{HeapApi, read_context};
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::StepName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The step: its item's key and its number, such as k3x9a.3. The item may be team/heap/key or a link
    #[arg(value_name = "STEP")]
    pub step: String,
    /// A new title
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,
    /// New background for the step, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE", conflicts_with = "no_context")]
    pub context_file: Option<String>,
    /// Clear the context
    #[arg(long)]
    pub no_context: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Changes the fields given and leaves the rest. `--quiet` prints nothing and `--json` the item.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = StepName::parse(&args.step)?;
    if args.title.is_none() && args.context_file.is_none() && !args.no_context {
        return Err(UsageError::NothingToChangeInStep.into());
    }
    let at = locate_item(&name.item_typed, name.item.clone(), args.heap.name.as_deref(), app)?;
    let context = match (&args.context_file, args.no_context) {
        (Some(path), _) => Some(Some(read_context(path, app).await?)),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&name.item_typed, &at.key).await?;
    let step = find_step(&item.value, name.number).ok_or_else(|| api.no_such_step(&at.key, name.number))?;
    let patch = StepPatchDto {
        title: args.title,
        context,
        ..StepPatchDto::default()
    };
    let edited = api.patch_step(&at.key, step.number, step.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let now = find_step(&edited.value, name.number).unwrap_or(step);
            let label = step_name(&edited.value, now);
            if now.version == step.version {
                out.line(&format!("No change to {label}."))?;
            } else {
                out.line(&format!("Edited {label}: {}.", now.title))?;
            }
        }
    }
    Ok(())
}
