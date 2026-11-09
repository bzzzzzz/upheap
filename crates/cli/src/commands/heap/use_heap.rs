//! `upheap heap use <heap>`: makes a heap the default for commands that work in one.

use api_types::me::MeDto;
use clap::Args;

use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::config::{self, ConfigFile, server_key};
use crate::lookup::{HeapName, resolve_heap};
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct UseArgs {
    /// team/heap, or a heap slug only one of your teams has
    #[arg(value_name = "HEAP")]
    pub name: String,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Checks the heap against the caller's heaps (`GET /me`), so a typo fails here rather than in the next command,
/// and writes its `team/heap` to the config file. A heap is one server's, so `--server` naming another server than
/// the config file's writes that server too, and the two stay a pair.
pub async fn run(args: UseArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let name = HeapName::parse(&args.name)?;
    let server = app.server(&args.shared)?;
    let key = server_key(&server);
    let client = app.client(server)?;
    let token = app.access_token(&client).await?;
    let me = client.get::<MeDto>(&["me"], NO_QUERY, &token).await?;
    let handle = resolve_heap(name, &me.value)?;
    let new_server = args.shared.server.is_some() && app.config.server.as_deref() != Some(key.as_str());
    let config = ConfigFile {
        heap: Some(handle.to_string()),
        server: if new_server {
            Some(key.clone())
        } else {
            app.config.server.clone()
        },
    };
    config::save_config(app.config_dir()?, &config)?;
    if out.mode() == Mode::Human {
        out.line(&format!("Commands now work in {handle} unless told otherwise."))?;
        if new_server {
            out.line(&format!(
                "The default server is now {key} too, the server {handle} is on."
            ))?;
        }
    }
    Ok(())
}
