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
    /// Counts an address change of the account, which mails a link, and refuses it with
    /// [`ServiceError::TooManyAddressChanges`] once the account has used up its changes for now. Shared by a change
    /// asked from Account and a correction on the Check your mail card, with the sign-in limit's numbers. Keyed by
    /// the user, so neither a changed address nor a new session starts it over, and never cleared, so it runs out
    /// with its window.
    pub(crate) async fn count_address_change(&self, user_id: UserId) -> Result<(), ServiceError> {
        self.count_sign_in_attempt(&sha256_hex(&format!("address-change:{user_id}")))
            .await
            .map_err(|error| match error {
                ServiceError::TooManySignInAttempts { retry_after } => {
                    ServiceError::TooManyAddressChanges { retry_after }
                }
                other => other,
            })
    }

    /// Asks to change the caller's email address to `email`, with a link mailed to it. `session` is the caller's
    /// own web session.
    ///
    /// Fails with [`ServiceError::NotFound`] when the server has no mail set up, before anything else. An empty
    /// `current_password` or `email`, or an `email` that isn't an address, fails with a [`ValidationError`] before
    /// anything is counted. Then the attempt is counted per session, as a password change counts it, and one past
    /// the limit fails with [`ServiceError::TooManySignInAttempts`]. A wrong password, or a user with none, fails
    /// with [`ServiceError::WrongPassword`], changes nothing and adds one to the account's sign-in count. The right
    /// one is counted as an address change of the account ([`Services::count_address_change`]), whether the address
    /// is free or taken, so the answer still says nothing about accounts.
    ///
    /// With the right password the answer is the same whether another account has the address or not, so it
    /// reveals nothing about accounts, and so is the history entry. Either way, in one transaction, the user's
    /// working email change links are cancelled. For a free address a new one is made, and after the transaction
    /// commits the mail is sent in a task of its own. For an address another account has, no link is made and no
    /// mail is sent. The user's own current address is treated as a free one.
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
        self.count_address_change(user.id).await?;

        let taken = store::users::find_user_by_email(&self.db, &new_email)
            .await?
            .is_some_and(|other| other.id != user.id);
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let now = self.now();
        let expires_at = EmailChangeLink::end_from(now).ok_or(ValidationError::TimestampOutOfRange)?;
        let token = ResetToken::new(random_token());

        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        // Cancelled for a taken address too, so the earlier link can't tell a taken address from a free one.
        let cancelled = store::email_changes::cancel_working_email_change_links(&txn, user.id, now).await?;
        if !taken {
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
            self.send_later(
                mail,
                change_mail(&user, mail, &new_email, &token),
                user.id,
                "email change",
            );
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
    /// working password reset link, since one mailed to the old address mustn't keep working. The new address is
    /// proven by the link, so it counts as confirmed. Sessions and tokens are
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

        self.send_later(
            mail,
            changed_mail(&user, mail, &link.new_email, self.operator_contact()),
            user.id,
            "email changed",
        );
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

/// The notice to the old address that the address changed, with the operator's contact line, when set, on its own
/// line under the sentence that says to ask whoever runs the server.
fn changed_mail(user: &User, mail: &MailSettings, new_email: &Email, operator_contact: Option<&str>) -> Mail {
    let contact = operator_contact.map(|line| format!("{line}\n")).unwrap_or_default();
    Mail {
        to: user.email.as_str().to_owned(),
        subject: "Your Upheap address changed".to_owned(),
        body: format!(
            "The address for @{username} on {host} changed from {old} to {new}.\n\nIf it wasn't you, ask whoever runs this Upheap server: they can make you a reset link.\n{contact}",
            username = user.username,
            host = mail.host(),
            old = user.email.as_str(),
            new = new_email.as_str(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::handle::Handle;
    use domain::user::DisplayName;
    use domain::{Timestamp, Version};
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    const UNCHANGED: &str = "The address for @sam on upheap.example.com changed from sam@old.example to sam@new.example.\n\nIf it wasn't you, ask whoever runs this Upheap server: they can make you a reset link.\n";

    fn notice(operator_contact: Option<&str>) -> Result<Mail> {
        let user = User {
            id: UserId::from_uuid(Uuid::nil()),
            username: Handle::parse("sam", Field::Username)?,
            email: Email::parse("sam@old.example")?,
            display_name: DisplayName::parse("Sam")?,
            created_at: Timestamp::EPOCH,
            version: Version::INITIAL,
        };
        let mail = MailSettings {
            mailer: Arc::new(mail::Mailer::new("log:", "Upheap <upheap@example.com>")?),
            base_url: "https://upheap.example.com".to_owned(),
        };
        Ok(changed_mail(
            &user,
            &mail,
            &Email::parse("sam@new.example")?,
            operator_contact,
        ))
    }

    #[gtest]
    fn the_changed_notice_adds_the_operator_contact_under_the_ask_line() -> Result<()> {
        let mail = notice(Some("Ask Sam in #it"))?;

        expect_that!(mail.body, eq(&format!("{UNCHANGED}Ask Sam in #it\n")));
        Ok(())
    }

    #[gtest]
    fn the_changed_notice_without_an_operator_contact_is_unchanged() -> Result<()> {
        let mail = notice(None)?;

        expect_that!(mail.to, eq("sam@old.example"));
        expect_that!(mail.body, eq(UNCHANGED));
        Ok(())
    }
}
