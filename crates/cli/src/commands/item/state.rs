//! `upheap item start | block | done | wontdo | reopen <item>`: move an item to another state.
//!
//! The server owns every transition rule. The CLI only notices an item that is already where it's asked to go, which
//! it has in hand from the read that gives it the version, and then makes no write.

use api_types::common::{ItemDto, StateDto};
use api_types::items::ItemPatchDto;
use clap::Args;

use super::common::{ItemLocation, locate_item, state_text};
use crate::commands::heap_api::HeapApi;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct StateArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

#[derive(Args, Debug)]
pub struct BlockArgs {
    #[command(flatten)]
    pub state: StateArgs,
    /// The item it waits for, named like ITEM. It must be in the same heap and not closed
    #[arg(long, value_name = "ITEM")]
    pub by: Option<String>,
}

/// The state a command moves an item to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateVerb {
    Start,
    Block,
    Done,
    WontDo,
    Reopen,
}

impl StateVerb {
    fn target(self) -> StateDto {
        match self {
            Self::Start => StateDto::InProgress,
            Self::Block => StateDto::Blocked,
            Self::Done => StateDto::Done,
            Self::WontDo => StateDto::WontDo,
            Self::Reopen => StateDto::Open,
        }
    }

    /// What the command says it did to `item`, as it is after the change.
    fn done_line(self, item: &ItemDto) -> String {
        let (key, title) = (&item.key, &item.title);
        match self {
            Self::Start => format!("Started {key}: {title}."),
            Self::Block => match &item.blocked_by {
                Some(blocker) => format!("Blocked {key}: {title}. Waiting for {}.", blocker.key),
                None => format!("Blocked {key}: {title}."),
            },
            Self::Done => format!("Done {key}: {title}."),
            Self::WontDo => format!("Won't do {key}: {title}."),
            Self::Reopen => format!("Reopened {key}: {title}."),
        }
    }
}

/// Moves the item to the verb's state, or says it's there already.
pub async fn run_verb(verb: StateVerb, args: StateArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    change_state(verb, &args, None, app, out).await
}

/// Blocks the item, on `--by` when given.
pub async fn run_block(args: BlockArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    change_state(StateVerb::Block, &args.state, args.by.as_deref(), app, out).await
}

/// Where the item a block waits for is.
enum BlockerSpot<'a> {
    /// A bare key: in the heap of the item being blocked.
    SameHeap(String),
    /// A path or link, which names its heap.
    Located(ItemLocation<'a>),
}

async fn change_state(
    verb: StateVerb,
    args: &StateArgs,
    by: Option<&str>,
    app: &App,
    out: &mut Output,
) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    // A bare key is in the item's own heap, however that was found; a path or link names its own.
    let blocker_at = match by {
        Some(typed) => Some((
            typed,
            match ItemName::parse(typed)? {
                ItemName::Key(key) => BlockerSpot::SameHeap(key),
                name => BlockerSpot::Located(locate_item(typed, name, args.heap.name.as_deref(), app)?),
            },
        )),
        None => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let item = api.get_item(&args.item, &at.key).await?;
    // The blocker may be in another heap, which the server then refuses; it needs the blocker's ID to say so.
    let blocker = match blocker_at {
        Some((typed, BlockerSpot::SameHeap(key))) => Some(api.get_item(typed, &key).await?.value),
        Some((typed, BlockerSpot::Located(at))) => {
            let blocker_heap = at.heap.resolve(&client, &token).await?;
            let blocker_api = HeapApi {
                client: &client,
                token: &token,
                heap: &blocker_heap,
            };
            Some(blocker_api.get_item(typed, &at.key).await?.value)
        }
        None => None,
    };
    let target = verb.target();
    let blocker_id = blocker.as_ref().map(|blocker| blocker.id);
    let is_already = item.value.state == target
        && blocker_id.is_none_or(|id| item.value.blocked_by.as_ref().is_some_and(|current| current.id == id));
    if is_already {
        match out.mode() {
            Mode::Json => out.json(&item.body)?,
            Mode::Quiet => {}
            Mode::Human => out.line(&format!("{} is already {}.", item.value.key, state_text(target)))?,
        }
        return Ok(());
    }
    let patch = ItemPatchDto {
        state: Some(target),
        blocked_by_item_id: blocker_id.map(Some),
        ..ItemPatchDto::default()
    };
    let moved = api.patch_item(&at.key, item.value.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&moved.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&verb.done_line(&moved.value))?,
    }
    Ok(())
}
