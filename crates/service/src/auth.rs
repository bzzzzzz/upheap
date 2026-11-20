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
use domain::audit::{AuditAction, AuditField, EntityType};
use domain::handle::Handle;
use domain::heap::Heap;
use domain::team::Team;
use domain::text::Name;
use domain::user::{DisplayName, Email, User, validate_new_password};
use domain::validation::{Field, ValidationError};
use rand::RngCore;
use serde_json::json;
use sha2::{Digest, Sha256};
use store::{StoreError, TransactionTrait};

use crate::email_confirmation::Registered;
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

    pub(crate) fn to_hash(&self) -> String {
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

/// The outcome of a password change.
#[derive(Debug)]
pub struct PasswordChanged {
    /// The caller's new session. The one the change was made from has ended.
    pub session: SessionGrant,
    /// How many other sign-ins the change ended that were still live: other web sessions, and sign-ins by clients
    /// such as the CLI.
    pub ended_sign_ins: u32,
}

/// What a new user enters to register.
pub struct Registration<'a> {
    pub email: &'a str,
    /// Becomes the slug of their personal team, and never changes.
    pub username: &'a str,
    pub display_name: &'a str,
    pub password: &'a str,
}

/// A registration that passed the input checks, password not yet hashed.
pub(crate) struct ValidRegistration {
    email: Email,
    username: Handle,
    display_name: DisplayName,
    /// The name of the personal team: the display name.
    team_name: Name,
    password: String,
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

/// Checks what a new user entered, in the order the form shows it. Cheap, and touches no database, so nothing is
/// made before the input is known to be good.
pub(crate) fn validate_registration(registration: Registration<'_>) -> Result<ValidRegistration, ServiceError> {
    let email = Email::parse(registration.email)?;
    let username = Handle::parse(registration.username, Field::Username)?;
    let display_name = DisplayName::parse(registration.display_name)?;
    validate_new_password(registration.password)?;
    let team_name = Name::parse(display_name.as_str())?;
    Ok(ValidRegistration {
        email,
        username,
        display_name,
        team_name,
        password: registration.password.to_owned(),
    })
}

/// Hashes the password of a checked registration. Costly (argon2), so it runs off the async threads and callers do
/// it as late as they can.
pub(crate) async fn hash_registration(valid: ValidRegistration) -> Result<NewAccount, ServiceError> {
    let password = valid.password;
    let password_hash = run_blocking(move || hash_password(&password)).await?;
    Ok(NewAccount {
        email: valid.email,
        username: valid.username,
        display_name: valid.display_name,
        team_name: valid.team_name,
        password_hash,
    })
}

/// What a person typed to name an account: an email address, or a username.
pub(crate) enum Login {
    Email(Email),
    Username(Handle),
}

impl Login {
    /// An email address when `login` has an `@` (a username can't), and a username, lowercased, otherwise. `None` for
    /// one that can't be either, which no account has.
    pub(crate) fn parse(login: &str) -> Option<Self> {
        if login.contains('@') {
            Email::parse(login).ok().map(Self::Email)
        } else {
            Handle::parse(&login.to_ascii_lowercase(), Field::Login)
                .ok()
                .map(Self::Username)
        }
    }

    /// What sign-in attempts at this name are counted under when no account has it: the address, or `@` and the
    /// username, which no address can be.
    fn unknown_key(&self) -> String {
        match self {
            Self::Email(email) => email.as_str().to_owned(),
            Self::Username(username) => format!("@{}", username.as_str()),
        }
    }
}

/// Hash of a fixed password, verified against when an email is unknown so that both cases take as long.
static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| hash_password("dummy password for timing").ok());

impl Services {
    /// Creates a local account with its personal team and heap, and signs the user in. On a server with mail set up
    /// it doesn't sign them in: the account is made unconfirmed, a confirmation link is mailed to its address after
    /// the transaction commits, and the answer is [`Registered::ConfirmationSent`].
    ///
    /// The team is named after the user and takes their username as its slug, and the user is its admin. Its first
    /// heap is `personal`.
    pub async fn register(&self, registration: Registration<'_>) -> Result<Registered, ServiceError> {
        self.ensure_password_sign_in()?;
        if !self.settings.allow_registration {
            return Err(ServiceError::RegistrationClosed);
        }
        let account = hash_registration(validate_registration(registration)?).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (user, team, heap) = self.insert_account(&txn, account, now).await?;
        let user_id = user.id;
        let access = self.open_new_account(&txn, user, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%user_id, team_id = %team.id, heap_id = %heap.id, "user registered");
        Ok(self.registered(access))
    }

