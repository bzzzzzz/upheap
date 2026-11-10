//! `upheap step tick | drop <step>`: move a step to ticked or dropped.
//!
//! The server owns the rules. The CLI only notices a step that is already where it's asked to go, which it has in
//! hand from the read that gives it the step's version, and then makes no write.

use api_types::common::StepStateDto;
use api_types::steps::StepPatchDto;
use clap::Args;

use super::{find_step, step_name};
use crate::commands::heap_api::HeapApi;
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::StepName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct StepArgs {
    /// The step: its item's key and its number, such as k3x9a.3. The item may be team/heap/key or a link
    #[arg(value_name = "STEP")]
    pub step: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// The state a command moves a step to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepVerb {
    Tick,
    Drop,
}

impl StepVerb {
    const fn target(self) -> StepStateDto {
        match self {
            Self::Tick => StepStateDto::Ticked,
            Self::Drop => StepStateDto::Dropped,
        }
    }

    const fn past(self) -> &'static str {
        match self {
            Self::Tick => "ticked",
            Self::Drop => "dropped",
        }
    }
}

/// Moves the step to the verb's state, or says it's there already.
pub async fn run_verb(verb: StepVerb, args: StepArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = StepName::parse(&args.step)?;
    let at = locate_item(&name.item_typed, name.item.clone(), args.heap.name.as_deref(), app)?;
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
    if step.state == verb.target() {
        match out.mode() {
            Mode::Json => out.json(&item.body)?,
            Mode::Quiet => {}
            Mode::Human => out.line(&format!("{} is already {}.", step_name(&item.value, step), verb.past()))?,
        }
        return Ok(());
    }
    let patch = StepPatchDto {
        state: Some(verb.target()),
        ..StepPatchDto::default()
    };
    let moved = api.patch_step(&at.key, step.number, step.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&moved.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let (key, title) = moved
                .value
                .steps
                .iter()
                .flatten()
                .find(|moved_step| moved_step.number == name.number)
                .map_or((step_name(&item.value, step), step.title.clone()), |moved_step| {
                    (step_name(&moved.value, moved_step), moved_step.title.clone())
                });
            let verb_text = match verb {
                StepVerb::Tick => "Ticked",
                StepVerb::Drop => "Dropped",
            };
            out.line(&format!("{verb_text} {key}: {title}."))?;
        }
    }
    Ok(())
}
