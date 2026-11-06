//! The signed-in user, what they can reach, and finding other users to add to a team.

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::permission::Permission;
use domain::team::{Role, Team};
use domain::user::{DisplayName, User, UserQuery};
use domain::{TeamId, Version};
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::{Principal, ServiceError, Services};

/// The most users a lookup returns. A lookup is for picking one person, not for browsing.
pub const USER_LOOKUP_LIMIT: u64 = 10;

/// The caller, their teams and each team's heaps.
#[derive(Debug)]
pub struct MeView {
    pub user: User,
    pub teams: Vec<TeamView>,
}

#[derive(Debug)]
pub struct TeamView {
    pub team: Team,
    pub role: Role,
    pub heaps: Vec<Heap>,
}

impl Services {
    pub async fn load_me(&self, principal: Principal) -> Result<MeView, ServiceError> {
        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let team_ids: Vec<_> = teams.iter().map(|(team, _)| team.id).collect();
        let mut heaps = store::heaps::list_heaps_for_teams(&self.db, &team_ids).await?;

        let teams = teams
            .into_iter()
            .map(|(team, role)| {
                let (own, rest): (Vec<Heap>, Vec<Heap>) = heaps.drain(..).partition(|heap| heap.team_id == team.id);
                heaps = rest;
                TeamView { team, role, heaps: own }
            })
            .collect();
        Ok(MeView { user, teams })
    }
}

impl Services {
    /// Changes the caller's display name if their user is still at version `expected`. The same name succeeds
    /// without an audit entry.
    ///
    /// The audit log is kept per team, so the change is recorded once in each team the user belongs to, where
    /// teammates see the name. A user in no team has nowhere to record it, and the change goes unrecorded.
    pub async fn update_display_name(
        &self,
        principal: Principal,
        display_name: DisplayName,
        expected: Version,
    ) -> Result<User, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::users::find_user(&txn, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        if current.display_name == display_name {
            return Ok(current);
        }

        // Team rows first, in ID order, then the user row: the order membership changes take them in (team, then the
        // user rows their inserts reference), so the two can't deadlock on MySQL.
        let mut team_ids: Vec<TeamId> = store::teams::list_teams_for_user(&txn, principal.user_id)
            .await?
            .into_iter()
            .map(|(team, _)| team.id)
            .collect();
        team_ids.sort();
        for team_id in &team_ids {
            store::teams::lock_team(&txn, *team_id).await?;
        }

        let now = self.now();
        let diff = json!({
            "display_name": { "before": current.display_name.as_str(), "after": display_name.as_str() }
        });
        for team_id in team_ids {
            let entry = self.team_audit_entry(
                principal,
                team_id,
                AuditAction::UserUpdated,
                EntityType::User,
                *principal.user_id.as_uuid(),
                diff.clone(),
                now,
            );
            store::audit::insert_audit_entry(&txn, &entry).await?;
        }
        let renamed = User {
            display_name,
            ..current.clone()
        };
        let saved =
            store::users::update_user(&txn, &renamed, expected)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %principal.user_id, "display name changed");
        Ok(saved)
    }

    /// Users for an admin picking someone to add to a team: those with a word of their display name starting with
    /// `query`, and, when `query` has an email's local part, `@` and part of its domain, those whose email starts
    /// with it. Up to [`USER_LOOKUP_LIMIT`], by display name.
    ///
    /// A match shows the email only when the caller typed that much of it, so a lookup by name can't be used to
    /// collect addresses.
    ///
    /// Only admins of some team may look users up ([`Permission::ManageTeam`]); everyone else gets
    /// [`ServiceError::Forbidden`].
    pub async fn find_users(&self, principal: Principal, query: UserQuery) -> Result<Vec<UserMatch>, ServiceError> {
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let is_admin = teams
            .iter()
            .any(|(_, role)| Permission::ManageTeam.is_granted_to(*role));
        if !is_admin {
            return Err(ServiceError::Forbidden(Permission::ManageTeam));
        }
        let by_email = match query.as_email_prefix() {
            Some(prefix) => store::users::find_users_by_email_prefix(&self.db, prefix, USER_LOOKUP_LIMIT).await?,
            None => Vec::new(),
        };
        let by_name = store::users::find_users_by_name(&self.db, &query, USER_LOOKUP_LIMIT).await?;

        let mut matches: Vec<UserMatch> = by_email
            .into_iter()
            .map(|user| UserMatch {
                user,
                email_matched: true,
            })
            .collect();
        for user in by_name {
            if !matches.iter().any(|m| m.user.id == user.id) {
                matches.push(UserMatch {
                    user,
                    email_matched: false,
                });
            }
        }
        matches
            .sort_by(|a, b| (a.user.display_name.as_str(), a.user.id).cmp(&(b.user.display_name.as_str(), b.user.id)));
        matches.truncate(usize::try_from(USER_LOOKUP_LIMIT).unwrap_or(usize::MAX));
        Ok(matches)
    }
}

/// A user found by [`Services::find_users`].
#[derive(Debug)]
pub struct UserMatch {
    pub user: User,
    /// Whether the query matched the start of their email, which the caller may then be shown.
    pub email_matched: bool,
}