    /// Signs in with email and password.
    ///
    /// Unknown emails and wrong passwords fail the same way, after the same amount of work. Attempts are limited
    /// per email address (`domain::sign_in`): one past the limit fails with
    /// [`ServiceError::TooManySignInAttempts`] before the password is checked. An empty email or password fails
    /// with [`ValidationError::Empty`] before anything is counted, so a form sent before a password manager filled
    /// it doesn't use up an attempt.
    ///
    /// On a server with mail set up, the right password for an account that hasn't confirmed its address fails with
    /// [`ServiceError::EmailNotConfirmed`] instead of opening a session, after a new confirmation link is mailed
    /// (see [`Services::mail_new_confirmation_link`]). The attempt stays counted, so the mails are limited too.
    pub async fn sign_in(&self, login: &str, password: &str) -> Result<SessionGrant, ServiceError> {
        self.ensure_password_sign_in()?;
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
        if let Some(mail) = &self.mail
            && !store::users::is_email_confirmed(&self.db, user.id).await?
        {
            self.mail_new_confirmation_link(mail, &user).await?;
            return Err(ServiceError::EmailNotConfirmed);
        }
        // The personal team's lock puts the session in turn with a provider deactivating the person, as for a
        // provider sign-in.
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        let grant = self.open_session(&txn, user, self.now(), None).await?;
        txn.commit().await.map_err(StoreError::from)?;
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
        let Some(login) = Login::parse(login) else {
            return Ok((None, None));
        };
        let user = self.find_user_by_login(&login).await?;
        let key = user
            .as_ref()
            .map_or_else(|| login.unknown_key(), |user| user.email.as_str().to_owned());
        Ok((user, Some(sha256_hex(&key))))
    }

    /// The account `login` names, if there is one.
    pub(crate) async fn find_user_by_login(&self, login: &Login) -> Result<Option<User>, ServiceError> {
        let user = match login {
            Login::Email(email) => store::users::find_user_by_email(&self.db, email).await?,
            Login::Username(username) => store::users::find_user_by_username(&self.db, username).await?,
        };
        Ok(user)
    }

