//! `upheap auth logout`: ends the sign-in on the server and forgets it.

use clap::Args;

use crate::auth;
use crate::commands::SharedArgs;
use crate::config::server_key;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct LogoutArgs {
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Revokes the stored refresh token, which ends every token of the sign-in, then removes it from the keychain. Not
/// being signed in is nothing to do. When the server can't be reached the token is kept, so trying again later
/// still revokes it.
pub async fn run(args: LogoutArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let server = app.server(&args.shared)?;
    let key = server_key(&server);
    let client = app.client(server)?;
    // A refresh in another `upheap` would replace the token being revoked.
    let _lock = auth::lock_tokens(app.config_dir()?).await?;
    let Some(tokens) = auth::load_tokens(&app.tokens, &key).await? else {
        if out.mode() == Mode::Human {
            out.line(&format!("Not signed in to {key}; nothing to do."))?;
        }
        return Ok(());
    };
    auth::revoke(&client, &tokens.refresh_token).await?;
    auth::delete_tokens(&app.tokens, &key).await?;
    if out.mode() == Mode::Human {
        out.line(&format!("Signed out of {key}."))?;
    }
    Ok(())
}
