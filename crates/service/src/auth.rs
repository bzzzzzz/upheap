//! Local accounts and web sessions.
//!
//! Session tokens are 256-bit random values. The client holds the token; the database holds only its SHA-256,
//! so a leaked database can't be used to sign in.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::Timestamp;
use domain::handle::Handle;
use domain::text::Name;
use domain::user::{DisplayName, Email, User, validate_new_password};
use domain::validation::{Field, ValidationError};
use rand::RngCore;
use sha2::{Digest, Sha256};
use store::{StoreError, TransactionTrait};

use crate::{Principal, ServiceError, Services};

/// A session token as sent to the client. Never log it.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn from_client(raw: String) -> Self {
        Self(raw)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    fn generate() -> Self {
        Self(random_token())
    }

    fn to_hash(&self) -> String {
        sha256_hex(&self.0)
    }
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionToken(<redacted>)")
    }
}

/// A new session for a signed-in user.
#[derive(Debug)]
pub struct SessionGrant {
    pub token: SessionToken,
    pub user: User,
    pub expires_at: Timestamp,
}

/// What a new user enters to register.
pub struct Registration<'a> {
    pub email: &'a str,
    /// Becomes the slug of their personal team, and never changes.
    pub username: &'a str,
    pub display_name: &'a str,
    pub password: &'a str,
}

/// A registration that passed the input checks, with its password hashed.
pub(crate) struct NewAccount {
    pub email: Email,
    pub username: Handle,
    pub display_name: DisplayName,
    /// The name of the personal team: the display name.
    pub team_name: Name,
    pub password_hash: String,
}

/// Checks what a new user entered, in the order the form shows it, and hashes the password. Touches no database,
/// so nothing is made before the input is known to be good.
pub(crate) async fn parse_registration(registration: Registration<'_>) -> Result<NewAccount, ServiceError> {
    let email = Email::parse(registration.email)?;
    let username = Handle::parse(registration.username, Field::Username)?;
    let display_name = DisplayName::parse(registration.display_name)?;
    validate_new_password(registration.password)?;
    let team_name = Name::parse(display_name.as_str())?;
    let password = registration.password.to_owned();
    let password_hash = run_blocking(move || hash_password(&password)).await?;
    Ok(NewAccount {
        email,
        username,
        display_name,
        team_name,
        password_hash,
    })
}

/// Hash of a fixed password, verified against when an email is unknown so that both cases take as long.
static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| hash_password("dummy password for timing").ok());

impl Services {
    /// Creates a local account with its personal team and heap, and signs the user in.
    ///
    /// The team is named after the user and takes their username as its slug, and the user is its admin. Its first
    /// heap is `personal`.
    pub async fn register(&self, registration: Registration<'_>) -> Result<SessionGrant, ServiceError> {
        if !self.settings.allow_registration {
            return Err(ServiceError::RegistrationClosed);
        }
        let account = parse_registration(registration).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let made = self
            .insert_personal_user(
                &txn,
                account.email,
                account.username,
                account.display_name,
                account.team_name,
                now,
            )
            .await?;
        store::users::insert_password(&txn, made.user.id, &account.password_hash, now).await?;
        let grant = self.open_session(&txn, made.user, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %grant.user.id, team_id = %made.team.id, heap_id = %made.heap.id, "user registered");
        Ok(grant)
    }

    /// Signs in with email and password.
    ///
    /// Unknown emails and wrong passwords fail the same way, after the same amount of work. Attempts are limited
    /// per email address (`domain::sign_in`): one past the limit fails with
    /// [`ServiceError::TooManySignInAttempts`] before the password is checked. An empty email or password fails
    /// with [`ValidationError::Empty`] before anything is counted, so a form sent before a password manager filled
    /// it doesn't use up an attempt.
    pub async fn sign_in(&self, login: &str, password: &str) -> Result<SessionGrant, ServiceError> {
        let login = login.trim();
        if login.is_empty() {
            return Err(ValidationError::Empty { field: Field::Login }.into());
        }
        if password.is_empty() {
            return Err(ValidationError::Empty { field: Field::Password }.into());
        }
        let (user, attempts_key) = self.find_sign_in_user(login).await?;
        if let Some(key) = &attempts_key {
            self.count_sign_in_attempt(key).await?;
        }
        let stored_hash = match &user {
            Some(user) => store::users::find_password_hash(&self.db, user.id).await?,
            None => None,
        };
        let known = stored_hash.is_some();
        let hash = stored_hash
            .or_else(|| DUMMY_HASH.clone())
            .ok_or_else(|| ServiceError::PasswordHashing("no dummy hash".into()))?;
        let password = password.to_owned();
        let verified = run_blocking(move || Ok(verify_password(&password, &hash))).await?;

        let Some(user) = user.filter(|_| known && verified) else {
            tracing::info!("sign-in rejected");
            return Err(ServiceError::InvalidCredentials);
        };
        let grant = self.open_session(&self.db, user, self.now()).await?;
        // The session exists by now, so failing to clear the count mustn't fail the sign-in; the count then just
        // runs out with its window.
        if let Some(key) = &attempts_key
            && let Err(error) = store::sign_in_attempts::clear_sign_in_attempts(&self.db, key).await
        {
            tracing::warn!(?error, "can't clear sign-in attempts");
        }
        tracing::info!(user_id = %grant.user.id, "user signed in");
        Ok(grant)
    }

