//! `upheap heap add <name>`: add a heap to a team.

use api_types::heaps::HeapDto;
use api_types::me::MeDto;
use api_types::teams::CreateNamedDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::lookup::{HeapHandle, find_personal_team};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct AddArgs {
    /// The heap's name, 1 to 100 characters
    #[arg(id = "heap_name", value_name = "NAME")]
    pub name: String,
    /// The team it goes to, by slug. Left out, your personal team
    #[arg(long, value_name = "TEAM")]
    pub team: Option<String>,
    /// The name in the heap's links and in team/heap: lowercase letters, digits and single dashes, starting with a
    /// letter, 2 to 39 characters. Left out, the server makes one from the name
    #[arg(long, value_name = "SLUG")]
    pub slug: Option<String>,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Adds the heap with the name and slug as typed: the server checks them, and makes the slug when none is given,
/// so the output names the heap by the `team/heap` the server kept. `--quiet` prints that alone and `--json` the
/// created heap. The default heap stays what it was.
pub async fn run(args: AddArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let client = app.client(app.server(&args.shared)?)?;
    let token = app.access_token(&client).await?;
    let team = match args.team {
        Some(team) => team,
        None => {
            let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
            find_personal_team(&me.value)?.to_owned()
        }
    };
    let body = CreateNamedDto {
        name: args.name,
        slug: args.slug,
    };
    let added = client
        .post::<HeapDto>(&["teams", &team, "heaps"], &body, &token)
        .await?;
    let handle = HeapHandle {
        team,
        heap: added.value.slug,
    };
    match out.mode() {
        Mode::Json => out.json(&added.body)?,
        Mode::Quiet => out.line(&handle.to_string())?,
        Mode::Human => out.line(&format!("Added heap {handle}."))?,
    }
    Ok(())
}
