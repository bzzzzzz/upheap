//! Changing a user's email address: they ask from the Account page with their password, a link is mailed to the new
//! address, and applying the link changes the address (architecture.md, Authentication and authorization).
//!
//! A link's token is a 256-bit random value. The database holds only its SHA-256, so the link exists only in the
//! mail.

use domain::UserId;
use domain::audit::{AuditAction, AuditField, EntityType};
use domain::email_change::{EmailChangeLink, EmailChangeLinkError};
use domain::password_reset::ResetToken;
use domain::user::{Email, User};
use domain::validation::{Field, ValidationError};
use mail::Mail;
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::auth::{SessionToken, random_token, run_blocking, sha256_hex, verify_password};
use crate::{MailSettings, Principal, ServiceError, Services};

impl Services {
    /// Asks to change the caller's email address to `email`, with a link mailed to it. `session` is the caller's
    /// own web session.
    ///
    /// Fails with [`ServiceError::NotFound`] when the server has no mail set up, before anything else. An empty
    /// `current_password` or `email`, or an `email` that isn't an address, fails with a [`ValidationError`] before
    /// anything is counted. Then the attempt is counted per session, as a password change counts it, and one past
    /// the limit fails with [`ServiceError::TooManySignInAttempts`]. Unlike a password change a success doesn't
    /// clear the count, so it also limits the mails a session sends. A wrong password, or a user with none, fails
    /// with [`ServiceError::WrongPassword`], changes nothing and adds one to the account's sign-in count.
    ///
    /// With the right password the answer is the same whether another account has the address or not, so it
    /// reveals nothing about accounts, and so is the history entry. For a free address, in one transaction, the
    /// user's working email change links are cancelled and a new one made; after it commits the mail is sent in a
    /// task of its own. For an address another account has, nothing is written to links and no mail is sent. The
    /// user's own current address is treated as a free one.
    pub async fn request_email_change(
        &self,
        principal: Principal,
        session: &SessionToken,
        email: &str,
        current_password: &str,
    ) -> Result<(), ServiceError> {
        let Some(mail) = &self.mail else {
            return Err(ServiceError::NotFound);
        };
        if current_password.is_empty() {
            return Err(ValidationError::Empty {
                field: Field::CurrentPassword,
            }
            .into());
        }
        if email.is_empty() {
            return Err(ValidationError::Empty { field: Field::Email }.into());
        }
        let new_email = Email::parse(email)?;

        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let session_key = sha256_hex(&format!("session:{}", session.to_hash()));
        self.count_sign_in_attempt(&session_key).await?;

        let stored_hash = store::users::find_password_hash(&self.db, user.id)
            .await?
            .ok_or(ServiceError::WrongPassword)?;
        let current = current_password.to_owned();
        if !run_blocking(move || Ok(verify_password(&current, &stored_hash))).await? {
            self.add_to_sign_in_count(&sha256_hex(user.email.as_str())).await?;
            tracing::info!(user_id = %user.id, "email change rejected");
            return Err(ServiceError::WrongPassword);
        }

        let taken = store::users::find_user_by_email(&self.db, &new_email)
            .await?
            .is_some_and(|other| other.id != user.id);
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let now = self.now();
        let expires_at = EmailChangeLink::end_from(now).ok_or(ValidationError::TimestampOutOfRange)?;
        let token = ResetToken::new(random_token());

        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        let mut cancelled = 0;
        if !taken {
            cancelled = store::email_changes::cancel_working_email_change_links(&txn, user.id, now).await?;
            let link = EmailChangeLink {
                user_id: user.id,
                new_email: new_email.clone(),
                created_at: now,
                expires_at,
                used_at: None,
                cancelled_at: None,
            };
            store::email_changes::insert_email_change_link(&txn, &sha256_hex(token.expose()), &link).await?;
        }
        let entry = self.team_audit_entry(
            principal,
            personal_team,
            AuditAction::EmailChangeAsked,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::Email.code(): new_email.as_str() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        if !taken {
            self.send_later(change_mail(&user, mail, &new_email, &token), user.id, "email change");
        }
        tracing::info!(user_id = %user.id, cancelled_earlier = cancelled > 0, taken, "email change asked");
        Ok(())
    }

    /// Changes the address of the link's user to the one the link was mailed to, and tells the old address. Anyone
    /// holding the token may; it is not a sign-in.
    ///
    /// An unknown token fails with [`ServiceError::NotFound`], and a used, cancelled or ended link with
    /// [`ServiceError::EmailChangeLink`]. An address another account has by now fails with
    /// [`ServiceError::EmailTaken`], and the link keeps working.
    ///
    /// Applying marks the link used (a conditional update, so of two requests racing for one link exactly one wins
    /// and the other fails like a used link), sets the address and bumps the user's version, and cancels the user's
    /// working password reset link, since one mailed to the old address mustn't keep working. Sessions and tokens are
    /// left alone. The change is recorded in the user's personal team with the address before and after. After it
    /// commits the notice is sent, in a task of its own, to the old address. Returns the new address.
    pub async fn apply_email_change(&self, token: &ResetToken) -> Result<Email, ServiceError> {
        let Some(mail) = &self.mail else {
            return Err(ServiceError::NotFound);
        };
        let token_hash = sha256_hex(token.expose());
        let link = self.load_email_change_link(&token_hash).await?;
        if let Some(error) = link.state(self.now()).email_change_error() {
            return Err(error.into());
        }
        let user = store::users::find_user(&self.db, link.user_id)
            .await?
            .ok_or_else(|| corrupt_link(link.user_id, "link has no user"))?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::email_changes::use_email_change_link(&txn, &token_hash, now).await? {
            // Used, cancelled or ended since it was read. Roll back before reading again, so the answer says which.
            txn.rollback().await.map_err(StoreError::from)?;
            let link = self.load_email_change_link(&token_hash).await?;
            return Err(link
                .state(now)
                .email_change_error()
                .unwrap_or(EmailChangeLinkError::Used)
                .into());
        }
        // The address may have been taken since the ask. The unique index decides, and a refusal ends the transaction
        // on Postgres, so the link's use rolls back with it and the link keeps working.
        if let Err(error) = store::users::set_email(&txn, user.id, &link.new_email).await {
            return Err(match error {
                StoreError::Duplicate(_) => ServiceError::EmailTaken,
                other => other.into(),
            });
        }
        store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        let entry = self.team_audit_entry(
            Principal {
                user_id: user.id,
                run: None,
            },
            personal_team,
            AuditAction::EmailChanged,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::Email.code(): { "before": user.email.as_str(), "after": link.new_email.as_str() } }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        self.send_later(changed_mail(&user, mail, &link.new_email), user.id, "email changed");
        tracing::info!(user_id = %user.id, "email changed with a link");
        Ok(link.new_email)
    }

    async fn load_email_change_link(&self, token_hash: &str) -> Result<EmailChangeLink, ServiceError> {
        store::email_changes::find_email_change_link(&self.db, token_hash)
            .await?
            .ok_or(ServiceError::NotFound)
    }
}

fn corrupt_link(user_id: UserId, problem: &'static str) -> StoreError {
    StoreError::corrupt("email_change_links", &user_id.to_string(), problem)
}

/// The mail with the link in it, to the new address.
fn change_mail(user: &User, mail: &MailSettings, new_email: &Email, token: &ResetToken) -> Mail {
    Mail {
        to: new_email.as_str().to_owned(),
        subject: "Confirm your new Upheap address".to_owned(),
        body: format!(
            "@{username} asked to use this address for their account on {host}.\n\nOpen this link to confirm it. It works once, for a day:\n{base_url}/change-email/{token}\n\nIf it wasn't you, ignore this mail. Nothing changes.\n",
            username = user.username,
            host = mail.host(),
            base_url = mail.base_url,
            token = token.expose(),
        ),
    }
}

/// The notice to the old address that the address changed.
fn changed_mail(user: &User, mail: &MailSettings, new_email: &Email) -> Mail {
    Mail {
        to: user.email.as_str().to_owned(),
        subject: "Your Upheap address changed".to_owned(),
        body: format!(
            "The address for @{username} on {host} changed from {old} to {new}.\n\nIf it wasn't you, ask whoever runs this Upheap server: they can make you a reset link.\n",
            username = user.username,
            host = mail.host(),
            old = user.email.as_str(),
            new = new_email.as_str(),
        ),
    }
}
