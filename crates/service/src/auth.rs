//! Local accounts and web sessions.
//!
//! Session tokens are 256-bit random values. The client holds the token; the database holds only its SHA-256,
//! so a leaked database can't be used to sign in.

use std::sync::LazyLock;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::audit::{AuditAction, EntityType};
use domain::heap::{Heap, HeapPolicy};
use domain::team::{Membership, Role, Team};
use domain::text::Name;
use domain::user::{DisplayName, Email, User, validate_new_password};
use domain::{HeapId, TeamId, Timestamp, UserId, Version};
use rand::RngCore;
use serde_json::json;
use sha2::{Digest, Sha256};
use store::audit::NewAuditEntry;
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
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
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
    pub display_name: &'a str,
    pub password: &'a str,
}

/// Hash of a fixed password, verified against when an email is unknown so that both cases take as long.
static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| hash_password("dummy password for timing").ok());

impl Services {
    /// Creates a local account with its personal team and heap, and signs the user in.
    ///
    /// The team and heap are named after the user, and the user is the team's admin.
    pub async fn register(&self, registration: Registration<'_>) -> Result<SessionGrant, ServiceError> {
        if !self.settings.allow_registration {
            return Err(ServiceError::RegistrationClosed);
        }
        let email = Email::parse(registration.email)?;
        let display_name = DisplayName::parse(registration.display_name)?;
        validate_new_password(registration.password)?;
        let personal_name = Name::parse(display_name.as_str())?;
        let password = registration.password.to_owned();
        let password_hash = run_blocking(move || hash_password(&password)).await?;

        let now = self.now();
        let user = User {
            id: UserId::from_uuid(self.new_id(now)),
            email,
            display_name,
            created_at: now,
            version: Version::INITIAL,
        };
        let team = Team {
            id: TeamId::from_uuid(self.new_id(now)),
            name: personal_name.clone(),
            created_at: now,
            version: Version::INITIAL,
        };
        let membership = Membership {
            team_id: team.id,
            user_id: user.id,
            role: Role::Admin,
            joined_at: now,
        };
        let heap = Heap {
            id: HeapId::from_uuid(self.new_id(now)),
            team_id: team.id,
            name: personal_name,
            policy: HeapPolicy::default(),
            archived_at: None,
            version: Version::INITIAL,
        };

        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::users::insert_user(&txn, &user).await.map_err(|e| match e {
            StoreError::Duplicate(_) => ServiceError::EmailTaken,
            other => other.into(),
        })?;
        store::users::insert_password(&txn, user.id, &password_hash, now).await?;
        store::teams::insert_team(&txn, &team).await?;
        store::teams::insert_membership(&txn, &membership).await?;
        store::heaps::insert_heap(&txn, &heap, now).await?;
        let entry = |action, entity_type, entity_id, heap_id, diff| NewAuditEntry {
            id: self.new_id(now),
            team_id: team.id,
            heap_id,
            actor: user.id,
            action,
            entity_type,
            entity_id,
            diff,
            created_at: now,
        };
        let entries = [
            entry(
                AuditAction::UserRegistered,
                EntityType::User,
                *user.id.as_uuid(),
                None,
                json!({ "display_name": user.display_name.as_str() }),
            ),
            entry(
                AuditAction::TeamCreated,
                EntityType::Team,
                *team.id.as_uuid(),
                None,
                json!({ "name": team.name.as_str() }),
            ),
            entry(
                AuditAction::MemberAdded,
                EntityType::User,
                *user.id.as_uuid(),
                None,
                json!({ "role": Role::Admin.code() }),
            ),
            entry(
                AuditAction::HeapCreated,
                EntityType::Heap,
                *heap.id.as_uuid(),
                Some(heap.id),
                json!({ "name": heap.name.as_str() }),
            ),
        ];
        for entry in &entries {
            store::audit::insert_audit_entry(&txn, entry).await?;
        }
        let grant = self.open_session(&txn, user, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %grant.user.id, team_id = %team.id, heap_id = %heap.id, "user registered");
        Ok(grant)
    }

    /// Signs in with email and password.
    ///
    /// Unknown emails and wrong passwords fail the same way, after the same amount of work. Attempts are limited
    /// per email address (`domain::sign_in`): one past the limit fails with
    /// [`ServiceError::TooManySignInAttempts`] before the password is checked.
    pub async fn sign_in(&self, email: &str, password: &str) -> Result<SessionGrant, ServiceError> {
        let email = Email::parse(email).ok();
        // Only a well-formed address can have an account, so only those are counted.
        let attempts_key = email.as_ref().map(|email| sha256_hex(email.as_str()));
        if let Some(key) = &attempts_key {
            self.count_sign_in_attempt(key).await?;
        }
        let user = match &email {
            Some(email) => store::users::find_user_by_email(&self.db, email).await?,
            None => None,
        };
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

    async fn open_session(
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

fn sha256_hex(value: &str) -> String {
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
