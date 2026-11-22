//! The operator role and what an operator does about accounts: list them, turn a local one off and on, make a
//! reset link for one and read an account's history (architecture.md, Authentication and authorization).
//!
//! An account is an operator when `users.operator_since` is set, read on every call, so taking the role away takes
//! effect on the person's next request. Only the server's shell grants or removes it; nothing here takes a caller
//! for that. Whoever isn't an operator gets [`ServiceError::NotFound`] from every call, as for a thing that isn't
//! there, so the pages don't show that operators exist.

use domain::audit::{AuditAction, AuditField};
use domain::user::User;
use domain::{Timestamp, UserId};
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::auth::Login;
use crate::password_reset::ResetLinkMade;
use crate::{Principal, ServiceError, Services};

/// Who asks for an operator action.
#[derive(Clone, Copy, Debug)]
pub enum Operator {
    /// An account that must be an operator.
    Account(Principal),
    /// Whoever runs the server, in its shell: the permission is being there. The entries it records name the
    /// affected person as their actor, since an entry needs an account and the shell has none.
    Server,
}

/// How an account signs in: with an identity at the configured provider, or with a local password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    Local,
    Provider,
}

/// What an operator may do about an account in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountAction {
    /// Make a reset link: a local, active account, on a server with password sign-in.
    MakeResetLink,
    /// Turn it off: a local, active account that isn't the operator's own.
    Deactivate,
    /// Turn it back on: a local account an operator turned off, not the operator's own.
    Reactivate,
    /// Read its history: every account.
    History,
}

/// An account as an operator sees it in the list.
#[derive(Debug)]
pub struct Account {
    pub user: User,
    pub kind: AccountKind,
    pub confirmed: bool,
    /// An operator turned a local account off, or the provider turned a provider account off over SCIM.
    pub deactivated: bool,
    pub operator: bool,
    /// When a session was last opened for the account; `None` until one is.
    pub last_sign_in_at: Option<Timestamp>,
    /// What the caller may do about it, by the rules the actions themselves enforce.
    pub allowed_actions: Vec<AccountAction>,
}

/// What turning an account off would end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignInsToEnd {
    /// Live web sessions and OAuth sign-ins of clients such as the CLI.
    pub sign_ins: u32,
    pub personal_tokens: u32,
}

/// What turning an account off did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deactivation {
    /// The account is off now, and this many sign-ins ended.
    Ended(u32),
    /// It was off already; nothing changed.
    AlreadyOff,
}

/// Which operator role change to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleChange {
    Grant,
    Remove,
}

/// What a write that may find its work done did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    Changed,
    /// It was so already; nothing changed and nothing was recorded.
    Unchanged,
}