    /// The account `login` names, by email address when it has an `@` (a username can't) and by username otherwise,
    /// and the key its attempts are counted under. An account's attempts count under its email address whichever way
    /// it's named, so switching between the two gets no more tries. A name no account has is counted too, under the
    /// address or `@` and the username, which no address can be; one that can't be an address or username at all
    /// can't have an account, so it isn't.
    async fn find_sign_in_user(&self, login: &str) -> Result<(Option<User>, Option<String>), ServiceError> {
        let (user, unknown_key) = if login.contains('@') {
            let Ok(email) = Email::parse(login) else {
                return Ok((None, None));
            };
            let user = store::users::find_user_by_email(&self.db, &email).await?;
            (user, email.as_str().to_owned())
        } else {
            let Ok(username) = Handle::parse(&login.to_ascii_lowercase(), Field::Login) else {
                return Ok((None, None));
            };
            let user = store::users::find_user_by_username(&self.db, &username).await?;
            (user, format!("@{}", username.as_str()))
        };
        let key = user.as_ref().map_or(unknown_key, |user| user.email.as_str().to_owned());
        Ok((user, Some(sha256_hex(&key))))
    }

    /// Counts a sign-in attempt for the address with this key, and refuses it when the address has used up its
    /// attempts for now. Also forgets a few addresses whose windows have ended.
    async fn count_sign_in_attempt(&self, key: &str) -> Result<(), ServiceError> {
        let now = self.now();
        let limit = self.settings.sign_in_limit;
        let window_end = limit
            .window_end(now)
            .ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        // Housekeeping: a failure here mustn't stop anyone signing in. A few rows per attempt keep up with the
        // attempts that add them.
        if let Err(error) = store::sign_in_attempts::delete_ended_sign_in_windows(&self.db, now, 10).await {
            tracing::warn!(?error, "can't delete ended sign-in windows");
        }
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let attempts = store::sign_in_attempts::count_sign_in_attempt(&txn, key, now, window_end).await?;
        txn.commit().await.map_err(StoreError::from)?;
        limit.check(attempts, now).map_err(|retry_after| {
            tracing::warn!(attempts = attempts.count, "sign-in refused: too many attempts");
            ServiceError::TooManySignInAttempts { retry_after }
        })
    }

    /// Resolves a session token to its user. Fails with [`ServiceError::Unauthenticated`] when the session is
    /// unknown or expired.
    pub async fn authenticate_session(&self, token: &SessionToken) -> Result<Principal, ServiceError> {
        let user_id = store::sessions::find_session_user(&self.db, &token.to_hash(), self.now())
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        Ok(Principal { user_id })
    }

    /// Ends the session. Ending an unknown session succeeds.
    pub async fn sign_out(&self, token: &SessionToken) -> Result<(), ServiceError> {
        store::sessions::delete_session(&self.db, &token.to_hash()).await?;
        Ok(())
    }

    pub(crate) async fn open_session(
        &self,
        db: &impl store::ConnectionTrait,
        user: User,
        now: Timestamp,
    ) -> Result<SessionGrant, ServiceError> {
        let token = SessionToken::generate();
        let expires_at = now
            .checked_add(self.settings.session_ttl)
            .ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        store::sessions::insert_session(db, &token.to_hash(), user.id, now, expires_at).await?;
        Ok(SessionGrant {
            token,
            user,
            expires_at,
        })
    }
}

/// A new random 256-bit token, base64url without padding: the form of session tokens and of OAuth codes and tokens.
pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The SHA-256 of `value` in lowercase hex: what is stored in place of a token, so a leaked database can't be used
/// to sign in.
pub(crate) fn sha256_hex(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn hash_password(password: &str) -> Result<String, ServiceError> {
    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|e| ServiceError::PasswordHashing(e.to_string()))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| ServiceError::PasswordHashing(e.to_string()))
}

/// `false` both for a wrong password and for a stored hash that doesn't parse.
fn verify_password(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Runs CPU-heavy work (argon2) off the async workers.
async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ServiceError> + Send + 'static,
) -> Result<T, ServiceError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| ServiceError::PasswordHashing(e.to_string()))?
}
