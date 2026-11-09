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
    save_tokens(store, server, &fresh).await?;
    tracing::debug!("refreshed the access token");
    Ok(fresh.access_token)
}

/// Exchanges the code the redirect URI received for tokens.
pub async fn exchange_code(
    client: &ApiClient,
    code: String,
    redirect_uri: &str,
    verifier: String,
) -> Result<TokenResponseDto, ClientError> {
    let form = TokenRequestForm {
        grant_type: "authorization_code".to_owned(),
        client_id: Some(CLI_CLIENT_ID.to_owned()),
        code: Some(code),
        redirect_uri: Some(redirect_uri.to_owned()),
        code_verifier: Some(verifier),
        refresh_token: None,
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
