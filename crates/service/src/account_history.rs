//! A user's account history: the password changes, reset links and resets recorded about them, the address changes
//! asked and made, the sign-in provider's turn-offs and turn-ons, and their own heap priority changes (architecture.md, Authentication and
//! authorization).
//!
//! The entries are the audit log's, in the user's personal team where their account entries go. The history is
//! theirs alone, so there is no way to name another user or team.

use std::collections::{HashMap, HashSet};

use domain::audit::{AuditAction, AuditField};
use domain::due::describe_time;
use domain::{HeapId, TeamId, Timestamp};
use jiff::tz::TimeZone;
use serde_json::Value;
use store::audit::{AuditEntry, AuditScope};
use uuid::Uuid;

use crate::audit_log::{AuditCursor, AuditRequest};
use crate::{Principal, ServiceError, Services};

/// The audit actions an account history holds.
const ACCOUNT_ACTIONS: [AuditAction; AccountEventKind::ALL.len()] = {
    let mut actions = [AuditAction::PasswordChanged; AccountEventKind::ALL.len()];
    let mut i = 0;
    while i < actions.len() {
        actions[i] = AccountEventKind::ALL[i].action();
        i += 1;
    }
    actions
};

/// What happened to the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountEventKind {
    PasswordChanged,
    PasswordResetLinkMade,
    PasswordReset,
    AccountDeactivated,
    AccountReactivated,
    HeapPriorityChanged,
    EmailChangeAsked,
    EmailChanged,
}

impl AccountEventKind {
    /// Every kind. The actions the history reads are derived from it, so a new kind is read as soon as it's listed.
    pub const ALL: [Self; 8] = [
        Self::PasswordChanged,
        Self::PasswordResetLinkMade,
        Self::PasswordReset,
        Self::AccountDeactivated,
        Self::AccountReactivated,
        Self::HeapPriorityChanged,
        Self::EmailChangeAsked,
        Self::EmailChanged,
    ];

    /// The audit action a kind is recorded as.
    pub const fn action(self) -> AuditAction {
        match self {
            Self::PasswordChanged => AuditAction::PasswordChanged,
            Self::PasswordResetLinkMade => AuditAction::PasswordResetLinkMade,
            Self::PasswordReset => AuditAction::PasswordReset,
            Self::AccountDeactivated => AuditAction::AccountDeactivated,
            Self::AccountReactivated => AuditAction::AccountReactivated,
            Self::HeapPriorityChanged => AuditAction::HeapPriorityChanged,
            Self::EmailChangeAsked => AuditAction::EmailChangeAsked,
            Self::EmailChanged => AuditAction::EmailChanged,
        }
    }

    /// The kind an audit action is, or `None` for an action the account history doesn't hold.
    fn of(action: AuditAction) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.action() == action)
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
    /// The caller's own password changes, reset links made for them, passwords set with one, address changes asked
    /// and made, the sign-in provider's turn-offs and turn-ons, and heap priority changes, newest first.
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
        let personal_team = self.load_personal_team(&self.db, &user).await?;
        let request = AuditRequest {
            entity_id: Some(*principal.user_id.as_uuid()),
            after,
            limit,
        };
        let (entries, next) = self
            .read_page(AuditScope::Team(personal_team), &ACCOUNT_ACTIONS, request)
            .await?;
        let names = self.heap_names_for(principal, &entries).await?;
        let entries = entries
            .into_iter()
            .filter_map(|entry| {
                let kind = AccountEventKind::of(entry.action)?;
                Some(AccountEvent {
                    id: entry.id,
                    kind,
                    description: describe_account_event(kind, &entry.diff, heap_name(&entry.diff, &names)),
                    created_at: entry.created_at,
                    created_label: describe_time(entry.created_at, tz),
                })
            })
            .collect();
        Ok(AccountHistoryPage { entries, next })
    }

    /// The current names of the heaps that priority entries without a recorded name point to, by heap ID. Only heaps
    /// of teams the caller is still in: a heap they left may have been renamed since, and that name isn't theirs to see.
    async fn heap_names_for(
        &self,
        principal: Principal,
        entries: &[AuditEntry],
    ) -> Result<HashMap<HeapId, String>, ServiceError> {
        let ids: Vec<HeapId> = entries
            .iter()
            .filter(|entry| entry.action == AuditAction::HeapPriorityChanged)
            .filter(|entry| recorded_name(&entry.diff).is_none())
            .filter_map(|entry| diff_heap_id(&entry.diff))
            .collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let teams: HashSet<TeamId> = store::teams::list_teams_for_user(&self.db, principal.user_id)
            .await?
            .into_iter()
            .map(|(team, _)| team.id)
            .collect();
        let heaps = store::heaps::find_heaps(&self.db, &ids).await?;
        Ok(heaps
            .into_iter()
            .filter(|heap| teams.contains(&heap.team_id))
            .map(|heap| (heap.id, heap.name.as_str().to_owned()))
            .collect())
    }
}

fn recorded_name(diff: &Value) -> Option<&str> {
    diff.get(AuditField::Name.code()).and_then(Value::as_str)
}

fn diff_heap_id(diff: &Value) -> Option<HeapId> {
    let id = diff.get(AuditField::HeapId.code())?.as_str()?;
    Some(HeapId::from_uuid(id.parse().ok()?))
}

/// The heap's name for a priority entry: the one recorded with it, else the heap's current name.
fn heap_name<'a>(diff: &'a Value, current: &'a HashMap<HeapId, String>) -> Option<&'a str> {
    recorded_name(diff).or_else(|| current.get(&diff_heap_id(diff)?).map(String::as_str))
}

