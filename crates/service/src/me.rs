//! The signed-in user and what they can reach.

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::team::{Role, Team};
use domain::user::{DisplayName, User};
use domain::{HeapId, TeamId, Version};
use serde_json::json;
use store::{StoreError, TransactionTrait};

use crate::items::count_open_items;
use crate::{Principal, ServiceError, Services};

/// The caller, their teams and each team's heaps.
#[derive(Debug)]
pub struct MeView {
    pub user: User,
    /// Whether the user has a local password: not a user who signs in through an OpenID Connect provider.
    pub has_password: bool,
    pub teams: Vec<TeamView>,
}

#[derive(Debug)]
pub struct TeamView {
    pub team: Team,
    pub role: Role,
    /// Everyone in the team, the caller included.
    pub member_count: u32,
    /// By name, archived ones included.
    pub heaps: Vec<HeapOverview>,
}

/// A heap and how many items it has open.
#[derive(Debug)]
pub struct HeapOverview {
    pub heap: Heap,
    /// Items its list shows by default, before filters: open and in progress ones.
    pub open_count: u32,
}

impl Services {
    pub async fn load_me(&self, principal: Principal) -> Result<MeView, ServiceError> {
        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let has_password = store::users::find_password_hash(&self.db, principal.user_id)
            .await?
            .is_some();
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let team_ids: Vec<_> = teams.iter().map(|(team, _)| team.id).collect();
        let member_counts = store::teams::count_members_by_team(&self.db, &team_ids).await?;
        let mut heaps = store::heaps::list_heaps_for_teams(&self.db, &team_ids).await?;
        let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
        let open_counts = count_open_items(&self.db, &heap_ids, self.now()).await?;

        let teams = teams
            .into_iter()
            .map(|(team, role)| {
                let (own, rest): (Vec<Heap>, Vec<Heap>) = heaps.drain(..).partition(|heap| heap.team_id == team.id);
                heaps = rest;
                let heaps = own
                    .into_iter()
                    .map(|heap| HeapOverview {
                        open_count: open_counts.get(&heap.id).copied().unwrap_or(0),
                        heap,
                    })
                    .collect();
                TeamView {
                    member_count: member_counts.get(&team.id).copied().unwrap_or(0),
                    team,
                    role,
                    heaps,
                }
            })
            .collect();
        Ok(MeView {
            user,
            has_password,
            teams,
        })
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
}
