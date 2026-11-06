//! Team use cases: creating and renaming teams, and managing their members.
//!
//! Every team write locks the team row before it reads anything else, then reads the memberships and checks the
//! caller's permission on them, so two changes in one team run one after the other and the last-admin rule
//! ([`domain::team::ensure_admin_remains`]) sees both (see `lock_team_for`).

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::permission::{
    HeapAction, MemberAction, Permission, TeamAction, allowed_heap_actions, allowed_member_actions,
    allowed_team_actions,
};
use domain::team::{Membership, Role, Team, ensure_admin_remains};
use domain::text::Name;
use domain::user::User;
use domain::{TeamId, Timestamp, UserId, Version};
use serde_json::json;
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};
use uuid::Uuid;

use crate::access::authorize_team;
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
    /// Empty unless the caller is an admin of the heap's team.
    pub allowed_actions: Vec<HeapAction>,
}

/// Whether [`Services::add_member`] added someone or found them already in the team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberAdded {
    Added,
    AlreadyMember,
}

/// A team locked for a membership change, with its memberships read under the lock.
struct LockedTeam {
    team: Team,
    members: Vec<Membership>,
}

impl LockedTeam {
    fn find(&self, user_id: UserId) -> Option<Membership> {
        self.members.iter().find(|m| m.user_id == user_id).copied()
    }
}

impl Services {
    /// Creates a team with the caller as its only admin. The team has no heaps yet.
    pub async fn create_team(&self, principal: Principal, name: Name) -> Result<TeamDetail, ServiceError> {
        let now = self.now();
        let team = Team {
            id: TeamId::from_uuid(self.new_id(now)),
            name,
            created_at: now,
            version: Version::INITIAL,
        };
        let membership = Membership {
            team_id: team.id,
            user_id: principal.user_id,
            role: Role::Admin,
            joined_at: now,
        };

        let txn = self.db.begin().await.map_err(StoreError::from)?;
        store::users::find_user(&txn, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        store::teams::insert_team(&txn, &team).await?;
        store::teams::insert_membership(&txn, &membership).await?;
        let entries = [
            self.team_audit_entry(
                principal,
                team.id,
                AuditAction::TeamCreated,
                EntityType::Team,
                *team.id.as_uuid(),
                json!({ "name": team.name.as_str() }),
                now,
            ),
            self.team_audit_entry(
                principal,
                team.id,
                AuditAction::MemberAdded,
                EntityType::User,
                *principal.user_id.as_uuid(),
                json!({ "role": Role::Admin.code() }),
                now,
            ),
        ];
        for entry in &entries {
            store::audit::insert_audit_entry(&txn, entry).await?;
        }
        let detail = load_team_detail(&txn, principal, team).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(team_id = %detail.team.id, user_id = %principal.user_id, "team created");
        Ok(detail)
    }

    /// The team with its members and heaps. Non-members get [`ServiceError::NotFound`].
    pub async fn load_team(&self, principal: Principal, team_id: TeamId) -> Result<TeamDetail, ServiceError> {
        authorize_team(&self.db, principal, team_id, Permission::ViewTeam).await?;
        let team = store::teams::find_team(&self.db, team_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        load_team_detail(&self.db, principal, team).await
    }

    /// Renames the team if it is still at version `expected`. The same name succeeds without an audit entry.
    pub async fn rename_team(
        &self,
        principal: Principal,
        team_id: TeamId,
        name: Name,
        expected: Version,
    ) -> Result<TeamDetail, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        let current = locked.team;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        if current.name == name {
            return load_team_detail(&txn, principal, current).await;
        }

        let renamed = Team {
            name,
            ..current.clone()
        };
        let saved =
            store::teams::update_team(&txn, &renamed, expected)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        let now = self.now();
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::TeamUpdated,
            EntityType::Team,
            *team_id.as_uuid(),
            json!({ "name": { "before": current.name.as_str(), "after": saved.name.as_str() } }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let detail = load_team_detail(&txn, principal, saved).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, user_id = %principal.user_id, "team renamed");
        Ok(detail)
    }

    /// Adds an existing user to the team as a member. Adding someone who is already a member changes nothing and
    /// succeeds. Fails with [`ServiceError::UserNotFound`] when there is no such user.
    pub async fn add_member(
        &self,
        principal: Principal,
        team_id: TeamId,
        user_id: UserId,
    ) -> Result<(TeamDetail, MemberAdded), ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        if locked.find(user_id).is_some() {
            let detail = load_team_detail(&txn, principal, locked.team).await?;
            return Ok((detail, MemberAdded::AlreadyMember));
        }
        store::users::find_user(&txn, user_id)
            .await?
            .ok_or(ServiceError::UserNotFound { user_id })?;

        let now = self.now();
        let membership = Membership {
            team_id,
            user_id,
            role: Role::Member,
            joined_at: now,
        };
        store::teams::insert_membership(&txn, &membership).await?;
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::MemberAdded,
            EntityType::User,
            *user_id.as_uuid(),
            json!({ "role": Role::Member.code() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let detail = load_team_detail(&txn, principal, locked.team).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, member_id = %user_id, user_id = %principal.user_id, "member added");
        Ok((detail, MemberAdded::Added))
    }

    /// Makes a member an admin or an admin a member. Setting the role they already have succeeds without an audit
    /// entry. Fails with [`domain::team::TeamError::LastAdmin`] when it would leave the team without an admin, and
    /// with [`ServiceError::NotFound`] when `user_id` isn't a member.
    pub async fn change_member_role(
        &self,
        principal: Principal,
        team_id: TeamId,
        user_id: UserId,
        role: Role,
    ) -> Result<TeamDetail, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        let target = locked.find(user_id).ok_or(ServiceError::NotFound)?;
        if target.role == role {
            return load_team_detail(&txn, principal, locked.team).await;
        }
        ensure_admin_remains(team_id, &locked.members, user_id, Some(role))?;

        store::teams::update_role(&txn, team_id, user_id, role).await?;
        let now = self.now();
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::MemberRoleChanged,
            EntityType::User,
            *user_id.as_uuid(),
            json!({ "role": { "before": target.role.code(), "after": role.code() } }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let detail = load_team_detail(&txn, principal, locked.team).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, member_id = %user_id, user_id = %principal.user_id, role = role.code(), "member role changed");
        Ok(detail)
    }