/// The sentence for an account entry. The count of ended sign-ins comes from the diff; without a readable one the
/// sentence leaves it out. A priority change names the heap by `heap_name`, or "a heap" without one.
fn describe_account_event(kind: AccountEventKind, diff: &Value, heap_name: Option<&str>) -> String {
    let ended = diff.get(AuditField::EndedSignIns.code()).and_then(Value::as_u64);
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
        (AccountEventKind::AccountDeactivated, Some(0)) => {
            "Turned off by your sign-in provider. No sign-ins were open.".to_owned()
        }
        (AccountEventKind::AccountDeactivated, Some(1)) => {
            "Turned off by your sign-in provider. 1 sign-in ended.".to_owned()
        }
        (AccountEventKind::AccountDeactivated, Some(n)) => {
            format!("Turned off by your sign-in provider. {n} sign-ins ended.")
        }
        (AccountEventKind::AccountDeactivated, None) => "Turned off by your sign-in provider.".to_owned(),
        (AccountEventKind::AccountReactivated, _) => "Turned back on by your sign-in provider.".to_owned(),
        (AccountEventKind::HeapPriorityChanged, _) => {
            let heap = heap_name.unwrap_or("a heap");
            let priority = diff.get(AuditField::Priority.code());
            let number = |side: &str| priority.and_then(|p| p.get(side)).and_then(Value::as_i64);
            match (number("before"), number("after")) {
                (Some(before), Some(after)) => {
                    format!("You changed your priority for {heap} from {before} to {after}.")
                }
                _ => format!("You changed your priority for {heap}."),
            }
        }
        (AccountEventKind::EmailChangeAsked, _) => {
            let email = diff.get(AuditField::Email.code()).and_then(Value::as_str);
            match email {
                Some(email) => format!("Address change to {email} asked. A link was mailed there."),
                None => "Address change asked. A link was mailed to the new address.".to_owned(),
            }
        }
        (AccountEventKind::EmailChanged, _) => {
            let email = diff.get(AuditField::Email.code());
            let side = |side: &str| email.and_then(|email| email.get(side)).and_then(Value::as_str);
            match (side("before"), side("after")) {
                (Some(before), Some(after)) => format!("Address changed from {before} to {after}."),
                _ => "Address changed.".to_owned(),
            }
        }
        (AccountEventKind::PasswordResetLinkMade, _) => {
            match diff.get(AuditField::MadeBy.code()).and_then(Value::as_str) {
                Some("mail") => "Reset link sent by mail, as asked on the sign-in page.".to_owned(),
                _ => "Reset link made by whoever runs the server.".to_owned(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn describe(kind: AccountEventKind, diff: Value) -> String {
        describe_account_event(kind, &diff, None)
    }

    fn describe_heap(diff: Value, name: Option<&str>) -> String {
        describe_account_event(AccountEventKind::HeapPriorityChanged, &diff, name)
    }

    #[test]
    fn a_priority_change_names_the_heap_and_the_numbers() {
        let diff = json!({ "priority": { "before": 1, "after": 3 } });
        assert_eq!(
            describe_heap(diff, Some("Work")),
            "You changed your priority for Work from 1 to 3."
        );
    }

    #[test]
    fn a_priority_change_without_a_name_says_a_heap() {
        let diff = json!({ "priority": { "before": 1, "after": 3 } });
        assert_eq!(
            describe_heap(diff, None),
            "You changed your priority for a heap from 1 to 3."
        );
    }

    #[test]
    fn unreadable_priority_numbers_are_left_out() {
        for diff in [
            json!({}),
            json!({ "priority": { "before": 1 } }),
            json!({ "priority": { "before": "low", "after": 3 } }),
        ] {
            assert_eq!(describe_heap(diff, Some("Work")), "You changed your priority for Work.");
        }
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
    fn account_deactivated_counts_ended_sign_ins() {
        let off = |n| describe(AccountEventKind::AccountDeactivated, json!({ "ended_sign_ins": n }));
        assert_eq!(off(0), "Turned off by your sign-in provider. No sign-ins were open.");
        assert_eq!(off(1), "Turned off by your sign-in provider. 1 sign-in ended.");
        assert_eq!(off(3), "Turned off by your sign-in provider. 3 sign-ins ended.");
        assert_eq!(
            describe(AccountEventKind::AccountDeactivated, json!({})),
            "Turned off by your sign-in provider."
        );
    }

    #[test]
    fn account_reactivated_says_the_provider_turned_it_back_on() {
        assert_eq!(
            describe(AccountEventKind::AccountReactivated, json!({})),
            "Turned back on by your sign-in provider."
        );
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

    #[test]
    fn an_address_change_asked_names_the_new_address() {
        assert_eq!(
            describe(
                AccountEventKind::EmailChangeAsked,
                json!({ "email": "new@example.com" })
            ),
            "Address change to new@example.com asked. A link was mailed there."
        );
        assert_eq!(
            describe(AccountEventKind::EmailChangeAsked, json!({})),
            "Address change asked. A link was mailed to the new address."
        );
    }

    #[test]
    fn an_address_changed_names_both_addresses() {
        assert_eq!(
            describe(
                AccountEventKind::EmailChanged,
                json!({ "email": { "before": "old@example.com", "after": "new@example.com" } })
            ),
            "Address changed from old@example.com to new@example.com."
        );
        assert_eq!(describe(AccountEventKind::EmailChanged, json!({})), "Address changed.");
    }

    #[test]
    fn a_reset_link_sent_by_mail_says_so() {
        assert_eq!(
            describe(AccountEventKind::PasswordResetLinkMade, json!({ "made_by": "mail" })),
            "Reset link sent by mail, as asked on the sign-in page."
        );
        assert_eq!(
            describe(AccountEventKind::PasswordResetLinkMade, json!({})),
            "Reset link made by whoever runs the server."
        );
    }
}