impl Services {
    /// Fails with [`ServiceError::NotFound`] unless the caller is an operator. An operator endpoint calls it before
    /// reading its request, so a non-operator learns nothing from a malformed one.
    pub async fn require_operator(&self, principal: Principal) -> Result<(), ServiceError> {
        if store::users::is_operator(&self.db, principal.user_id).await? {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    async fn authorize_operator(&self, by: Operator) -> Result<(), ServiceError> {
        match by {
            Operator::Account(principal) => self.require_operator(principal).await,
            Operator::Server => Ok(()),
        }
    }

    /// The account of an entry about `target` that `by` made.
    fn actor_of(by: Operator, target: UserId) -> Principal {
        match by {
            Operator::Account(principal) => principal,
            Operator::Server => Principal {
                user_id: target,
                run: None,
            },
        }
    }

    /// The account `login` names: an email address when it has an `@`, a username otherwise. Fails with
    /// [`ServiceError::NotFound`] when no account has that name. For the server's shell.
    pub async fn find_account_by_login(&self, login: &str) -> Result<User, ServiceError> {
        let login = Login::parse(login.trim()).ok_or(ServiceError::NotFound)?;
        self.find_user_by_login(&login).await?.ok_or(ServiceError::NotFound)
    }

    /// Every account, by display name ignoring case, then username. A server has tens to a few hundred, so the list
    /// isn't paged; the provider's pushes are read in one query.
    pub async fn list_accounts(&self, by: Operator) -> Result<Vec<Account>, ServiceError> {
        self.authorize_operator(by).await?;
        let identities = match &self.settings.oidc {
            Some(oidc) => store::identities::list_identity_subjects(&self.db, oidc.issuer()).await?,
            None => Default::default(),
        };
        let provider_inactive = if self.settings.scim.is_some() {
            store::scim::list_inactive_external_ids(&self.db).await?
        } else {
            Default::default()
        };
        let mut accounts = Vec::new();
        for record in store::users::list_accounts(&self.db).await? {
            let subject = identities.get(&record.user.id);
            let provider_off = subject.is_some_and(|subject| provider_inactive.contains(subject));
            let kind = if subject.is_some() {
                AccountKind::Provider
            } else {
                AccountKind::Local
            };
            let own = matches!(by, Operator::Account(principal) if principal.user_id == record.user.id);
            let mut allowed_actions = Vec::new();
            if kind == AccountKind::Local && !own {
                if record.turned_off {
                    allowed_actions.push(AccountAction::Reactivate);
                } else {
                    if self.settings.password_sign_in {
                        allowed_actions.push(AccountAction::MakeResetLink);
                    }
                    allowed_actions.push(AccountAction::Deactivate);
                }
            }
            allowed_actions.push(AccountAction::History);
            accounts.push(Account {
                kind,
                confirmed: !record.unconfirmed,
                deactivated: record.turned_off || provider_off,
                allowed_actions,
                operator: record.operator,
                last_sign_in_at: record.last_sign_in_at,
                user: record.user,
            });
        }
        Ok(accounts)
    }

    /// How many live sign-ins and personal tokens turning the account off would end.
    pub async fn count_sign_ins_to_end(
        &self,
        principal: Principal,
        user_id: UserId,
    ) -> Result<SignInsToEnd, ServiceError> {
        self.require_operator(principal).await?;
        store::users::find_user(&self.db, user_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let now = self.now();
        let sessions = store::sessions::count_live_sessions(&self.db, user_id, now).await?;
        let families = store::oauth::list_live_token_families(&self.db, user_id, now)
            .await?
            .len() as u64;
        let tokens = store::personal_tokens::count_live_personal_tokens(&self.db, user_id, now).await?;
        Ok(SignInsToEnd {
            sign_ins: saturate(sessions + families),
            personal_tokens: saturate(tokens),
        })
    }

    /// Turns a local account off: it can't sign in, and every sign-in it has ends, in one transaction with the entry
    /// that records it. The account, its items, teams and memberships stay. An account that is off already answers
    /// [`Deactivation::AlreadyOff`] and records nothing.
    ///
    /// Fails with [`ServiceError::AccountProviderManaged`] for a provider account, whose state the provider owns, and
    /// with [`ServiceError::OwnAccount`] for an operator's own account, which would lock them out. The shell can turn
    /// off any local account.
    pub async fn deactivate_account(&self, by: Operator, user_id: UserId) -> Result<Deactivation, ServiceError> {
        self.authorize_operator(by).await?;
        if matches!(by, Operator::Account(principal) if principal.user_id == user_id) {
            return Err(ServiceError::OwnAccount);
        }
        let user = self.find_account(user_id).await?;
        self.refuse_provider_account(user.id).await?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        // The personal team first: sign-ins open a session under its lock, as SCIM's deactivation takes it.
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::users::set_turned_off(&txn, user.id, now).await? {
            return Ok(Deactivation::AlreadyOff);
        }
        let ended_sign_ins = self.end_sign_ins(&txn, user.id, None, None, now).await?;
        let diff = json!({ AuditField::EndedSignIns.code(): ended_sign_ins, AuditField::MadeBy.code(): "operator" });
        self.record_account_change(
            &txn,
            Self::actor_of(by, user.id),
            user.id,
            personal_team,
            AuditAction::AccountDeactivated,
            diff,
            now,
        )
        .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, ended_sign_ins, "account turned off by an operator");
        Ok(Deactivation::Ended(ended_sign_ins))
    }

    /// Turns a local account back on. Nothing else is restored: the sign-ins that ended stay ended. An account that
    /// isn't off answers [`Applied::Unchanged`] and records nothing.
    ///
    /// Fails with [`ServiceError::AccountProviderManaged`] for a provider account.
    pub async fn reactivate_account(&self, by: Operator, user_id: UserId) -> Result<Applied, ServiceError> {
        self.authorize_operator(by).await?;
        let user = self.find_account(user_id).await?;
        self.refuse_provider_account(user.id).await?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        if !store::users::clear_turned_off(&txn, user.id).await? {
            return Ok(Applied::Unchanged);
        }
        self.record_account_change(
            &txn,
            Self::actor_of(by, user.id),
            user.id,
            personal_team,
            AuditAction::AccountReactivated,
            json!({ AuditField::MadeBy.code(): "operator" }),
            now,
        )
        .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, "account turned back on by an operator");
        Ok(Applied::Changed)
    }

    /// Makes a reset link for the account `user_id`, as [`Services::create_password_reset_link`] does, with the
    /// caller as the entry's actor. The link is in the answer and nowhere else.
    ///
    /// Fails with [`ServiceError::AccountProviderManaged`] for a provider account, whose state the provider owns, and
    /// with [`ServiceError::AccountTurnedOff`] for an account an operator turned off, which the link couldn't sign in.
    pub async fn create_operator_reset_link(
        &self,
        principal: Principal,
        user_id: UserId,
    ) -> Result<ResetLinkMade, ServiceError> {
        self.require_operator(principal).await?;
        self.ensure_password_sign_in()?;
        let user = self.find_account(user_id).await?;
        self.refuse_provider_account(user.id).await?;
        if store::users::is_turned_off(&self.db, user.id).await? {
            return Err(ServiceError::AccountTurnedOff);
        }
        self.make_reset_link(user, "operator", principal.user_id).await
    }

    /// Makes the account `login` names an operator. For the server's shell, the only way to. Fails with
    /// [`ServiceError::NotFound`] when no account has that name. An operator already answers [`Applied::Unchanged`].
    pub async fn grant_operator(&self, login: &str) -> Result<(User, Applied), ServiceError> {
        self.change_operator(login, RoleChange::Grant).await
    }

    /// Takes the operator role from the account `login` names. For the server's shell. Someone who isn't an
    /// operator answers [`Applied::Unchanged`].
    pub async fn remove_operator(&self, login: &str) -> Result<(User, Applied), ServiceError> {
        self.change_operator(login, RoleChange::Remove).await
    }

    async fn change_operator(&self, login: &str, change: RoleChange) -> Result<(User, Applied), ServiceError> {
        let user = self.find_account_by_login(login).await?;
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::teams::lock_team(&txn, personal_team).await?;
        let (changed, action) = match change {
            RoleChange::Grant => (
                store::users::set_operator(&txn, user.id, now).await?,
                AuditAction::OperatorGranted,
            ),
            RoleChange::Remove => (
                store::users::clear_operator(&txn, user.id).await?,
                AuditAction::OperatorRemoved,
            ),
        };
        if !changed {
            return Ok((user, Applied::Unchanged));
        }
        self.record_account_change(
            &txn,
            Self::actor_of(Operator::Server, user.id),
            user.id,
            personal_team,
            action,
            json!({}),
            now,
        )
        .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %user.id, action = action.code(), "operator role changed");
        Ok((user, Applied::Changed))
    }

    async fn find_account(&self, user_id: UserId) -> Result<User, ServiceError> {
        store::users::find_user(&self.db, user_id)
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// Fails with [`ServiceError::AccountProviderManaged`] for an account with an identity at the configured
    /// provider.
    async fn refuse_provider_account(&self, user_id: UserId) -> Result<(), ServiceError> {
        let Some(oidc) = &self.settings.oidc else {
            return Ok(());
        };
        if store::identities::find_identity_subject(&self.db, oidc.issuer(), user_id)
            .await?
            .is_some()
        {
            return Err(ServiceError::AccountProviderManaged);
        }
        Ok(())
    }
}

fn saturate(count: u64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}
