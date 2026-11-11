//! Password reset links: the operator makes one for a user, and whoever holds it sets a new password and is signed
//! in (architecture.md, Authentication and authorization).
//!
//! A link's token is a 256-bit random value. The database holds only its SHA-256, so the link is shown once, to the
//! operator who made it.

use domain::audit::{AuditAction, EntityType};
use domain::password_reset::{ResetLink, ResetLinkError, ResetLinkState, ResetToken};
use domain::user::{User, validate_new_password};
use domain::validation::{Field, ValidationError};
use domain::{Timestamp, UserId};
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::auth::{Login, SessionGrant, hash_password, random_token, run_blocking, sha256_hex};
use crate::{Principal, ServiceError, Services};

/// A reset link just made. It holds the only copy of the token.
#[derive(Debug)]
pub struct ResetLinkMade {
    pub token: ResetToken,
    pub user: User,
    /// When the link stops working by itself.
    pub expires_at: Timestamp,
    /// Whether an earlier link of the user was still working and this one cancelled it.
    pub cancelled_earlier: bool,
}

/// A reset link as someone holding it sees it.
#[derive(Debug)]
pub struct ResetLinkView {
    pub state: ResetLinkState,
    /// The account the link is for, as it is now.
    pub user: User,
}

impl Services {
    /// Makes a reset link for the account `login` names: an email address when it has an `@`, a username otherwise.
    /// It works for an hour, and cancels any link of the user that still worked.
    ///
    /// Fails with [`ServiceError::NotFound`] when no account has that name. There is no caller to authorize: the
    /// operator runs it on the server, which is the permission. The audit entry is in the user's personal team with
    /// the user as its actor, since an entry needs an account as its actor and the operator has none; its diff says
    /// who made the link.
    pub async fn create_password_reset_link(&self, login: &str) -> Result<ResetLinkMade, ServiceError> {
        let login = Login::parse(login.trim()).ok_or(ServiceError::NotFound)?;
        let user = self.find_user_by_login(&login).await?.ok_or(ServiceError::NotFound)?;
        let personal_team = self.load_personal_team(&user).await?;

        let now = self.now();
        let expires_at = ResetLink::end_from(now).ok_or(ValidationError::TimestampOutOfRange)?;
        let token = ResetToken::new(random_token());
        let link = ResetLink {
            user_id: user.id,
            created_at: now,
            expires_at,
            used_at: None,
            cancelled_at: None,
        };
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        let cancelled = store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        store::password_resets::insert_reset_link(&txn, &sha256_hex(token.expose()), &link).await?;
        let entry = self.team_audit_entry(
            Principal { user_id: user.id },
            personal_team,
            AuditAction::PasswordResetLinkMade,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ "made_by": "operator" }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, cancelled_earlier = cancelled > 0, "password reset link made");
        Ok(ResetLinkMade {
            token,
            user,
            expires_at,
            cancelled_earlier: cancelled > 0,
        })
    }

    /// Opens a reset link: whose account it is for and whether it works, was used, was cancelled or has ended.
    /// Anyone holding the token may. Fails with [`ServiceError::NotFound`] for a token no link has.
    pub async fn open_password_reset(&self, token: &ResetToken) -> Result<ResetLinkView, ServiceError> {
        let link = self.load_reset_link(token).await?;
        let user = self.load_reset_link_user(&link).await?;
        Ok(ResetLinkView {
            state: link.state(self.now()),
            user,
        })
    }

    /// Sets a new password for the link's user, and signs the caller in as them, in one transaction.
    ///
    /// The link must work: an unknown token fails with [`ServiceError::NotFound`], and a used, cancelled or ended
    /// link with [`ServiceError::ResetLink`], before the password is looked at. Then an empty or unacceptable
    /// password fails with [`ValidationError`] and leaves the link working. The new password may equal the old one.
    ///
    /// Setting it marks the link used (a conditional update, so of two requests racing for one link exactly one wins
    /// and the other fails like a used link), and ends every sign-in of the user: their unused authorization codes,
    /// web sessions and token families, whichever browser or client made them. The caller gets a new session. Any
    /// other working link of the user is cancelled. The user's sign-in attempt count starts over, since the person
    /// holding the link has proved who they are. The reset is recorded in the user's personal team with the number of
    /// sign-ins it ended and nothing about the password or the token.
    pub async fn reset_password(&self, token: &ResetToken, new: &str) -> Result<SessionGrant, ServiceError> {
        let token_hash = sha256_hex(token.expose());
        let link = self.load_reset_link(token).await?;
        if let Some(error) = link.state(self.now()).error() {
            return Err(error.into());
        }
        if new.is_empty() {
            return Err(ValidationError::Empty { field: Field::Password }.into());
        }
        validate_new_password(new)?;
        let user = self.load_reset_link_user(&link).await?;
        let personal_team = self.load_personal_team(&user).await?;
        let new = new.to_owned();
        let new_hash = run_blocking(move || hash_password(&new)).await?;

        // After the hashing, which takes a while.
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::password_resets::use_reset_link(&txn, &token_hash, now).await? {
            // Used, cancelled or ended since it was read. Roll back before reading again, so the answer says which.
            txn.rollback().await.map_err(StoreError::from)?;
            let link = self.load_reset_link(token).await?;
            return Err(link.state(now).error().unwrap_or(ResetLinkError::Used).into());
        }
        let live_sessions = store::sessions::count_live_sessions(&txn, user.id, now).await?;
        let live_clients = store::oauth::list_live_token_families(&txn, user.id, now).await?.len();
        let ended_sign_ins = u32::try_from(live_sessions.saturating_add(live_clients as u64)).unwrap_or(u32::MAX);
        store::users::set_password(&txn, user.id, &new_hash, now).await?;
        store::oauth::expire_user_auth_codes(&txn, user.id, now).await?;
        store::sessions::delete_user_sessions(&txn, user.id).await?;
        let session = self.open_session(&txn, user.clone(), now).await?;
        store::oauth::revoke_user_token_families(&txn, user.id, now).await?;
        // There shouldn't be one, since making a link cancels the others; this keeps "one working link" true anyway.
        store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        let entry = self.team_audit_entry(
            Principal { user_id: user.id },
            personal_team,
            AuditAction::PasswordReset,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ "ended_sign_ins": ended_sign_ins }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        // The password is set by now, so failing to clear the count mustn't fail the request.
        if let Err(error) =
            store::sign_in_attempts::clear_sign_in_attempts(&self.db, &sha256_hex(user.email.as_str())).await
        {
            tracing::warn!(?error, "can't clear sign-in attempts");
        }
        tracing::info!(user_id = %user.id, ended_sign_ins, "password reset with a link");
        Ok(session)
    }

    async fn load_reset_link(&self, token: &ResetToken) -> Result<ResetLink, ServiceError> {
        store::password_resets::find_reset_link(&self.db, &sha256_hex(token.expose()))
            .await?
            .ok_or(ServiceError::NotFound)
    }

    async fn load_reset_link_user(&self, link: &ResetLink) -> Result<User, ServiceError> {
        Ok(store::users::find_user(&self.db, link.user_id)
            .await?
            .ok_or_else(|| corrupt_link(link.user_id, "link has no user"))?)
    }

    /// The id of the user's personal team, the team their account entries are recorded in (as `UserRegistered` is).
    /// Its slug is the username.
    pub(crate) async fn load_personal_team(&self, user: &User) -> Result<domain::TeamId, ServiceError> {
        let team = store::teams::find_team_by_slug(&self.db, &user.username)
            .await?
            .ok_or_else(|| StoreError::corrupt("users", &user.id.to_string(), "user has no personal team"))?;
        Ok(team.id)
    }
}

fn corrupt_link(user_id: UserId, problem: &'static str) -> StoreError {
    StoreError::corrupt("password_reset_links", &user_id.to_string(), problem)
}
