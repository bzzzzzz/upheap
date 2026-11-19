//! Personal tokens (`domain::personal_token`): making, listing and revoking them for the signed-in user, and
//! resolving one presented as a bearer token.
//!
//! The secret is `uph_` and a random 256-bit value, made and hashed as session tokens are: the person holds the
//! secret, the database only its SHA-256. It is shown once, in the answer to making it.

use domain::oauth::DeviceName;
use domain::personal_token::{LAST_USED_PRECISION, PERSONAL_TOKEN_PREFIX, TokenExpiry, TokenState};
use domain::{PersonalTokenId, Timestamp, ValidationError};

use crate::auth::{SessionToken, random_token, sha256_hex};
use crate::oauth::OAuthSecret;
use crate::{Principal, ServiceError, Services};
use store::{StoreError, TransactionTrait};

/// One of the user's live personal tokens.
#[derive(Debug)]
pub struct PersonalToken {
    pub id: PersonalTokenId,
    pub name: String,
    pub created_at: Timestamp,
    /// `None` for a token that never expires.
    pub expires_at: Option<Timestamp>,
    /// `None` until it is first used. Precise to a minute.
    pub last_used_at: Option<Timestamp>,
}

impl From<store::personal_tokens::PersonalToken> for PersonalToken {
    fn from(token: store::personal_tokens::PersonalToken) -> Self {
        Self {
            id: token.id,
            name: token.name,
            created_at: token.created_at,
            expires_at: token.expires_at,
            last_used_at: token.last_used_at,
        }
    }
}

/// A token just made, with the only copy of its secret.
#[derive(Debug)]
pub struct MadePersonalToken {
    pub token: PersonalToken,
    pub secret: OAuthSecret,
}

impl Services {
    /// Makes a personal token for the caller, good for `expiry` from now. The caller's `session` is checked again
    /// in the transaction that inserts the token, after the personal team is locked as a password change locks it,
    /// so a token can't be made with a session that change has ended. Fails with
    /// [`ServiceError::Unauthenticated`] when the session is gone.
    pub async fn make_personal_token(
        &self,
        principal: Principal,
        session: &SessionToken,
        name: &DeviceName,
        expiry: TokenExpiry,
    ) -> Result<MadePersonalToken, ServiceError> {
        let now = self.now();
        let expires_at = expiry
            .duration()
            .map(|lifetime| {
                now.checked_add(lifetime)
                    .ok_or(ServiceError::Invalid(ValidationError::TimestampOutOfRange))
            })
            .transpose()?;
        let secret = OAuthSecret::from_client(format!("{PERSONAL_TOKEN_PREFIX}{}", random_token()));
        let id = PersonalTokenId::from_uuid(self.new_id(now));
        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if store::sessions::find_session_user(&txn, &session.to_hash(), now)
            .await?
            .is_none()
        {
            return Err(ServiceError::Unauthenticated);
        }
        store::personal_tokens::insert_personal_token(
            &txn,
            &store::personal_tokens::NewPersonalToken {
                id,
                user_id: principal.user_id,
                name: name.as_str(),
                token_hash: &sha256_hex(secret.expose()),
                created_at: now,
                expires_at,
            },
        )
        .await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(user_id = %principal.user_id, token_id = %id, "personal token made");
        Ok(MadePersonalToken {
            token: PersonalToken {
                id,
                name: name.as_str().to_owned(),
                created_at: now,
                expires_at,
                last_used_at: None,
            },
            secret,
        })
    }

    /// The caller's live personal tokens, newest first.
    pub async fn list_personal_tokens(&self, principal: Principal) -> Result<Vec<PersonalToken>, ServiceError> {
        let tokens = store::personal_tokens::list_live_personal_tokens(&self.db, principal.user_id, self.now()).await?;
        Ok(tokens.into_iter().map(PersonalToken::from).collect())
    }

    /// Revokes one of the caller's tokens. Succeeds when it was revoked or expired already. Fails with
    /// [`ServiceError::NotFound`] for a token that doesn't exist or isn't the caller's, which the answer doesn't
    /// tell apart.
    pub async fn revoke_personal_token(&self, principal: Principal, id: PersonalTokenId) -> Result<(), ServiceError> {
        let token = store::personal_tokens::find_personal_token(&self.db, id)
            .await?
            .filter(|token| token.user_id == principal.user_id)
            .ok_or(ServiceError::NotFound)?;
        let revoked = store::personal_tokens::revoke_personal_token(&self.db, token.id, self.now()).await?;
        tracing::info!(user_id = %principal.user_id, token_id = %token.id, revoked, "personal token revoked");
        Ok(())
    }

    /// Resolves a personal token to its user, as [`Services::authenticate_access_token`] does an OAuth one, and
    /// records that it was used. Fails with [`ServiceError::Unauthenticated`] for a token this server doesn't know,
    /// and with [`ServiceError::TokenRevoked`] or [`ServiceError::TokenExpired`] for one that is no longer live:
    /// only the holder of the secret can ask, so saying which leaks nothing.
    pub async fn authenticate_personal_token(&self, secret: &OAuthSecret) -> Result<Principal, ServiceError> {
        let now = self.now();
        let token = store::personal_tokens::find_personal_token_by_hash(&self.db, &sha256_hex(secret.expose()))
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        match TokenState::of(token.revoked_at, token.expires_at, now) {
            TokenState::Live => {}
            TokenState::Revoked => return Err(ServiceError::TokenRevoked),
            TokenState::Expired => return Err(ServiceError::TokenExpired),
        }
        self.ensure_not_local_user_while_passwords_off(token.user_id).await?;
        // The write is conditional too; skipping the call keeps a busy token from taking the write lock per request.
        let fresh = token
            .last_used_at
            .is_some_and(|used| now.checked_sub(LAST_USED_PRECISION).is_some_and(|stale| used > stale));
        if !fresh {
            store::personal_tokens::touch_personal_token(&self.db, token.id, now).await?;
        }
        Ok(Principal {
            user_id: token.user_id,
            run: None,
        })
    }
}