    /// Takes `user_id` out of the team. Removing someone who isn't a member succeeds. Fails with
    /// [`domain::team::TeamError::LastAdmin`] for the last admin.
    pub async fn remove_member(
        &self,
        principal: Principal,
        team_id: TeamId,
        user_id: UserId,
    ) -> Result<(), ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        let Some(target) = locked.find(user_id) else {
            return Ok(());
        };
        // Removing yourself is leaving, and the audit log says so.
        let action = if user_id == principal.user_id {
            AuditAction::MemberLeft
        } else {
            AuditAction::MemberRemoved
        };
        self.end_membership(txn, principal, &locked, target, action).await
    }

    /// Takes the caller out of the team. Fails with [`domain::team::TeamError::LastAdmin`] for the last admin.
    pub async fn leave_team(&self, principal: Principal, team_id: TeamId) -> Result<(), ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::LeaveTeam).await?;
        let target = locked.find(principal.user_id).ok_or(ServiceError::NotFound)?;
        self.end_membership(txn, principal, &locked, target, AuditAction::MemberLeft)
            .await
    }

    async fn end_membership(
        &self,
        txn: store::DatabaseTransaction,
        principal: Principal,
        locked: &LockedTeam,
        target: Membership,
        action: AuditAction,
    ) -> Result<(), ServiceError> {
        let team_id = locked.team.id;
        ensure_admin_remains(team_id, &locked.members, target.user_id, None)?;
        store::teams::delete_membership(&txn, team_id, target.user_id).await?;
        let entry = self.team_audit_entry(
            principal,
            team_id,
            action,
            EntityType::User,
            *target.user_id.as_uuid(),
            json!({ "role": target.role.code() }),
            self.now(),
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, member_id = %target.user_id, user_id = %principal.user_id, action = action.as_str(), "membership ended");
        Ok(())
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

/// Locks the team row, reads the memberships, and checks the caller holds `permission` among them.
///
/// The lock comes before any other read on purpose. On MySQL the first plain read of a transaction fixes the snapshot
/// that later plain reads see, so a membership read made before the lock would hide changes that committed while
/// this one waited for it: two admins leaving at once would each still see the other, and both would go. Reading
/// after the lock sees every change that committed before it, including the caller's own demotion or removal.
///
/// Non-members get [`ServiceError::NotFound`], like a team that doesn't exist.
async fn lock_team_for(
    db: &impl ConnectionTrait,
    principal: Principal,
    team_id: TeamId,
    permission: Permission,
) -> Result<LockedTeam, ServiceError> {
    let team = store::teams::lock_team(db, team_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let members = store::teams::list_memberships(db, team_id).await?;
    let locked = LockedTeam { team, members };
    let caller = locked.find(principal.user_id).ok_or(ServiceError::NotFound)?;
    if !permission.is_granted_to(caller.role) {
        return Err(ServiceError::Forbidden(permission));
    }
    Ok(locked)
}

/// The team as the caller sees it. Fails with [`ServiceError::NotFound`] if the caller isn't a member.
async fn load_team_detail(
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
    let heaps = store::heaps::list_heaps_for_teams(db, &[team.id])
        .await?
        .into_iter()
        .map(|heap| HeapView {
            allowed_actions: allowed_heap_actions(&heap, caller.role),
            heap,
        })
        .collect();
    Ok(TeamDetail {
        allowed_actions: allowed_team_actions(&memberships, principal.user_id),
        role: caller.role,
        team,
        members,
        heaps,
    })
}
