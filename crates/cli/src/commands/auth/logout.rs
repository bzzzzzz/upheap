//! `upheap auth logout [--local]`: ends the sign-in on the server and forgets it.

use clap::Args;

use crate::auth::{self, AuthError, StoredTokens, TokenStoreError};
use crate::commands::SharedArgs;
use crate::config::server_key;
use crate::output::{Mode, Output};
use crate::{App, CliError};

#[derive(Args, Debug)]
pub struct LogoutArgs {
    /// Only forget the sign-in on this machine, without asking the server to end it: for a server that's gone. The
    /// sign-in stays usable, by whoever holds a copy of its tokens, until they expire
    #[arg(long)]
    pub local: bool,
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Revokes the stored refresh token, which ends every token of the sign-in, then removes it from the keychain. Not
/// being signed in is nothing to do. When the server can't revoke it the token is kept, so trying again later
/// still revokes it; `--local` forgets it without asking.
pub async fn run(args: LogoutArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let server = app.server(&args.shared)?;
    let key = server_key(&server);
    let client = app.client(server)?;
    // A refresh in another `upheap` would replace the token being revoked.
    let _lock = auth::lock_tokens(app.config_dir()?).await?;
    let stored = match auth::load_tokens(&app.tokens, &key).await {
        // An entry the CLI can't read has no token to revoke, but `--local` can still forget it.
        Err(TokenStoreError::Corrupt { .. }) if args.local => Stored::Unreadable,
        Err(err) => return Err(err.into()),
        Ok(Some(tokens)) => Stored::Tokens(tokens),
        Ok(None) => {
            if out.mode() == Mode::Human {
                out.line(&format!("Not signed in to {key}; nothing to do."))?;
            }
            return Ok(());
        }
    };
    if let (Stored::Tokens(tokens), false) = (&stored, args.local) {
        auth::revoke(&client, &tokens.refresh_token)
            .await
            .map_err(AuthError::Revoke)?;
    }
    auth::delete_tokens(&app.tokens, &key).await?;
    if out.mode() == Mode::Human {
        out.line(&format!("Signed out of {key}."))?;
    }
    if args.local {
        out.note(
            "The sign-in was forgotten here only: it may stay open on the server until its tokens expire, up to 30 \
             days from their last use.",
        )?;
    }
    Ok(())
}

/// What the keychain holds for the server.
enum Stored {
    Tokens(StoredTokens),
    /// An entry that isn't one the CLI wrote, or no longer reads.
    Unreadable,
}
