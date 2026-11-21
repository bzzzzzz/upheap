//! Confirming the address of a new account: on a server with mail set up, an account made with a password can't
//! sign in until it opens the link mailed to its address (architecture.md, Authentication and authorization).
//!
//! A link's token is a 256-bit random value. The database holds only its SHA-256, so the link exists only in the
//! mail. The stored link is a [`ResetLink`], and a link that doesn't work fails like an email change link.

use std::sync::Arc;

use domain::Timestamp;
use domain::UserId;
use domain::audit::{AuditAction, AuditField, EntityType};
use domain::email_change::{EmailChangeLinkError, confirmation_link_end_from};
use domain::handle::Handle;
use domain::password_reset::{ResetLink, ResetToken};
use domain::user::{Email, User};
use domain::validation::{Field, ValidationError};
use mail::Mail;
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::auth::{Login, SessionGrant, random_token, sha256_hex};
use crate::{MailSettings, Principal, ServiceError, Services};

/// What registering gave a new account.
#[derive(Debug)]
pub enum Registered {
    /// The account is signed in: the server has no mail set up.
    SignedIn(SessionGrant),
    /// The account must confirm its address first: a link was mailed to it, and nobody is signed in.
    ConfirmationSent(User),
}

/// What the operator's confirmation did.
#[derive(Debug)]
pub enum OperatorConfirmed {
    Confirmed(User),
    AlreadyConfirmed(User),
}

/// Where a new confirmation link went, and the account it is for.
#[derive(Debug)]
pub struct ConfirmationAddress {
    /// The address the account has now, as parsed.
    pub email: Email,
    pub username: Handle,
}

/// What a new account got inside its transaction, before the transaction commits.
pub(crate) enum NewAccountAccess {
    SignedIn(SessionGrant),
    /// Not confirmed: the link to mail once the transaction has committed.
    Confirm {
        user: User,
        token: ResetToken,
        mail: Arc<MailSettings>,
    },
}

impl Services {
    /// Gives a new account what the server lets it have, inside the caller's transaction. With mail set up that is
    /// no session: the account is marked unconfirmed and a link is made for it. Without mail it is a session.
    pub(crate) async fn open_new_account(
        &self,
        txn: &impl store::ConnectionTrait,
        user: User,
        now: Timestamp,
    ) -> Result<NewAccountAccess, ServiceError> {
        let Some(mail) = &self.mail else {
            return Ok(NewAccountAccess::SignedIn(
                self.open_session(txn, user, now, None).await?,
            ));
        };
        store::users::set_unconfirmed(txn, user.id, now).await?;
        let token = make_confirmation_link(txn, user.id, now).await?;
        Ok(NewAccountAccess::Confirm {
            user,
            token,
            mail: Arc::clone(mail),
        })
    }

    /// Ends a registration once its transaction has committed: mails the link of an unconfirmed account, in a task
    /// of its own.
    pub(crate) fn registered(&self, access: NewAccountAccess) -> Registered {
        match access {
            NewAccountAccess::SignedIn(grant) => Registered::SignedIn(grant),
            NewAccountAccess::Confirm { user, token, mail } => {
                self.send_later(
                    &mail,
                    confirmation_mail(&user, &mail, &token),
                    user.id,
                    "email confirmation",
                );
                Registered::ConfirmationSent(user)
            }
        }
    }

