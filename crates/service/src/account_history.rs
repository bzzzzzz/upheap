//! A user's account history: the password changes, reset links and resets recorded about them (architecture.md,
//! Authentication and authorization).
//!
//! The entries are the audit log's, in the user's personal team where their account entries go. The history is
//! theirs alone, so there is no way to name another user or team.

use domain::Timestamp;
use domain::audit::AuditAction;
use domain::due::describe_time;
use jiff::tz::TimeZone;
use serde_json::Value;
use store::audit::AuditScope;
use uuid::Uuid;

use crate::audit_log::{AuditCursor, AuditRequest};
use crate::{Principal, ServiceError, Services};

/// The audit actions an account history holds.
const ACCOUNT_ACTIONS: [AuditAction; 3] = [
    AuditAction::PasswordChanged,
    AuditAction::PasswordResetLinkMade,
    AuditAction::PasswordReset,
];

/// What happened to the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountEventKind {
    PasswordChanged,
    PasswordResetLinkMade,
    PasswordReset,
}

impl AccountEventKind {
    /// The kind an audit action is, or `None` for an action the account history doesn't hold.
    fn of(action: AuditAction) -> Option<Self> {
        match action {
            AuditAction::PasswordChanged => Some(Self::PasswordChanged),
            AuditAction::PasswordResetLinkMade => Some(Self::PasswordResetLinkMade),
            AuditAction::PasswordReset => Some(Self::PasswordReset),
            _ => None,
        }
    }
}

/// A page of a user's account history, newest first.
#[derive(Debug)]
pub struct AccountHistoryPage {
    pub entries: Vec<AccountEvent>,
    /// Where the next page starts; `None` on the last page.
    pub next: Option<AuditCursor>,
}

/// One thing that happened to the account, worded for the user.
#[derive(Debug)]
pub struct AccountEvent {
    pub id: Uuid,
    pub kind: AccountEventKind,
    /// One sentence: "Password changed. 2 other sign-ins ended."
    pub description: String,
    pub created_at: Timestamp,
    /// When it happened, in the caller's timezone.
    pub created_label: String,
}

impl Services {
    /// The caller's own password changes, reset links made for them and passwords set with one, newest first.
    /// `limit` is clamped to 1..=[`crate::audit_log::MAX_AUDIT_PAGE`].
    pub async fn list_account_history(
        &self,
        principal: Principal,
        after: Option<AuditCursor>,
        limit: u64,
        tz: &TimeZone,
    ) -> Result<AccountHistoryPage, ServiceError> {
        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let personal_team = self.load_personal_team(&user).await?;
        let request = AuditRequest {
            entity_id: Some(*principal.user_id.as_uuid()),
            after,
            limit,
        };
        let (entries, next) = self
            .read_page(AuditScope::Team(personal_team), &ACCOUNT_ACTIONS, request)
            .await?;
        let entries = entries
            .into_iter()
            .filter_map(|entry| {
                let kind = AccountEventKind::of(entry.action)?;
                Some(AccountEvent {
                    id: entry.id,
                    kind,
                    description: describe_account_event(kind, &entry.diff),
                    created_at: entry.created_at,
                    created_label: describe_time(entry.created_at, tz),
                })
            })
            .collect();
        Ok(AccountHistoryPage { entries, next })
    }
}

/// The sentence for an account entry. The count of ended sign-ins comes from the diff; without a readable one the
/// sentence leaves it out.
fn describe_account_event(kind: AccountEventKind, diff: &Value) -> String {
    let ended = diff.get("ended_sign_ins").and_then(Value::as_u64);
    match (kind, ended) {
        (AccountEventKind::PasswordChanged, Some(0)) => "Password changed. No other sign-ins were open.".to_owned(),
        (AccountEventKind::PasswordChanged, Some(1)) => "Password changed. 1 other sign-in ended.".to_owned(),
        (AccountEventKind::PasswordChanged, Some(n)) => format!("Password changed. {n} other sign-ins ended."),
        (AccountEventKind::PasswordChanged, None) => "Password changed.".to_owned(),
        (AccountEventKind::PasswordReset, Some(0)) => {
            "Password set with a reset link. No sign-ins were open.".to_owned()
        }
        (AccountEventKind::PasswordReset, Some(1)) => "Password set with a reset link. 1 sign-in ended.".to_owned(),
        (AccountEventKind::PasswordReset, Some(n)) => format!("Password set with a reset link. {n} sign-ins ended."),
        (AccountEventKind::PasswordReset, None) => "Password set with a reset link.".to_owned(),
        (AccountEventKind::PasswordResetLinkMade, _) => "Reset link made by whoever runs the server.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn describe(kind: AccountEventKind, diff: Value) -> String {
        describe_account_event(kind, &diff)
    }

    #[test]
    fn password_changed_counts_other_sign_ins() {
        let changed = |n| describe(AccountEventKind::PasswordChanged, json!({ "ended_sign_ins": n }));
        assert_eq!(changed(0), "Password changed. No other sign-ins were open.");
        assert_eq!(changed(1), "Password changed. 1 other sign-in ended.");
        assert_eq!(changed(2), "Password changed. 2 other sign-ins ended.");
    }

    #[test]
    fn password_reset_counts_all_sign_ins() {
        let reset = |n| describe(AccountEventKind::PasswordReset, json!({ "ended_sign_ins": n }));
        assert_eq!(reset(0), "Password set with a reset link. No sign-ins were open.");
        assert_eq!(reset(1), "Password set with a reset link. 1 sign-in ended.");
        assert_eq!(reset(3), "Password set with a reset link. 3 sign-ins ended.");
    }

    #[test]
    fn an_unreadable_count_is_left_out() {
        for diff in [
            json!({}),
            json!({ "ended_sign_ins": "many" }),
            json!({ "ended_sign_ins": -1 }),
        ] {
            assert_eq!(
                describe(AccountEventKind::PasswordChanged, diff.clone()),
                "Password changed."
            );
            assert_eq!(
                describe(AccountEventKind::PasswordReset, diff),
                "Password set with a reset link."
            );
        }
    }

    #[test]
    fn a_reset_link_made_says_who_made_it() {
        assert_eq!(
            describe(
                AccountEventKind::PasswordResetLinkMade,
                json!({ "made_by": "operator" })
            ),
            "Reset link made by whoever runs the server."
        );
    }
}
