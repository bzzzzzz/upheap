//! Where a sign-in's tokens are kept: the OS keychain, one entry per server, never a plain file.

use std::collections::HashMap;
use std::sync::Mutex;

use api_types::oauth::TokenResponseDto;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use super::security_tool::SecurityTool;

/// A sign-in to one server: the refresh token, and the access token last issued with its expiry, so most commands
/// don't refresh. Refreshing spends the refresh token, so fewer refreshes also mean fewer chances to race one.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTokens {
    pub refresh_token: String,
    pub access_token: String,
    /// When the access token expires, in seconds since the Unix epoch, by this machine's clock.
    pub access_expires_at: i64,
}

/// Says when the access token expires, never what either token is.
impl std::fmt::Debug for StoredTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredTokens")
            .field("refresh_token", &"<redacted>")
            .field("access_token", &"<redacted>")
            .field("access_expires_at", &self.access_expires_at)
            .finish()
    }
}

impl StoredTokens {
    /// The tokens of `response`, issued at `now` (seconds since the Unix epoch).
    pub fn issued(response: TokenResponseDto, now: i64) -> Self {
        Self {
            refresh_token: response.refresh_token,
            access_token: response.access_token,
            access_expires_at: now.saturating_add(response.expires_in),
        }
    }

    /// Whether the access token is good for a command started at `now`: it has at least a minute left, which covers
    /// the command's requests.
    pub fn has_fresh_access(&self, now: i64) -> bool {
        self.access_expires_at > now.saturating_add(ACCESS_MARGIN_SECONDS)
    }
}

const ACCESS_MARGIN_SECONDS: i64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    /// There's no keychain to use, or it refused access. The CLI never falls back to a plain file.
    #[error("can't use the system keychain to keep the sign-in")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("the keychain entry for {server} isn't one the CLI wrote")]
    Corrupt { server: String },
}

/// Keeps one [`StoredTokens`] per server, keyed by the server's URL.
pub trait TokenStore: Send + Sync {
    /// The sign-in to `server`, or `None` when there's none.
    fn load(&self, server: &str) -> Result<Option<StoredTokens>, TokenStoreError>;
    /// Replaces the sign-in to `server`.
    fn save(&self, server: &str, tokens: &StoredTokens) -> Result<(), TokenStoreError>;
    /// Forgets the sign-in to `server`. Forgetting one there isn't succeeds.
    fn delete(&self, server: &str) -> Result<(), TokenStoreError>;
}

/// The OS keychain: the macOS keychain, the Windows credential manager, or the Secret Service on Linux.
///
/// On macOS it's reached through the system's `security` tool, not from this process, so the entry doesn't belong
/// to one build of `upheap` ([`super::security_tool`]).
pub struct KeychainStore;

const KEYCHAIN_SERVICE: &str = "upheap";

#[cfg(not(target_os = "macos"))]
impl KeychainStore {
    fn entry(server: &str) -> Result<keyring::Entry, TokenStoreError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, server).map_err(unavailable)
    }

    fn read(server: &str) -> Result<Option<String>, TokenStoreError> {
        match Self::entry(server)?.get_password() {
            Ok(text) => Ok(Some(text)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(unavailable(err)),
        }
    }

    fn write(server: &str, text: &str) -> Result<(), TokenStoreError> {
        Self::entry(server)?.set_password(text).map_err(unavailable)
    }

    fn remove(server: &str) -> Result<(), TokenStoreError> {
        match Self::entry(server)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(unavailable(err)),
        }
    }
}

#[cfg(target_os = "macos")]
impl KeychainStore {
    fn read(server: &str) -> Result<Option<String>, TokenStoreError> {
        SecurityTool::system()
            .find(KEYCHAIN_SERVICE, server)
            .map_err(unavailable)
    }

    fn write(server: &str, text: &str) -> Result<(), TokenStoreError> {
        SecurityTool::system()
            .add(KEYCHAIN_SERVICE, server, text.as_bytes())
            .map_err(unavailable)
    }

