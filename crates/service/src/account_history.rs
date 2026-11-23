//! A user's account history: the password changes, reset links and resets recorded about them, the address changes
//! asked and made, the address confirmations, the sign-in provider's turn-offs and turn-ons, an operator's
//! turn-offs, turn-ons and role changes, and their own heap priority changes (architecture.md, Authentication and
//! authorization).
//!
//! The entries are the audit log's, in the user's personal team where their account entries go. A heap priority
//! change is recorded there and not in the heap's team, since the value is private to the user. Each entry's sentence
//! is made from its diff on reading, so a wording fix reaches old entries. A user reads their own history; an operator
//! reads anyone's, worded for the operator.

use std::collections::{HashMap, HashSet};

use domain::audit::{AuditAction, AuditField};
use domain::due::describe_time;
use domain::user::User;
use domain::{HeapId, TeamId, Timestamp, UserId};
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
    EmailConfirmed,
    OperatorGranted,
    OperatorRemoved,
}

impl AccountEventKind {
    /// Every kind. The actions the history reads are derived from it, so a new kind is read as soon as it's listed.
    pub const ALL: [Self; 11] = [
        Self::PasswordChanged,
        Self::PasswordResetLinkMade,
        Self::PasswordReset,
        Self::AccountDeactivated,
        Self::AccountReactivated,
        Self::HeapPriorityChanged,
        Self::EmailChangeAsked,
        Self::EmailChanged,
        Self::EmailConfirmed,
        Self::OperatorGranted,
        Self::OperatorRemoved,
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
            Self::EmailConfirmed => AuditAction::EmailConfirmed,
            Self::OperatorGranted => AuditAction::OperatorGranted,
            Self::OperatorRemoved => AuditAction::OperatorRemoved,
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

/// Who the sentence is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Voice<'a> {
    /// The person the entries are about.
    Own,
    /// An operator reading the person's history. `by` names the account that did an operator-made entry, and is
    /// `None` for one made from the server's shell.
    Operator { by: Option<&'a str> },
}

impl Services {
    /// The caller's own password changes, reset links made for them, passwords set with one, address changes asked
    /// and made, the sign-in provider's and an operator's turn-offs and turn-ons, and heap priority changes, newest
    /// first. `limit` is clamped to 1..=[`crate::audit_log::MAX_AUDIT_PAGE`].
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
        self.read_account_history(principal, &user, false, after, limit, tz)
            .await
    }

    /// The same entries about the account `user_id`, worded for an operator: who did what is named. Fails with
    /// [`ServiceError::NotFound`] for a caller who isn't an operator, before anything else is read, and for an
    /// account that doesn't exist.
    pub async fn list_operator_account_history(
        &self,
        principal: Principal,
        user_id: UserId,
        after: Option<AuditCursor>,
        limit: u64,
        tz: &TimeZone,
    ) -> Result<AccountHistoryPage, ServiceError> {
        self.require_operator(principal).await?;
        let user = store::users::find_user(&self.db, user_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.read_account_history(principal, &user, true, after, limit, tz)
            .await
    }

    /// A page of `user`'s account history. `heap_names_for` looks heap names up as `principal`, who may be an
    /// operator reading someone else's.
    async fn read_account_history(
        &self,
        principal: Principal,
        user: &User,
        operator_view: bool,
        after: Option<AuditCursor>,
        limit: u64,
        tz: &TimeZone,
    ) -> Result<AccountHistoryPage, ServiceError> {
        let personal_team = self.load_personal_team(&self.db, user).await?;
        let request = AuditRequest {
            entity_id: Some(*user.id.as_uuid()),
            after,
            limit,
        };
        let (entries, next) = self
            .read_page(AuditScope::Team(personal_team), &ACCOUNT_ACTIONS, request)
            .await?;
        let names = self.heap_names_for(principal, &entries).await?;
        let actors = if operator_view {
            self.actor_names(user.id, &entries).await?
        } else {
            HashMap::new()
        };
        let entries = entries
            .into_iter()
            .filter_map(|entry| {
                let kind = AccountEventKind::of(entry.action)?;
                let voice = if operator_view {
                    Voice::Operator {
                        by: actors.get(&entry.actor).map(String::as_str),
                    }
                } else {
                    Voice::Own
                };
                Some(AccountEvent {
                    id: entry.id,
                    kind,
                    description: describe_account_event(kind, &entry.diff, heap_name(&entry.diff, &names), voice),
                    created_at: entry.created_at,
                    created_label: describe_time(entry.created_at, tz),
                })
            })
            .collect();
        Ok(AccountHistoryPage { entries, next })
    }

    /// The display names of the accounts that did entries about `person`, other than `person`: an entry the person is
    /// the actor of was made by the server, or by the person.
    async fn actor_names(
        &self,
        person: UserId,
        entries: &[AuditEntry],
    ) -> Result<HashMap<UserId, String>, ServiceError> {
        let ids: Vec<UserId> = entries
            .iter()
            .map(|entry| entry.actor)
            .filter(|actor| *actor != person)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Ok(store::users::list_users(&self.db, &ids)
            .await?
            .into_iter()
            .map(|user| (user.id, user.display_name.as_str().to_owned()))
            .collect())
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
fn describe_account_event(kind: AccountEventKind, diff: &Value, heap_name: Option<&str>, voice: Voice<'_>) -> String {
    let ended = diff.get(AuditField::EndedSignIns.code()).and_then(Value::as_u64);
    let made_by = diff.get(AuditField::MadeBy.code()).and_then(Value::as_str);
    let by_operator = made_by == Some("operator");
    let origin = match voice {
        Voice::Operator { by: Some(name) } => format!("by {name}"),
        Voice::Operator { by: None } | Voice::Own => "from the server".to_owned(),
    };
    match (kind, ended) {
        (AccountEventKind::OperatorGranted, _) => "Made an operator from the server.".to_owned(),
        (AccountEventKind::OperatorRemoved, _) => "Operator role removed from the server.".to_owned(),
        (AccountEventKind::AccountDeactivated, _) if by_operator => {
            let who = match voice {
                Voice::Own => "Turned off by whoever runs the server.".to_owned(),
                Voice::Operator { .. } => format!("Deactivated {origin}."),
            };
            format!("{who}{}", ended_sign_ins(ended))
        }
        (AccountEventKind::AccountReactivated, _) if by_operator => match voice {
            Voice::Own => "Turned back on by whoever runs the server.".to_owned(),
            Voice::Operator { .. } => format!("Reactivated {origin}."),
        },
        (AccountEventKind::PasswordResetLinkMade, _) if made_by != Some("mail") && voice != Voice::Own => {
            format!("Reset link made {origin}.")
        }
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
        (AccountEventKind::EmailConfirmed, _) => match diff.get(AuditField::MadeBy.code()).and_then(Value::as_str) {
            Some("link") => "Address confirmed with the mailed link.".to_owned(),
            _ => "Address confirmed by whoever runs the server.".to_owned(),
        },
        (AccountEventKind::PasswordResetLinkMade, _) => {
            match diff.get(AuditField::MadeBy.code()).and_then(Value::as_str) {
                Some("mail") => "Reset link sent by mail, as asked on the sign-in page.".to_owned(),
                _ => "Reset link made by whoever runs the server.".to_owned(),
            }
        }
    }
}

/// The end of a sentence that counts the sign-ins an action ended: " 3 sign-ins ended.", and nothing without a
/// readable count.
fn ended_sign_ins(ended: Option<u64>) -> String {
    match ended {
        Some(0) => " No sign-ins were open.".to_owned(),
        Some(1) => " 1 sign-in ended.".to_owned(),
        Some(n) => format!(" {n} sign-ins ended."),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn describe(kind: AccountEventKind, diff: Value) -> String {
        describe_account_event(kind, &diff, None, Voice::Own)
    }

    fn describe_for_operator(kind: AccountEventKind, diff: Value, by: Option<&str>) -> String {
        describe_account_event(kind, &diff, None, Voice::Operator { by })
    }

    fn describe_heap(diff: Value, name: Option<&str>) -> String {
        describe_account_event(AccountEventKind::HeapPriorityChanged, &diff, name, Voice::Own)
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
    fn a_confirmation_says_who_made_it() {
        assert_eq!(
            describe(AccountEventKind::EmailConfirmed, json!({ "made_by": "link" })),
            "Address confirmed with the mailed link."
        );
        assert_eq!(
            describe(AccountEventKind::EmailConfirmed, json!({ "made_by": "operator" })),
            "Address confirmed by whoever runs the server."
        );
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

    #[test]
    fn an_operators_reset_link_is_named_in_both_views() {
        let diff = || json!({ "made_by": "operator" });
        let kind = AccountEventKind::PasswordResetLinkMade;
        assert_eq!(describe(kind, diff()), "Reset link made by whoever runs the server.");
        assert_eq!(
            describe_for_operator(kind, diff(), Some("Ana Lee")),
            "Reset link made by Ana Lee."
        );
        assert_eq!(
            describe_for_operator(kind, diff(), None),
            "Reset link made from the server."
        );
        assert_eq!(
            describe_for_operator(kind, json!({ "made_by": "mail" }), None),
            "Reset link sent by mail, as asked on the sign-in page."
        );
    }

    #[test]
    fn an_operators_turn_off_counts_the_sign_ins_in_both_views() {
        let off = |n| json!({ "made_by": "operator", "ended_sign_ins": n });
        let kind = AccountEventKind::AccountDeactivated;
        assert_eq!(
            describe(kind, off(3)),
            "Turned off by whoever runs the server. 3 sign-ins ended."
        );
        assert_eq!(
            describe(kind, off(1)),
            "Turned off by whoever runs the server. 1 sign-in ended."
        );
        assert_eq!(
            describe(kind, off(0)),
            "Turned off by whoever runs the server. No sign-ins were open."
        );
        assert_eq!(
            describe_for_operator(kind, off(3), Some("Ana Lee")),
            "Deactivated by Ana Lee. 3 sign-ins ended."
        );
        assert_eq!(
            describe_for_operator(kind, off(1), None),
            "Deactivated from the server. 1 sign-in ended."
        );
        assert_eq!(
            describe_for_operator(kind, off(0), None),
            "Deactivated from the server. No sign-ins were open."
        );
    }

    #[test]
    fn an_operators_turn_on_is_named_in_both_views() {
        let diff = || json!({ "made_by": "operator" });
        let kind = AccountEventKind::AccountReactivated;
        assert_eq!(describe(kind, diff()), "Turned back on by whoever runs the server.");
        assert_eq!(
            describe_for_operator(kind, diff(), Some("Ana Lee")),
            "Reactivated by Ana Lee."
        );
        assert_eq!(
            describe_for_operator(kind, diff(), None),
            "Reactivated from the server."
        );
    }

    #[test]
    fn the_role_changes_read_the_same_in_both_views() {
        for voice in [Voice::Own, Voice::Operator { by: None }] {
            let say = |kind| describe_account_event(kind, &json!({}), None, voice);
            assert_eq!(
                say(AccountEventKind::OperatorGranted),
                "Made an operator from the server."
            );
            assert_eq!(
                say(AccountEventKind::OperatorRemoved),
                "Operator role removed from the server."
            );
        }
    }

    #[test]
    fn the_providers_entries_keep_their_words_in_the_operators_view() {
        assert_eq!(
            describe_for_operator(
                AccountEventKind::AccountDeactivated,
                json!({ "ended_sign_ins": 2 }),
                None
            ),
            "Turned off by your sign-in provider. 2 sign-ins ended."
        );
        assert_eq!(
            describe_for_operator(AccountEventKind::AccountReactivated, json!({}), None),
            "Turned back on by your sign-in provider."
        );
    }
}
