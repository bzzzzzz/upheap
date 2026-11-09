//! `upheap tag edit <tag>`: rename a tag or change its description.

use api_types::tags::{TagDto, TagListDto, TagPatchDto};
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::heap_api::{HeapApi, named_heap, read_description, resolve_heap_handle};
use crate::commands::{HeapArgs, SharedArgs};
use crate::lookup::find_tag;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The tag: its name, ignoring case, or its ID
    #[arg(value_name = "TAG")]
    pub tag: String,
    /// A new name, 1 to 50 characters, not used by another tag of the heap ignoring case
    #[arg(long, id = "new_name", value_name = "NAME")]
    pub name: Option<String>,
    /// A new description. `-` reads all of stdin
    #[arg(long, value_name = "TEXT", conflicts_with = "no_description")]
    pub description: Option<String>,
    /// Clear the description
    #[arg(long)]
    pub no_description: bool,
    #[command(flatten)]
    pub heap: HeapArgs,
    #[command(flatten)]
    pub shared: SharedArgs,
}

impl EditArgs {
    /// Whether any field flag is given.
    fn has_changes(&self) -> bool {
        self.name.is_some() || self.description.is_some() || self.no_description
    }
}

/// Changes the fields given and leaves the rest. Human output says whether the tag was renamed, updated or left
/// as it was; `--quiet` prints nothing and `--json` the edited tag.
pub async fn run(args: EditArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    if !args.has_changes() {
        if out.mode() == Mode::Human {
            out.line("Nothing to change: give --name, --description or --no-description.")?;
        }
        return Ok(());
    }
    let name = named_heap(args.heap.name.as_deref(), app)?;
    let description = match (&args.description, args.no_description) {
        (Some(raw), _) => Some(Some(read_description(raw, app).await?)),
        (None, true) => Some(None),
        (None, false) => None,
    };
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let heap = resolve_heap_handle(name, &client, &token).await?;
    let api = HeapApi {
        client: &client,
        token: &token,
        heap: &heap,
    };
    let tags = api.get::<TagListDto>(&["tags"], NO_QUERY).await?;
    let tag = find_tag(&args.tag, &tags.value, &heap)?;
    let patch = TagPatchDto {
        name: args.name,
        description,
    };
    let id = tag.id.to_string();
    let edited = api.patch::<TagDto>(&["tags", &id], tag.version, &patch).await?;
    match out.mode() {
        Mode::Json => out.json(&edited.body)?,
        Mode::Quiet => {}
        Mode::Human => out.line(&edit_line(tag, &edited.value))?,
    }
    Ok(())
}

/// What the edit did, from the tag as read and as the server returned it.
fn edit_line(before: &TagDto, after: &TagDto) -> String {
    if after.version == before.version {
        format!("No change to tag \"{}\".", after.name)
    } else if after.name != before.name {
        format!("Renamed tag \"{}\" to \"{}\".", before.name, after.name)
    } else {
        format!("Updated tag \"{}\".", after.name)
    }
}
