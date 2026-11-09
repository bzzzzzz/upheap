//! Signing in: the OAuth authorization code flow with PKCE, the keychain, and refreshing tokens.
//!
//! Access tokens last 15 minutes and refresh tokens are good for one refresh each; presenting a spent one revokes the
//! whole sign-in. So the access token is kept with its expiry and reused while it's fresh, and a refresh happens
//! under an exclusive lock on a file in the config directory, after reading the tokens again: a second `upheap`
//! running at the same time waits, then finds the tokens the first one saved instead of spending the old refresh
//! token a second time.

pub mod login;
pub mod loopback;
pub mod pkce;
pub mod store;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use api_types::oauth::{CLI_CLIENT_ID, RevokeRequestForm, TokenRequestForm, TokenResponseDto};

use crate::client::{ApiClient, ClientError};
pub use store::{KeychainStore, MemoryTokenStore, StoredTokens, TokenStore, TokenStoreError};

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("not signed in to {server}")]
    NotSignedIn { server: String },
    #[error(transparent)]
    Store(#[from] TokenStoreError),
    #[error("can't lock {}", path.display())]
    Lock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Client(#[from] ClientError),
    /// A refresh worked but the keychain wouldn't take the new tokens, so the sign-in was ended.
    #[error("refreshed the sign-in but can't keep it, so it's ended")]
    RefreshNotKept(#[source] TokenStoreError),
    /// Signing out couldn't end the sign-in on the server, so it's kept, to try again.
    #[error("can't end the sign-in on the server, so it's kept")]
    Revoke(#[source] ClientError),
}

/// An access token for `server` (its [`crate::config::server_key`]) at `now`, in seconds since the Unix epoch: the
/// stored one while it's fresh, or a new one from refreshing, which is saved before it's returned.
///
/// Fails with [`AuthError::NotSignedIn`] when there's no sign-in, or the server no longer accepts its refresh token,
/// which is then forgotten.
pub async fn access_token(
    store: &Arc<dyn TokenStore>,
    lock_dir: &Path,
    client: &ApiClient,
    server: &str,
    now: i64,
) -> Result<String, AuthError> {
    let not_signed_in = || AuthError::NotSignedIn {
        server: server.to_owned(),
    };
    let stored = load_tokens(store, server).await?.ok_or_else(not_signed_in)?;
    if stored.has_fresh_access(now) {
        return Ok(stored.access_token);
    }
    let _lock = lock_tokens(lock_dir).await?;
    // Another `upheap` may have refreshed while this one waited for the lock.
    let stored = load_tokens(store, server).await?.ok_or_else(not_signed_in)?;
    if stored.has_fresh_access(now) {
        return Ok(stored.access_token);
    }
    let form = TokenRequestForm {
        grant_type: "refresh_token".to_owned(),
        client_id: Some(CLI_CLIENT_ID.to_owned()),
        refresh_token: Some(stored.refresh_token),
        ..TokenRequestForm::default()
    };
    let issued = match client.request_tokens(&form).await {
        Ok(issued) => issued,
        Err(ClientError::OAuthRefused { refusal, .. }) if refusal.error == "invalid_grant" => {
            tracing::info!(error = %refusal.error_description, "refresh token refused; forgetting the sign-in");
            delete_tokens(store, server).await?;
            return Err(not_signed_in());
        }
        Err(err) => return Err(err.into()),
    };
    let fresh = StoredTokens::issued(issued, now);
    if let Err(err) = save_tokens(store, server, &fresh).await {
        // The old refresh token is spent and the new one can't be kept: nobody can use this sign-in again, so end it.
        if let Err(revoke_err) = revoke(client, &fresh.refresh_token).await {
            tracing::warn!(error = %revoke_err, "can't revoke the refreshed sign-in that couldn't be kept");
        }
        return Err(AuthError::RefreshNotKept(err));
    }
    tracing::debug!("refreshed the access token");
    Ok(fresh.access_token)
}

/// The name to give the sign-in for a machine called `hostname`, which is what the server accepts: control characters
/// removed, trimmed, and cut to 64 characters. `None` when nothing is left. The server refuses a name that isn't
/// clean, so the CLI makes sure it is rather than fail a sign-in over a label.
pub fn device_name(hostname: &str) -> Option<String> {
    let printable: String = hostname.chars().filter(|c| !c.is_control()).collect();
    let cut: String = printable.trim().chars().take(DEVICE_NAME_MAX_CHARS).collect();
    let name = cut.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// The longest device name the server accepts (`domain::oauth::DeviceName`), in characters.
const DEVICE_NAME_MAX_CHARS: usize = 64;

/// Exchanges the code the redirect URI received for tokens. `device_name` names the machine in the user's list of
/// sign-ins.
pub async fn exchange_code(
    client: &ApiClient,
    code: String,
    redirect_uri: &str,
    verifier: String,
    device_name: Option<String>,
) -> Result<TokenResponseDto, ClientError> {
    let form = TokenRequestForm {
        grant_type: "authorization_code".to_owned(),
        client_id: Some(CLI_CLIENT_ID.to_owned()),
        code: Some(code),
        redirect_uri: Some(redirect_uri.to_owned()),
        code_verifier: Some(verifier),
        refresh_token: None,
        device_name,
    };
    client.request_tokens(&form).await
}

/// Ends the sign-in `refresh_token` belongs to, on the server.
pub async fn revoke(client: &ApiClient, refresh_token: &str) -> Result<(), ClientError> {
    let form = RevokeRequestForm {
        token: Some(refresh_token.to_owned()),
        token_type_hint: Some("refresh_token".to_owned()),
        client_id: Some(CLI_CLIENT_ID.to_owned()),
    };
    client.revoke(&form).await
}

/// Holds the token lock until dropped.
pub struct TokenLock {
    _file: File,
}

/// Takes the exclusive lock on `tokens.lock` in `dir`, waiting for any other `upheap` holding it.
pub async fn lock_tokens(dir: &Path) -> Result<TokenLock, AuthError> {
    let path = dir.join("tokens.lock");
    let opened = path.clone();
    let locked = tokio::task::spawn_blocking(move || -> std::io::Result<File> {
        std::fs::create_dir_all(opened.parent().unwrap_or(&opened))?;
        let file = File::options().create(true).truncate(false).write(true).open(&opened)?;
        file.lock()?;
        Ok(file)
    })
    .await
    .map_err(std::io::Error::other)
    .and_then(|locked| locked);
    match locked {
        Ok(file) => Ok(TokenLock { _file: file }),
        Err(source) => Err(AuthError::Lock { path, source }),
    }
}

/// The keychain blocks, so these run on the blocking pool.
pub async fn load_tokens(store: &Arc<dyn TokenStore>, server: &str) -> Result<Option<StoredTokens>, TokenStoreError> {
    let (store, server) = (Arc::clone(store), server.to_owned());
    blocking(move || store.load(&server)).await
}

pub async fn save_tokens(
    store: &Arc<dyn TokenStore>,
    server: &str,
    tokens: &StoredTokens,
) -> Result<(), TokenStoreError> {
    let (store, server, tokens) = (Arc::clone(store), server.to_owned(), tokens.clone());
    blocking(move || store.save(&server, &tokens)).await
}

pub async fn delete_tokens(store: &Arc<dyn TokenStore>, server: &str) -> Result<(), TokenStoreError> {
    let (store, server) = (Arc::clone(store), server.to_owned());
    blocking(move || store.delete(&server)).await
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, TokenStoreError> + Send + 'static,
) -> Result<T, TokenStoreError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| TokenStoreError::Unavailable(Box::new(err)))?
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_hostname_is_trimmed() {
        expect_that!(device_name("  ada-laptop.local \t"), some(eq("ada-laptop.local")));
    }

    #[gtest]
    fn control_characters_are_removed() {
        expect_that!(device_name("ada\nlaptop"), some(eq("adalaptop")));
        expect_that!(device_name("\u{7}ada\u{1b}[31m"), some(eq("ada[31m")));
    }

    #[gtest]
    fn a_long_hostname_is_cut_to_64_characters() {
        let name = device_name(&"é".repeat(100));

        expect_that!(name, some(eq(&"é".repeat(64))));
    }

    #[gtest]
    fn the_cut_is_trimmed_again() {
        let hostname = format!("{} tail", "a".repeat(63));

        expect_that!(device_name(&hostname), some(eq(&"a".repeat(63))));
    }

    #[gtest]
    fn a_blank_hostname_is_no_name() {
        expect_that!(device_name(""), none());
        expect_that!(device_name(" \n\t "), none());
    }
}
