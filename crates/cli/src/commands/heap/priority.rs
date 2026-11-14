//! `upheap heap priority <N> [<heap>]`: set your own priority for a heap.

use api_types::heaps::{HeapDto, HeapPriorityRequestDto};
use clap::Args;
use domain::heap::HeapPriority;

use crate::commands::SharedArgs;
use crate::commands::heap_api::{HeapApi, named_heap, resolve_heap_handle};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
#[command(
    about = "Set your priority for a heap, 1 to 10 (1 by default)",
    long_about = "Set your priority for a heap, 1 to 10 (1 by default): how its items compare with your other heaps' in \
                  item list --all-heaps, what waits for you and work run. Only you see it."
)]
pub struct PriorityArgs {
    /// Your priority for the heap, from 1 to 10
    #[arg(value_name = "N", value_parser = clap::value_parser!(u8).range(
        i64::from(HeapPriority::MIN)..=i64::from(HeapPriority::MAX)
    ))]
    pub priority: u8,
    /// team/heap, or a heap slug only one of your teams has. Without it, the default heap
    #[arg(value_name = "HEAP")]
    pub heap: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Sets the priority and says what it is now. `--json` prints the heap as the server returns it, and `--quiet`
/// nothing. Setting the value already stored succeeds.
pub async fn run(args: PriorityArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = named_heap(args.heap.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let handle = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &handle,
    };
    let body = HeapPriorityRequestDto {
        priority: i64::from(args.priority),
    };
    let saved = api.put_declared::<HeapDto>(&["priority"], &body).await?;
    match out.mode() {
        Mode::Json => out.json(&saved.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!(
            "Your priority for {} is {}.",
            saved.value.name, saved.value.priority
        ))?,
    }
    Ok(())
}