    /// Mails `user` a new confirmation link, cancelling the one that still worked, for a sign-in the account's
    /// unconfirmed address refused. The link is made in one transaction, and the mail is sent after it commits, in
    /// a task of its own.
    pub(crate) async fn mail_new_confirmation_link(
        &self,
        mail: &MailSettings,
        user: &User,
    ) -> Result<(), ServiceError> {
        let personal_team = self.load_personal_team(&self.db, user).await?;
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        let token = make_confirmation_link(&txn, user.id, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        self.send_later(
            mail,
            confirmation_mail(user, mail, &token),
            user.id,
            "email confirmation",
        );
        tracing::info!(user_id = %user.id, "sign-in refused: address not confirmed, new link mailed");
        Ok(())
    }

    /// Sets the address of an account that hasn't confirmed it and mails a new confirmation link there, for a
    /// mistyped address at sign-up. The caller proves who they are with the account's login and password, as for a
    /// sign-in, and needs no session.
    ///
    /// Fails with [`ServiceError::PasswordSignInOff`] when the server has no password sign-in, and with
    /// [`ServiceError::NotFound`] when it has no mail set up. An empty `login`, `password` or `email`, or an `email`
    /// that isn't an address, fails with a [`ValidationError`] before anything is counted. Then the attempt is counted
    /// and checked as a sign-in's is ([`Services::verify_login`]): past the limit it fails with
    /// [`ServiceError::TooManySignInAttempts`], and an unknown login or a wrong password with
    /// [`ServiceError::InvalidCredentials`]. With the right password it is also counted as an address change of the
    /// account ([`Services::count_address_change`]), whether the address is free or taken, and one past that limit
    /// fails with [`ServiceError::TooManyAddressChanges`]: the sign-in count is keyed by the address, so a changed
    /// address starts it over, and the account's count is what limits the mails.
    ///
    /// In one transaction the address is set (a conditional update, so an account that is confirmed fails with
    /// [`ServiceError::InvalidCredentials`] and nothing changes), the account's working password reset link is
    /// cancelled, since one mailed to a wrong mailbox would give the account to whoever reads it, a new
    /// confirmation link is made, cancelling the earlier one, and the change is recorded in the personal team with
    /// the address before and after. After it commits the link is mailed, in a task of its own.
    ///
    /// An address another account has answers as a success, with nothing changed, mailed or recorded, so it
    /// reveals nothing about accounts.
    pub async fn change_unconfirmed_address(
        &self,
        login: &str,
        password: &str,
        email: &str,
    ) -> Result<ConfirmationAddress, ServiceError> {
        self.ensure_password_sign_in()?;
        let Some(mail) = &self.mail else {
            return Err(ServiceError::NotFound);
        };
        let login = login.trim();
        if login.is_empty() {
            return Err(ValidationError::Empty { field: Field::Login }.into());
        }
        if password.is_empty() {
            return Err(ValidationError::Empty { field: Field::Password }.into());
        }
        if email.is_empty() {
            return Err(ValidationError::Empty { field: Field::Email }.into());
        }
        let new_email = Email::parse(email)?;

        let (user, _) = self.verify_login(login, password).await?;
        self.count_address_change(user.id).await?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        // The unique index decides whether another account has the address. A refusal ends the transaction on
        // Postgres, so it rolls back right away (see `apply_email_change`).
        match store::users::set_unconfirmed_email(&txn, user.id, &new_email).await {
            Ok(true) => {}
            Ok(false) => {
                txn.rollback().await.map_err(StoreError::from)?;
                return Err(ServiceError::InvalidCredentials);
            }
            Err(StoreError::Duplicate(_)) => {
                txn.rollback().await.map_err(StoreError::from)?;
                tracing::info!(user_id = %user.id, taken = true, "unconfirmed address change asked");
                return Ok(ConfirmationAddress {
                    email: new_email,
                    username: user.username,
                });
            }
            Err(error) => return Err(error.into()),
        }
        store::password_resets::cancel_working_reset_links(&txn, user.id, now).await?;
        let token = make_confirmation_link(&txn, user.id, now).await?;
        let entry = self.team_audit_entry(
            Principal {
                user_id: user.id,
                run: None,
            },
            personal_team,
            AuditAction::EmailChanged,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::Email.code(): { "before": user.email.as_str(), "after": new_email.as_str() } }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        let user = User {
            email: new_email.clone(),
            ..user
        };
        self.send_later(
            mail,
            confirmation_mail(&user, mail, &token),
            user.id,
            "email confirmation",
        );
        tracing::info!(user_id = %user.id, taken = false, "unconfirmed address changed");
        Ok(ConfirmationAddress {
            email: new_email,
            username: user.username,
        })
    }

    /// Confirms the address of the link's user and signs the caller in as them, in one transaction. Anyone holding
    /// the token may; it works with or without mail set up.
    ///
    /// An unknown token fails with [`ServiceError::NotFound`], and a used, cancelled or ended link with
    /// [`ServiceError::ConfirmationLink`]. A server with password sign-in off fails with
    /// [`ServiceError::PasswordSignInOff`], and a deactivated account with [`ServiceError::AccountDeactivated`].
    ///
    /// Confirming marks the link used (a conditional update, so of two requests racing for one link exactly one
    /// wins and the other fails like a used link) and records the confirmation in the user's personal team, with
    /// the user as its actor. The caller gets a new session. An account whose address was already confirmed, by a
    /// reset or by the operator, is signed in all the same.
    pub async fn confirm_email(&self, token: &ResetToken) -> Result<SessionGrant, ServiceError> {
        self.ensure_password_sign_in()?;
        let token_hash = sha256_hex(token.expose());
        let link = self.load_confirmation_link(&token_hash).await?;
        if let Some(error) = link.state(self.now()).email_change_error() {
            return Err(ServiceError::ConfirmationLink(error));
        }
        let user = store::users::find_user(&self.db, link.user_id)
            .await?
            .ok_or_else(|| corrupt_link(link.user_id, "link has no user"))?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::email_confirmations::use_confirmation_link(&txn, &token_hash, now).await? {
            // Used, cancelled or ended since it was read. Roll back before reading again, so the answer says which.
            txn.rollback().await.map_err(StoreError::from)?;
            let link = self.load_confirmation_link(&token_hash).await?;
            return Err(ServiceError::ConfirmationLink(
                link.state(now)
                    .email_change_error()
                    .unwrap_or(EmailChangeLinkError::Used),
            ));
        }
        // Already confirmed, by a reset or the operator, since the link was made: that wrote its own entry.
        if store::users::confirm_email(&txn, user.id).await? {
            let entry = self.team_audit_entry(
                Principal {
                    user_id: user.id,
                    run: None,
                },
                personal_team,
                AuditAction::EmailConfirmed,
                EntityType::User,
                *user.id.as_uuid(),
                json!({ AuditField::MadeBy.code(): "link" }),
                now,
            );
            store::audit::insert_audit_entry(&txn, &entry).await?;
        }
        let session = self.open_session(&txn, user.clone(), now, None).await?;
        txn.commit().await.map_err(StoreError::from)?;

        // Refused sign-ins and their "Send it again" presses were counted, and the address is confirmed now, so
        // failing to clear the count mustn't fail the request.
        if let Err(error) =
            store::sign_in_attempts::clear_sign_in_attempts(&self.db, &sha256_hex(user.email.as_str())).await
        {
            tracing::warn!(?error, "can't clear sign-in attempts");
        }

        tracing::info!(user_id = %user.id, "address confirmed with a link");
        Ok(session)
    }

    /// Confirms the address of the account `login` names: an email address when it has an `@`, a username
    /// otherwise. The operator runs it on the server, which is the permission, and it needs no mail. The entry is in
    /// the user's personal team with the user as its actor, since an entry needs an account as its actor and the
    /// operator has none; its diff says who confirmed. An address already confirmed writes nothing.
    ///
    /// Fails with [`ServiceError::NotFound`] when no account has that name.
    pub async fn confirm_email_by_operator(&self, login: &str) -> Result<OperatorConfirmed, ServiceError> {
        let login = Login::parse(login.trim()).ok_or(ServiceError::NotFound)?;
        let user = self.find_user_by_login(&login).await?.ok_or(ServiceError::NotFound)?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::users::confirm_email(&txn, user.id).await? {
            txn.rollback().await.map_err(StoreError::from)?;
            return Ok(OperatorConfirmed::AlreadyConfirmed(user));
        }
        let entry = self.team_audit_entry(
            Principal {
                user_id: user.id,
                run: None,
            },
            personal_team,
            AuditAction::EmailConfirmed,
            EntityType::User,
            *user.id.as_uuid(),
            json!({ AuditField::MadeBy.code(): "operator" }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, "address confirmed by the operator");
        Ok(OperatorConfirmed::Confirmed(user))
    }

    async fn load_confirmation_link(&self, token_hash: &str) -> Result<ResetLink, ServiceError> {
        store::email_confirmations::find_confirmation_link(&self.db, token_hash)
            .await?
            .ok_or(ServiceError::NotFound)
    }
}

/// Cancels the user's working confirmation link and makes a new one, in the caller's transaction. Returns the token,
/// the only copy.
async fn make_confirmation_link(
    txn: &impl store::ConnectionTrait,
    user_id: UserId,
    now: Timestamp,
) -> Result<ResetToken, ServiceError> {
    let expires_at = confirmation_link_end_from(now).ok_or(ValidationError::TimestampOutOfRange)?;
    let token = ResetToken::new(random_token());
    let link = ResetLink {
        user_id,
        created_at: now,
        expires_at,
        used_at: None,
        cancelled_at: None,
    };
    store::email_confirmations::cancel_working_confirmation_links(txn, user_id, now).await?;
    store::email_confirmations::insert_confirmation_link(txn, &sha256_hex(token.expose()), &link).await?;
    Ok(token)
}

fn corrupt_link(user_id: UserId, problem: &'static str) -> StoreError {
    StoreError::corrupt("email_confirmation_links", &user_id.to_string(), problem)
}

/// The mail with the confirmation link in it, to the account's address.
fn confirmation_mail(user: &User, mail: &MailSettings, token: &ResetToken) -> Mail {
    Mail {
        to: user.email.as_str().to_owned(),
        subject: "Confirm your Upheap address".to_owned(),
        body: format!(
            "Someone made the account @{username} on {host} with this address.\n\nOpen this link to confirm it and sign in. It works once, for a day:\n{base_url}/confirm/{token}\n\nIf it wasn't you, ignore this mail. The account can't be used without it.\n",
            username = user.username,
            host = mail.host(),
            base_url = mail.base_url,
            token = token.expose(),
        ),
    }
}
