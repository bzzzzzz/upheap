//! Teams as their members see them. Changing teams and their members is in `manage`, which needs accounts.

#[cfg(feature = "accounts")]
mod manage;

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::permission::{
    HeapAction, MemberAction, Permission, TeamAction, allowed_heap_actions, allowed_member_actions,
    allowed_team_actions,
};
use domain::team::{Membership, Role, Team};
use domain::user::User;
use domain::{HeapId, TeamId, Timestamp, UserId};
#[cfg(feature = "accounts")]
pub use manage::{MemberAdded, TeamPatch};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError};
use uuid::Uuid;

use crate::access::authorize_team;
use crate::items::count_open_items;
use crate::{Principal, ServiceError, Services};

/// A team as one of its members sees it.
#[derive(Debug)]
pub struct TeamDetail {
    pub team: Team,
    /// The caller's role in the team.
    pub role: Role,
    /// In the order they joined.
    pub members: Vec<MemberView>,
    /// By name, archived ones included.
    pub heaps: Vec<HeapView>,
    pub allowed_actions: Vec<TeamAction>,
}

/// One member of a team, with what the caller may do with them.
#[derive(Debug)]
pub struct MemberView {
    pub membership: Membership,
    /// The member's user, for their display name and email, which teammates see.
    pub user: User,
    /// Empty unless the caller is an admin.
    pub allowed_actions: Vec<MemberAction>,
}

/// A heap with what the caller may do with it.
#[derive(Debug)]
pub struct HeapView {
    pub heap: Heap,
    /// Items its list shows by default, before filters: open and in progress ones.
    pub open_count: u32,
    /// Empty unless the caller is an admin of the heap's team.
    pub allowed_actions: Vec<HeapAction>,
}

/// `heap` as a member with `role` sees it, with its open items counted.
pub(crate) async fn load_heap_view(
    db: &impl ConnectionTrait,
    heap: Heap,
    role: Role,
) -> Result<HeapView, ServiceError> {
    let open_counts = count_open_items(db, &[heap.id]).await?;
    Ok(HeapView {
        open_count: open_counts.get(&heap.id).copied().unwrap_or(0),
        allowed_actions: allowed_heap_actions(&heap, role),
        heap,
    })
}

/// `heaps`, in the same order, as a member with `role` sees them, with their open items counted in one query.
pub(crate) async fn load_heap_views(
    db: &impl ConnectionTrait,
    heaps: Vec<Heap>,
    role: Role,
) -> Result<Vec<HeapView>, ServiceError> {
    let ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
    let open_counts = count_open_items(db, &ids).await?;
    Ok(heaps
        .into_iter()
        .map(|heap| HeapView {
            open_count: open_counts.get(&heap.id).copied().unwrap_or(0),
            allowed_actions: allowed_heap_actions(&heap, role),
            heap,
        })
        .collect())
}

impl Services {
    /// The team with its members and heaps. Non-members get [`ServiceError::NotFound`].
    pub async fn load_team(&self, principal: Principal, team_id: TeamId) -> Result<TeamDetail, ServiceError> {
        authorize_team(&self.db, principal, team_id, Permission::ViewTeam).await?;
        let team = store::teams::find_team(&self.db, team_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        load_team_detail(&self.db, principal, team).await
    }

    #[allow(clippy::too_many_arguments)] // Every field of the entry differs between callers.
    pub(crate) fn team_audit_entry(
        &self,
        principal: Principal,
        team_id: TeamId,
        action: AuditAction,
        entity_type: EntityType,
        entity_id: Uuid,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id,
            heap_id: None,
            actor: principal.user_id,
            action,
            entity_type,
            entity_id,
            diff,
            created_at: now,
        }
    }
}

/// The team as the caller sees it. Fails with [`ServiceError::NotFound`] if the caller isn't a member.
pub(super) async fn load_team_detail(
    db: &impl ConnectionTrait,
    principal: Principal,
    team: Team,
) -> Result<TeamDetail, ServiceError> {
    let memberships = store::teams::list_memberships(db, team.id).await?;
    let caller = memberships
        .iter()
        .find(|m| m.user_id == principal.user_id)
        .copied()
        .ok_or(ServiceError::NotFound)?;
    let user_ids: Vec<UserId> = memberships.iter().map(|m| m.user_id).collect();
    let users = store::users::list_users(db, &user_ids).await?;
    let members = memberships
        .iter()
        .map(|membership| {
            let user = users
                .iter()
                .find(|u| u.id == membership.user_id)
                .cloned()
                // The foreign key on team_members guarantees the user row.
                .ok_or(StoreError::CorruptRow {
                    table: "team_members",
                    id: membership.user_id.to_string(),
                    problem: "member has no user".to_owned(),
                })?;
            Ok(MemberView {
                allowed_actions: allowed_member_actions(&memberships, &caller, membership),
                membership: *membership,
                user,
            })
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;
    let heaps = store::heaps::list_heaps_for_teams(db, &[team.id]).await?;
    let heaps = load_heap_views(db, heaps, caller.role).await?;
    Ok(TeamDetail {
        allowed_actions: allowed_team_actions(&team, &memberships, principal.user_id),
        role: caller.role,
        team,
        members,
        heaps,
    })
}