    fn remove(server: &str) -> Result<(), TokenStoreError> {
        SecurityTool::system()
            .delete(KEYCHAIN_SERVICE, server)
            .map_err(unavailable)
    }
}

impl TokenStore for KeychainStore {
    fn load(&self, server: &str) -> Result<Option<StoredTokens>, TokenStoreError> {
        let Some(text) = Self::read(server)? else {
            return Ok(None);
        };
        parse_entry(&text).map(Some).ok_or_else(|| TokenStoreError::Corrupt {
            server: server.to_owned(),
        })
    }

    fn save(&self, server: &str, tokens: &StoredTokens) -> Result<(), TokenStoreError> {
        // Three strings and a number always serialize.
        let text = serde_json::to_string(tokens).map_err(|err| unavailable(Box::new(err)))?;
        Self::write(server, &text)
    }

    fn delete(&self, server: &str) -> Result<(), TokenStoreError> {
        Self::remove(server)
    }
}

/// The sign-in a keychain entry holds: JSON, or JSON in hexadecimal, which is how macOS's `security` prints an
/// entry that has anything but printable ASCII in it.
fn parse_entry(text: &str) -> Option<StoredTokens> {
    serde_json::from_str(text).ok().or_else(|| {
        let bytes = unhex(text)?;
        serde_json::from_slice(&bytes).ok()
    })
}

/// The bytes `text` spells in hexadecimal, or `None` when it isn't hexadecimal.
pub(super) fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

fn unavailable(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> TokenStoreError {
    TokenStoreError::Unavailable(err.into())
}

/// Tokens in memory, for tests and for programs that embed the CLI and keep the sign-in themselves.
#[derive(Default)]
pub struct MemoryTokenStore {
    tokens: Mutex<HashMap<String, StoredTokens>>,
}

impl MemoryTokenStore {
    fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<String, StoredTokens>> {
        // A poisoned lock only means another thread panicked holding it; the map is still whole.
        self.tokens.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self, server: &str) -> Result<Option<StoredTokens>, TokenStoreError> {
        Ok(self.tokens().get(server).cloned())
    }

    fn save(&self, server: &str, tokens: &StoredTokens) -> Result<(), TokenStoreError> {
        self.tokens().insert(server.to_owned(), tokens.clone());
        Ok(())
    }

    fn delete(&self, server: &str) -> Result<(), TokenStoreError> {
        self.tokens().remove(server);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn an_entry_is_read_as_json_or_as_json_in_hexadecimal() {
        // Arrange
        let json = r#"{"refresh_token":"r","access_token":"a","access_expires_at":42}"#;
        let hex: String = json.bytes().map(|byte| format!("{byte:02x}")).collect();

        // Act
        let (plain, from_hex) = (parse_entry(json), parse_entry(&hex));

        // Assert
        expect_that!(plain.as_ref().map(|tokens| tokens.access_expires_at), some(eq(42)));
        expect_that!(from_hex, eq(&plain));
        expect_that!(parse_entry("not a sign-in"), none());
        expect_that!(parse_entry("7b7d"), none());
    }

    #[gtest]
    fn unhex_reads_hexadecimal_and_nothing_else() {
        expect_that!(unhex("007bFF0a"), some(eq(&vec![0x00, 0x7b, 0xff, 0x0a])));
        expect_that!(unhex(""), some(is_empty()));
        expect_that!(unhex("7b2"), none());
        expect_that!(unhex("zz"), none());
        expect_that!(unhex("\u{e9}"), none());
    }

    #[gtest]
    fn debug_never_shows_a_token() {
        let tokens = StoredTokens {
            refresh_token: "the-refresh".to_owned(),
            access_token: "the-access".to_owned(),
            access_expires_at: 42,
        };

        let shown = format!("{tokens:?}");

        expect_that!(shown, not(contains_substring("the-refresh")));
        expect_that!(shown, not(contains_substring("the-access")));
        expect_that!(shown, contains_substring("access_expires_at: 42"));
    }
}
