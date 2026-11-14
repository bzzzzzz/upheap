//! `upheap item to-step <note> --task <task>`: make a note a step of an existing task in the same heap.
//!
//! The server owns the rules: that the item is a note and open, that the task is an open task of the note's heap,
//! and, in a run, that the note is the run's item, which also reaches the run's turn. The CLI resolves the task to
//! its ID and says what happened.

use api_types::common::{ItemDto, MadeStepDto, StateDto};
use api_types::items::ItemToStepDto;
use clap::Args;

use super::add::item_id_of;
use super::common::locate_item;
use super::state::StateArgs;
use crate::commands::heap_api::HeapApi;
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct ToStepArgs {
    #[command(flatten)]
    pub state: StateArgs,
    /// The task that gets the step, named like ITEM. It must be an open task in the same heap
    #[arg(long, value_name = "TASK")]
    pub task: String,
}

/// Makes the note a step at the end of the task's steps, and closes the note as became. In a run, the turn is
/// reached with it.
pub async fn run(args: ToStepArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let args_state = args.state;
    let name = ItemName::parse(&args_state.item)?;
    let task_name = ItemName::parse(&args.task)?;
    let at = locate_item(&args_state.item, name, args_state.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args_state.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let task_id = item_id_of(task_name, &args.task, &heap, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let note = api
        .to_step_item(&args_state.item, &at.key, &ItemToStepDto { task_id })
        .await?;
    match out.mode() {
        Mode::Json => out.json(&note.body)?,
        Mode::Quiet => {
            if let Some(step) = made_step(&note.value, task_id) {
                out.line(&format!("{}.{}", step.task.key, step.number))?;
            }
        }
        Mode::Human => {
            if let Some(step) = made_step(&note.value, task_id) {
                out.line(&format!(
                    "{} \"{}\" became step {}.{} of {} \"{}\".",
                    note.value.key, note.value.title, step.task.key, step.number, step.task.key, step.task.title
                ))?;
            }
            if app.env.run.is_some() && note.value.state == StateDto::Became {
                out.line("The turn is reached.")?;
            }
        }
    }
    Ok(())
}

/// The step the note became on the task `task_id`: its newest one there.
fn made_step(note: &ItemDto, task_id: uuid::Uuid) -> Option<&MadeStepDto> {
    note.became_steps
        .as_deref()?
        .iter()
        .rev()
        .find(|step| step.task.id == task_id)
}
