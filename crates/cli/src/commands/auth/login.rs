//! `upheap auth login [--server <url>]`: signs in through the browser and keeps the sign-in in the keychain.

use std::time::Duration;

use api_types::me::MeDto;
use clap::Args;

use crate::auth::login::{LoginError, authorization_request, read_callback};
use crate::auth::loopback::Loopback;
use crate::auth::pkce::{Pkce, generate_state};
use crate::auth::{self, StoredTokens, TokenStoreError};
use crate::client::NO_QUERY;
use crate::commands::SharedArgs;
use crate::config::{self, server_key};
use crate::output::{Mode, Output};
use crate::{App, CliError};

/// How long the CLI waits for the browser to come back from the consent page.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Args, Debug)]
pub struct LoginArgs {
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Opens the consent page, waits on a loopback port for the code, and exchanges it for tokens, which go into the
/// keychain in place of any earlier sign-in to the server. `--server` is written to the config file once signed in.
pub async fn run(args: LoginArgs, app: &App, out: &mut Output) -> Result<(), CliError> {
    let server = app.server(&args.shared)?;
    let key = server_key(&server);
    let client = app.client(server)?;
    // Reading the keychain first proves it works before the user goes to the browser. An entry it can't read is
    // replaced below.
    match auth::load_tokens(&app.tokens, &key).await {
        Ok(_) | Err(TokenStoreError::Corrupt { .. }) => {}
        Err(err) => return Err(err.into()),
    }
    let lock_dir = app.config_dir()?;

    let loopback = Loopback::bind().await.map_err(LoginError::from)?;
    let pkce = Pkce::generate().map_err(LoginError::Random)?;
    let state = generate_state().map_err(LoginError::Random)?;
    let request = authorization_request(loopback.redirect_uri(), &pkce, &state);
    let url = client.web_url(&["oauth", "authorize"], &request);
    out.note(&format!(
        "Opening the browser to sign in to {key}. If it doesn't open, go to:"
    ))?;
    out.note("")?;
    out.note(&format!("  {url}"))?;
    out.note("")?;
    if let Err(err) = app.browser.open(url.as_str()) {
        tracing::warn!(error = %err, "can't open the browser");
    }
    let redirect_uri = loopback.redirect_uri().to_owned();
    let callback = loopback.wait(&state, LOGIN_TIMEOUT).await.map_err(LoginError::from)?;
    let code = read_callback(callback)?;

    let issued = auth::exchange_code(&client, code, &redirect_uri, pkce.verifier).await?;
    let tokens = StoredTokens::issued(issued, app.env.now.as_second());
    // Another `upheap` may be refreshing the earlier sign-in; replace and revoke it only once that's done.
    let _lock = auth::lock_tokens(lock_dir).await?;
    let previous = match auth::load_tokens(&app.tokens, &key).await {
        Ok(previous) => previous,
        // Nothing the CLI can revoke: overwriting it is all that's left to do.
        Err(TokenStoreError::Corrupt { .. }) => None,
        Err(err) => return Err(err.into()),
    };
    if let Err(err) = auth::save_tokens(&app.tokens, &key, &tokens).await {
        // Not kept means not usable, so don't leave the sign-in open on the server.
        if let Err(revoke) = auth::revoke(&client, &tokens.refresh_token).await {
            tracing::warn!(error = %revoke, "can't revoke the sign-in that couldn't be kept");
        }
        return Err(err.into());
    }
    if let Some(previous) = previous
        && let Err(err) = auth::revoke(&client, &previous.refresh_token).await
    {
        tracing::warn!(error = %err, "can't revoke the earlier sign-in");
    }
    // A default heap is one server's, so it goes when the server does.
    let dropped_heap = match args.shared.server {
        Some(_) => config::update_config(app.config_dir()?, |file| {
            let moved = file.server.as_deref() != Some(key.as_str());
            file.server = Some(key.clone());
            if moved { file.heap.take() } else { None }
        })?,
        None => None,
    };

    let me = client.get::<MeDto>(&["me"], NO_QUERY, &tokens.access_token).await?;
    match out.mode() {
        Mode::Json => out.json(&me.body)?,
        Mode::Quiet => {}
        Mode::Human => {
            let user = &me.value.user;
            out.line(&format!(
                "Signed in to {key} as {} (@{}).",
                user.display_name, user.username
            ))?;
            if let Some(heap) = dropped_heap {
                out.line(&format!(
                    "The default heap {heap} was on another server, so it's cleared: pick one with `upheap heap use`."
                ))?;
            }
        }
    }
    Ok(())
}
