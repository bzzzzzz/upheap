//! `upheap step add <item> <title>`: add a step to a task.
//!
//! One `POST`, with no read before it: the server owns every rule (a task, an open item, at most 100 steps), and it
//! answers with the whole item, whose newest step, the one with the highest number, is the one added.

use api_types::steps::AddStepDto;
use clap::Args;
use domain::step::StepNumber;

use super::read_context;
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs, UsageError};
use crate::lookup::{HeapHandle, ItemName, LookupError, StepName};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The task: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// What the step is, 1 to 300 characters
    #[arg(value_name = "TITLE")]
    pub title: String,
    /// Background for the step, read from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub context_file: Option<String>,
    /// Put it right after this step, named like k3x9a.2 or by its number, 2. Left out, it goes at the end
    #[arg(long, value_name = "STEP")]
    pub after: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// The step `--after` names.
enum After {
    Number(u32),
    Step { typed: String, name: StepName },
}

impl After {
    /// A bare number, or a step name.
    fn parse(raw: &str) -> Result<Self, LookupError> {
        if raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return StepNumber::parse(raw)
                .map(|number| Self::Number(number.get()))
                .ok_or_else(|| LookupError::MalformedStep { name: raw.to_owned() });
        }
        Ok(Self::Step {
            typed: raw.to_owned(),
            name: StepName::parse(raw)?,
        })
    }

    /// The step's number, once it's known to be a step of the item `item` (as typed) with `key` in `heap`.
    fn number_in(&self, item: &str, key: &str, heap: &HeapHandle) -> Result<u32, UsageError> {
        match self {
            Self::Number(number) => Ok(*number),
            Self::Step { typed, name } => {
                let same_item = match &name.item {
                    ItemName::Key(other) => other == key,
                    ItemName::Full {
                        heap: other_heap,
                        key: other,
                    } => other == key && other_heap == heap,
                };
                if same_item {
                    Ok(name.number)
                } else {
                    Err(UsageError::StepInOtherItem {
                        step: typed.clone(),
                        item: item.to_owned(),
                    })
                }
            }
        }
    }
}

/// Adds the step. Human output names it; `--quiet` prints its name and `--json` the item.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let after = args.after.as_deref().map(After::parse).transpose()?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let context = match &args.context_file {
        Some(path) => Some(read_context(path, app).await?),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let after = after
        .map(|after| after.number_in(&args.item, &at.key, &heap))
        .transpose()?;
    let api = crate::commands::heap_api::HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let body = AddStepDto {
        title: args.title,
        context,
        after,
    };
    let added = api.post_step(&args.item, &at.key, &body).await?;
    // Numbers only grow, so the step just added has the highest.
    let step = added.value.steps.iter().flatten().max_by_key(|step| step.number);
    match (out.mode(), step) {
        (Mode::Json, _) => out.json(&added.body)?,
        (Mode::Quiet, Some(step)) => out.line(&format!("{}.{}", added.value.key, step.number))?,
        (Mode::Human, Some(step)) => {
            out.line(&format!("Added {}.{}: {}.", added.value.key, step.number, step.title))?
        }
        (Mode::Quiet | Mode::Human, None) => {}
    }
    Ok(())
}
