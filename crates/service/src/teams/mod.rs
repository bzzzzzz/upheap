//! Teams as their members see them. Changing teams and their members is in `manage`.

mod invite_links;
mod manage;

use domain::audit::{AuditAction, EntityType};
use domain::heap::{Heap, HeapPriority};
use domain::permission::{
    HeapAction, MemberAction, Permission, TeamAction, allowed_heap_actions, allowed_member_actions,
    allowed_team_actions,
};
use domain::team::{Membership, Role, Team};
use domain::user::User;
use domain::{HeapId, TeamId, Timestamp, UserId};
pub use invite_links::{InvitationView, InviteLinkView, InviteViewer};
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
    /// What the caller may do with the heap: every role that may view it can set their priority.
    pub allowed_actions: Vec<HeapAction>,
    /// The caller's own priority for the heap.
    pub priority: HeapPriority,
}

/// `heap` as a member with `role` sees it, with its open items counted at `now`.
pub(crate) async fn load_heap_view(
    db: &impl ConnectionTrait,
    heap: Heap,
    role: Role,
    user_id: UserId,
    now: Timestamp,
) -> Result<HeapView, ServiceError> {
    let open_count = count_heap_open_items(db, heap.id, now).await?;
    let priority = load_heap_priority(db, user_id, heap.id).await?;
    Ok(to_heap_view(heap, role, open_count, priority))
}

/// `user_id`'s priority for `heap_id`: the default until they set one.
pub(crate) async fn load_heap_priority(
    db: &impl ConnectionTrait,
    user_id: UserId,
    heap_id: HeapId,
) -> Result<HeapPriority, ServiceError> {
    Ok(store::heap_priorities::list_heap_priorities(db, user_id, &[heap_id])
        .await?
        .get(&heap_id)
        .copied()
        .unwrap_or(HeapPriority::DEFAULT))
}

/// How many open items `heap_id` has (see [`HeapView::open_count`]).
pub(crate) async fn count_heap_open_items(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    now: Timestamp,
) -> Result<u32, ServiceError> {
    Ok(count_open_items(db, &[heap_id], now)
        .await?
        .get(&heap_id)
        .copied()
        .unwrap_or(0))
}

/// `heap` as a member with `role` sees it, with `open_count` already counted and the member's `priority` loaded.
pub(crate) fn to_heap_view(heap: Heap, role: Role, open_count: u32, priority: HeapPriority) -> HeapView {
    HeapView {
        open_count,
        priority,
        allowed_actions: allowed_heap_actions(&heap, role),
        heap,
    }
}

/// `heaps`, in the same order, as the member `user_id` with `role` sees them, with their open items counted in one
/// query and their priorities in another.
pub(crate) async fn load_heap_views(
    db: &impl ConnectionTrait,
    heaps: Vec<Heap>,
    role: Role,
    user_id: UserId,
    now: Timestamp,
) -> Result<Vec<HeapView>, ServiceError> {
    let ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
    let open_counts = count_open_items(db, &ids, now).await?;
    let priorities = store::heap_priorities::list_heap_priorities(db, user_id, &ids).await?;
    Ok(heaps
        .into_iter()
        .map(|heap| {
            let open_count = open_counts.get(&heap.id).copied().unwrap_or(0);
            let priority = priorities.get(&heap.id).copied().unwrap_or(HeapPriority::DEFAULT);
            to_heap_view(heap, role, open_count, priority)
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
        load_team_detail(&self.db, principal, team, self.now()).await
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
            run_id: None,
            created_at: now,
        }
    }
}

/// The team as the caller sees it. Fails with [`ServiceError::NotFound`] if the caller isn't a member.
pub(super) async fn load_team_detail(
    db: &impl ConnectionTrait,
    principal: Principal,
    team: Team,
    now: Timestamp,
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
    let heaps = load_heap_views(db, heaps, caller.role, principal.user_id, now).await?;
    Ok(TeamDetail {
        allowed_actions: allowed_team_actions(&team, &memberships, principal.user_id),
        role: caller.role,
        team,
        members,
        heaps,
    })
}
