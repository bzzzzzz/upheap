//! `upheap item place <item> --above|--below <item>`, `upheap item place <item> --reset`: put an item right above or
//! below another in its heap's list, or put it back where its fields put it.
//!
//! The server owns every rule: which items can be an anchor and where the item lands. The CLI reads the item for its
//! version and the anchor for its ID, and ranks at the current time.

use api_types::common::PlacementDto;
use api_types::items::{ItemPlaceDto, PlaceSideDto};
use clap::{ArgGroup, Args};

use super::common::locate_item;
use crate::commands::heap_api::HeapApi;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
#[command(group(ArgGroup::new("place").required(true).args(["above", "below", "reset"])))]
pub struct PlaceArgs {
    /// The item to place: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// Place it right above this item, named like ITEM. It must be in the same heap and rank (open, in progress or
    /// blocked, and not a reminder to come)
    #[arg(long, value_name = "ITEM")]
    pub above: Option<String>,
    /// Place it right below this item, named like ITEM
    #[arg(long, value_name = "ITEM")]
    pub below: Option<String>,
    /// Take its place by hand away, so it ranks by its fields again
    #[arg(long)]
    pub reset: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Places the item, or resets its place.
pub async fn run(args: PlaceArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
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
    let item = api.get_item(&args.item, &at.key).await?;

    let anchor = match (&args.above, &args.below) {
        (Some(typed), None) => Some((typed, PlaceSideDto::Above)),
        (None, Some(typed)) => Some((typed, PlaceSideDto::Below)),
        _ => None,
    };
    let Some((typed, side)) = anchor else {
        let reset = api.reset_item_placement(&at.key, item.value.version).await?;
        match out.mode() {
            Mode::Json => out.json(&reset.body)?,
            Mode::Quiet => out.line(&reset.value.key)?,
            Mode::Human => out.line(&format!("{}'s place is reset.", reset.value.key))?,
        }
        return Ok(());
    };

    let anchor_at = locate_item(typed, ItemName::parse(typed)?, args.heap.name.as_deref(), app)?;
    let anchor_heap = anchor_at.heap.resolve(&client, &token).await?;
    let anchor_api = HeapApi {
        client: &client,
        token: &token,
        heap: &anchor_heap,
    };
    let anchor = anchor_api.get_item(typed, &anchor_at.key).await?;
    let body = ItemPlaceDto {
        anchor: anchor.value.id,
        side,
        as_of: None,
    };
    let placed = api
        .post_item_action(&at.key, "place", item.value.version, &body)
        .await?;
    match out.mode() {
        Mode::Json => out.json(&placed.body)?,
        Mode::Quiet => out.line(&placed.value.key)?,
        Mode::Human => {
            let way = match placed.value.placement {
                Some(PlacementDto::Down) => "down",
                _ => "up",
            };
            let next_to = match side {
                PlaceSideDto::Above => "above",
                PlaceSideDto::Below => "below",
            };
            out.line(&format!(
                "{} moved {way}, just {next_to} {}.",
                placed.value.key, anchor.value.key
            ))?;
        }
    }
    Ok(())
}
