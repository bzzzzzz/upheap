//! Password reset links: the operator makes one for a user, and whoever holds it sets a new password and is signed
//! in (architecture.md, Authentication and authorization).
//!
//! A link's token is a 256-bit random value. The database holds only its SHA-256, so the link is shown once, to the
//! operator who made it.

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::oauth::RevokeReason;
use domain::password_reset::{ResetLink, ResetLinkError, ResetLinkState, ResetToken};
use domain::user::{Email, User, validate_new_password};
use domain::validation::{Field, ValidationError};
use domain::{Timestamp, UserId};
use mail::Mail;
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::auth::{Login, SessionGrant, hash_password, random_token, run_blocking, sha256_hex};
use crate::{MailSettings, Principal, ServiceError, Services};

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
        self.ensure_password_sign_in()?;
        let login = Login::parse(login.trim()).ok_or(ServiceError::NotFound)?;
        let user = self.find_user_by_login(&login).await?.ok_or(ServiceError::NotFound)?;
        self.make_reset_link(user, "operator").await
    }

    /// Asks for a reset link by mail, with the email address of the account. Answers the same whether an account
    /// has the address or not, so it reveals nothing about accounts.
    ///
    /// Fails with [`ServiceError::NotFound`] when the server has no mail set up, before anything else. An `email`
    /// that isn't an address fails with a [`ValidationError`] and counts nothing. Then the request is counted per
    /// address, under a key of its own so it never uses up sign-in attempts (the sign-in limit): one past the limit
    /// fails with [`ServiceError::TooManyResetRequests`]. The count is counted for known and unknown addresses
    /// alike, and runs out with its window.
    ///
    /// For an address an account has, the link is made as the operator's is, and the mail is sent after the
    /// transaction commits, in a task of its own: the answer never waits for the provider. Nothing is written or
    /// sent for an address no account has.
    pub async fn request_password_reset(&self, email: &str) -> Result<(), ServiceError> {
        self.ensure_password_sign_in()?;
        let Some(mail) = &self.mail else {
            return Err(ServiceError::NotFound);
        };
        let email = Email::parse(email)?;
        self.count_sign_in_attempt(&sha256_hex(&format!("reset:{}", email.as_str())))
            .await
            .map_err(|error| match error {
                ServiceError::TooManySignInAttempts { retry_after } => {
                    ServiceError::TooManyResetRequests { retry_after }
                }
                other => other,
            })?;
        let Some(user) = store::users::find_user_by_email(&self.db, &email).await? else {
            return Ok(());
        };
        let made = self.make_reset_link(user, "mail").await?;
        self.send_later(mail, reset_mail(&made.user, mail, &made.token), made.user.id, "reset");
        Ok(())
    }

    /// Makes a reset link for `user` that works for an hour, cancels their earlier working one, and records who
    /// made it (`made_by`, `"operator"` or `"mail"`), in one transaction.
    async fn make_reset_link(&self, user: User, made_by: &'static str) -> Result<ResetLinkMade, ServiceError> {
        let personal_team = self.load_personal_team(&self.db, &user).await?;
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
            Principal {
                user_id: user.id,
                run: None,
            },
            personal_team,
            AuditAction::PasswordResetLinkMade,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::MadeBy.code(): made_by }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, cancelled_earlier = cancelled > 0, made_by, "password reset link made");
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
    /// holding the link has proved who they are. The user's address counts as confirmed, as the link was mailed to it (an
    /// operator's link is the same kind of link). The reset is recorded in the user's personal team with the number of
    /// sign-ins it ended and nothing about the password or the token.
    pub async fn reset_password(&self, token: &ResetToken, new: &str) -> Result<SessionGrant, ServiceError> {
        self.ensure_password_sign_in()?;
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
        let personal_team = self.load_personal_team(&self.db, &user).await?;
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
        store::users::set_password(&txn, user.id, &new_hash, now).await?;
        // A link that came by mail proves the address. The operator's is the same kind of link.
        store::users::confirm_email(&txn, user.id).await?;
        let ended_sign_ins = self
            .end_sign_ins(&txn, user.id, None, Some(RevokeReason::PasswordChanged), now)
            .await?;
        let session = self.open_session(&txn, user.clone(), now, None).await?;
        // There shouldn't be one, since making a link cancels the others; this keeps "one working link" true anyway.
        store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        let entry = self.team_audit_entry(
            Principal {
                user_id: user.id,
                run: None,
            },
            personal_team,
            AuditAction::PasswordReset,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::EndedSignIns.code(): ended_sign_ins }),
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
    pub(crate) async fn load_personal_team(
        &self,
        db: &impl store::ConnectionTrait,
        user: &User,
    ) -> Result<domain::TeamId, ServiceError> {
        let team = store::teams::find_team_by_slug(db, &user.username)
            .await?
            .ok_or_else(|| StoreError::corrupt("users", &user.id.to_string(), "user has no personal team"))?;
        Ok(team.id)
    }
}

fn corrupt_link(user_id: UserId, problem: &'static str) -> StoreError {
    StoreError::corrupt("password_reset_links", &user_id.to_string(), problem)
}

/// The mail with a reset link in it.
fn reset_mail(user: &User, mail: &MailSettings, token: &ResetToken) -> Mail {
    let (host, base_url) = (mail.host(), &mail.base_url);
    Mail {
        to: user.email.as_str().to_owned(),
        subject: "Reset your Upheap password".to_owned(),
        body: format!(
            "Someone asked to reset the password for @{username} on {host}.\n\nOpen this link to set a new one. It works once, for an hour:\n{base_url}/reset/{token}\n\nIf it wasn't you, ignore this mail. Your password stays as it is.\n",
            username = user.username,
            token = token.expose(),
        ),
    }
}
