//! `upheap item send | take-back <item>`: hand an item to an agent, and take it back.
//!
//! The server owns the rules: what may be sent, where the turn goes, and that taking back an item no agent has
//! changes nothing. Each command makes one request and says what the server answered.

use api_types::turns::{ReachDto, SendToAgentDto};
use clap::Args;

use super::common::locate_item;
use super::show::turn_text;
use super::state::StateArgs;
use crate::commands::heap_api::{HeapApi, read_text_flag};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct SendArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// Go on through the stable states until the work is done, not just to the next one
    #[arg(long)]
    pub all_the_way: bool,
    /// Something to tell the agent, or - to read it from stdin. At most 2,000 characters
    #[arg(long, value_name = "TEXT")]
    pub line: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Sends the item to an agent, to the item's next stable state.
pub async fn run_send(args: SendArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let line = match &args.line {
        Some(raw) => Some(read_text_flag(raw, "--line", app).await?),
        None => None,
    };
    let body = SendToAgentDto {
        reach: if args.all_the_way {
            ReachDto::AllTheWay
        } else {
            ReachDto::One
        },
        goal: None,
        line,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let sent = api.send_item(&args.item, &at.key, &body).await?;
    match out.mode() {
        Mode::Json => out.json(&sent.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let turn = sent.value.turn.as_ref().map(turn_text).unwrap_or_default();
            out.line(format!("Sent {} to an agent. {turn}", sent.value.key).trim_end())?;
        }
    }
    Ok(())
}

/// Takes the item back from the agent that has it. An item no agent has is answered unchanged, so this succeeds too.
pub async fn run_take_back(args: StateArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.take_back_item(&args.item, &at.key).await?;
    match out.mode() {
        Mode::Json => out.json(&item.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&format!("{} is back with you: {}.", item.value.key, item.value.title))?,
    }
    Ok(())
}