    /// Counts a sign-in attempt for the address with this key, and refuses it when the address has used up its
    /// attempts for now. Also forgets a few addresses whose windows have ended.
    pub(crate) async fn count_sign_in_attempt(&self, key: &str) -> Result<(), ServiceError> {
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

    /// Adds one to the address's sign-in count without refusing, for a wrong password that wasn't a sign-in.
    pub(crate) async fn add_to_sign_in_count(&self, key: &str) -> Result<(), ServiceError> {
        let now = self.now();
        let window_end = self
            .settings
            .sign_in_limit
            .window_end(now)
            .ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::sign_in_attempts::count_sign_in_attempt(&txn, key, now, window_end).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(())
    }

    /// Changes the caller's password, ends their other sign-ins and records it, in one transaction.
    ///
    /// `session` is the caller's own web session. It is replaced: every session of the user is deleted, that one
    /// included, and a new one is opened in the same transaction, so a copy of the old cookie stops working too.
    /// Every token family of theirs is revoked, so nothing signed in with the old password keeps working. Their
    /// working password reset link is cancelled, so a copy of it can't be used to undo the change. The new
    /// password may equal the current one: that ends the other sign-ins all the same.
    ///
    /// Empty or unacceptable input fails with [`ValidationError`] before anything is counted. Then the attempt is
    /// counted per session (`domain::sign_in`, the same limit as sign-in), and one past the limit fails with
    /// [`ServiceError::TooManySignInAttempts`] before the password is checked. The account's own sign-in count is
    /// not checked, so someone else using up the account's attempts can't stop the owner changing it. A wrong
    /// `current` fails with [`ServiceError::WrongPassword`], changes nothing, and adds one to the account's sign-in
    /// count, so guessing through stolen sessions still uses up its sign-in attempts. A success clears both counts.
    ///
    /// The change is recorded in the user's personal team, whose one member is the user, with the number of
    /// sign-ins it ended and nothing about either password.
    pub async fn change_password(
        &self,
        principal: Principal,
        session: &SessionToken,
        current: &str,
        new: &str,
    ) -> Result<PasswordChanged, ServiceError> {
        if current.is_empty() {
            return Err(ValidationError::Empty {
                field: Field::CurrentPassword,
            }
            .into());
        }
        if new.is_empty() {
            return Err(ValidationError::Empty { field: Field::Password }.into());
        }
        validate_new_password(new)?;

        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let account_key = sha256_hex(user.email.as_str());
        let session_key = sha256_hex(&format!("session:{}", session.to_hash()));
        self.count_sign_in_attempt(&session_key).await?;

        let stored_hash = store::users::find_password_hash(&self.db, user.id)
            .await?
            .ok_or(ServiceError::WrongPassword)?;
        let current = current.to_owned();
        let verified_hash = stored_hash.clone();
        if !run_blocking(move || Ok(verify_password(&current, &verified_hash))).await? {
            self.add_to_sign_in_count(&account_key).await?;
            tracing::info!(user_id = %user.id, "password change rejected");
            return Err(ServiceError::WrongPassword);
        }
        let new = new.to_owned();
        let new_hash = run_blocking(move || hash_password(&new)).await?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let old_hash = session.to_hash();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        // The personal team first, as `update_display_name` takes team rows before the user's.
        store::teams::lock_team(&txn, personal_team).await?;
        // The caller's session may have ended while the password was being checked.
        if store::sessions::find_session_user(&txn, &old_hash, now)
            .await?
            .is_none()
        {
            return Err(ServiceError::Unauthenticated);
        }
        // Only while the stored hash is the one `current` was verified against. If another change got in first,
        // the password the caller gave is no longer the current one, which is what `WrongPassword` says.
        if !store::users::update_password(&txn, user.id, &stored_hash, &new_hash, now).await? {
            return Err(ServiceError::WrongPassword);
        }
        let ended_sign_ins = self.end_sign_ins(&txn, user.id, Some(&old_hash), now).await?;
        let session = self.open_session(&txn, user.clone(), now, None).await?;
        store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        let entry = self.team_audit_entry(
            principal,
            personal_team,
            AuditAction::PasswordChanged,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::EndedSignIns.code(): ended_sign_ins }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        // The password is changed by now, so failing to clear the counts mustn't fail the request.
        for key in [&session_key, &account_key] {
            if let Err(error) = store::sign_in_attempts::clear_sign_in_attempts(&self.db, key).await {
                tracing::warn!(?error, "can't clear sign-in attempts");
            }
        }
        tracing::info!(user_id = %user.id, ended_sign_ins, "password changed");
        Ok(PasswordChanged {
            session,
            ended_sign_ins,
        })
    }

    /// Resolves a session token to its user. Fails with [`ServiceError::Unauthenticated`] when the session is
    /// unknown or expired.
    pub async fn authenticate_session(&self, token: &SessionToken) -> Result<Principal, ServiceError> {
        let user_id = store::sessions::find_session_user(&self.db, &token.to_hash(), self.now())
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        self.ensure_not_local_user_while_passwords_off(user_id).await?;
        Ok(Principal { user_id, run: None })
    }

    /// While passwords are off, ends every sign-in of every local user in one transaction: their web sessions,
    /// OAuth token families and live personal tokens. Passwords stay, so turning passwords back on lets them sign in
    /// again. Returns how many sessions, families and tokens ended, and 0 when passwords are on. The server runs it
    /// at startup; a sign-in that slips in later (an older instance still running) is refused by the auth check.
    pub async fn end_local_sign_ins(&self) -> Result<u64, ServiceError> {
        if self.settings.password_sign_in {
            return Ok(0);
        }
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let mut ended = 0;
        for user_id in store::identities::list_users_without_identity(&txn).await? {
            ended += store::sessions::delete_user_sessions(&txn, user_id).await?;
            ended += store::oauth::revoke_user_token_families(&txn, user_id, now).await?;
            ended += store::personal_tokens::revoke_user_personal_tokens(&txn, user_id, now).await?;
        }
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(ended, "local sign-ins ended");
        Ok(ended)
    }

    /// Ends the session. Ending an unknown session succeeds.
    ///
    /// Returns the URL that ends the person's session at the provider too, when provider logout is on, the session
    /// kept an ID token and the provider names an end session endpoint. The session is gone before the provider is
    /// asked, and the provider failing only means `None`.
    pub async fn sign_out(&self, token: &SessionToken) -> Result<Option<String>, ServiceError> {
        let id_token = store::sessions::delete_session(&self.db, &token.to_hash()).await?;
        let (Some(oidc), Some(id_token)) = (&self.settings.oidc, id_token) else {
            return Ok(None);
        };
        if !oidc.provider_logout() {
            return Ok(None);
        }
        Ok(crate::oidc::provider_logout_url(oidc, &id_token).await)
    }

    /// Makes the account inside the caller's transaction: the user with their personal team and first heap, and their
    /// password. Fails like [`Services::insert_personal_user`] when the email or username is taken. The caller
    /// gives the account its way in with [`Services::open_new_account`]. Returns the user and the personal team and
    /// heap, for the caller's log line.
    pub(crate) async fn insert_account(
        &self,
        txn: &impl store::ConnectionTrait,
        account: NewAccount,
        now: Timestamp,
    ) -> Result<(User, Team, Heap), ServiceError> {
        let made = self
            .insert_personal_user(
                txn,
                account.email,
                account.username,
                account.display_name,
                account.team_name,
                now,
            )
            .await?;
        store::users::insert_password(txn, made.user.id, &account.password_hash, now).await?;
        Ok((made.user, made.team, made.heap))
    }

    /// Ends every sign-in of the account in the caller's transaction: its web sessions, its OAuth token families and
    /// open authorization codes, and its personal tokens. Returns how many sign-ins were live: sessions, OAuth token
    /// families and personal tokens. The session whose hash is `own_session` is ended too, but isn't counted: it is the
    /// caller's, who is told a new one is open.
    pub(crate) async fn end_sign_ins(
        &self,
        txn: &impl store::ConnectionTrait,
        user_id: domain::UserId,
        own_session: Option<&str>,
        now: Timestamp,
    ) -> Result<u32, ServiceError> {
        let live_sessions = match own_session {
            Some(hash) => store::sessions::count_live_sessions_except(txn, user_id, hash, now).await?,
            None => store::sessions::count_live_sessions(txn, user_id, now).await?,
        };
        let live_clients = store::oauth::list_live_token_families(txn, user_id, now).await?.len() as u64;
        store::oauth::expire_user_auth_codes(txn, user_id, now).await?;
        store::sessions::delete_user_sessions(txn, user_id).await?;
        store::oauth::revoke_user_token_families(txn, user_id, now).await?;
        let revoked_tokens = store::personal_tokens::revoke_user_personal_tokens(txn, user_id, now).await?;
        let ended = live_sessions
            .saturating_add(live_clients)
            .saturating_add(revoked_tokens);
        Ok(u32::try_from(ended).unwrap_or(u32::MAX))
    }

    /// Opens a session for the user, the one place every sign-in, registration and password reset does. Refuses with
    /// [`ServiceError::AccountDeactivated`] when the provider has deactivated the person (see [`crate::scim`]).
    pub(crate) async fn open_session(
        &self,
        db: &impl store::ConnectionTrait,
        user: User,
        now: Timestamp,
        id_token: Option<&str>,
    ) -> Result<SessionGrant, ServiceError> {
        self.refuse_deactivated(db, user.id).await?;
        let token = SessionToken::generate();
        let expires_at = now
            .checked_add(self.settings.session_ttl)
            .ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        store::sessions::insert_session(db, &token.to_hash(), user.id, now, expires_at, id_token).await?;
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

pub(crate) fn hash_password(password: &str) -> Result<String, ServiceError> {
    let mut salt_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|e| ServiceError::PasswordHashing(e.to_string()))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| ServiceError::PasswordHashing(e.to_string()))
}

/// `false` both for a wrong password and for a stored hash that doesn't parse.
pub(crate) fn verify_password(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Runs CPU-heavy work (argon2) off the async workers.
pub(crate) async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ServiceError> + Send + 'static,
) -> Result<T, ServiceError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| ServiceError::PasswordHashing(e.to_string()))?
}
