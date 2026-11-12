//! `upheap comment add <item> <text>`: add a comment to an item.
//!
//! One `POST`, with no read before it: the server owns every rule (the text, at most 200 comments, an unarchived
//! heap), and it answers with the whole item, whose newest comment, the one with the highest number, is the one
//! added. The text is sent as given; the server trims it and refuses a blank one.

use api_types::comments::AddCommentDto;
use api_types::common::ItemDto;
use clap::Args;

use crate::commands::heap_api::{HeapApi, read_text_file};
use crate::commands::item::common::locate_item;
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::ItemName;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The item: its key, team/heap/key, or a link to it
    #[arg(value_name = "ITEM")]
    pub item: String,
    /// The comment: plain text, up to 20,000 characters. Or use --file
    #[arg(value_name = "TEXT", conflicts_with = "file", required_unless_present = "file")]
    pub text: Option<String>,
    /// Read the comment from this file, or from stdin for `-`
    #[arg(long, value_name = "FILE")]
    pub file: Option<String>,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the comment. Human output names it; `--quiet` prints its name and `--json` the item.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = ItemName::parse(&args.item)?;
    let at = locate_item(&args.item, name, args.heap.name.as_deref(), app)?;
    let body = match (args.text, &args.file) {
        (Some(text), _) => text,
        (None, Some(path)) => read_text_file(path, "--file", app).await?,
        // clap requires one of the two.
        (None, None) => String::new(),
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = at.heap.resolve(&client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let added = api.post_comment(&args.item, &at.key, &AddCommentDto { body }).await?;
    report(&added.value, &added.body, out)
}

/// Says what was added: human output names the comment, `--quiet` prints its name and `--json` the item.
fn report(item: &ItemDto, body: &str, out: &mut Output) -> Result<(), CliError> {
    // Numbers only grow, so the comment just added has the highest.
    let comment = item.comments.iter().flatten().max_by_key(|comment| comment.number);
    match (out.mode(), comment) {
        (Mode::Json, _) => out.json(body)?,
        (Mode::Quiet, Some(comment)) => out.line(&format!("{}.c{}", item.key, comment.number))?,
        (Mode::Human, Some(comment)) => {
            out.line(&format!(
                "Commented on {} ({}.c{}).",
                item.key, item.key, comment.number
            ))?;
        }
        (Mode::Quiet | Mode::Human, None) => {}
    }
    Ok(())
}
