//! Resource scoping and permission checks shared by the use cases.

use domain::heap::Heap;
use domain::permission::Permission;
use domain::team::Membership;
use domain::{HeapId, TeamId};
use store::ConnectionTrait;

use crate::{Principal, ServiceError};

/// A heap the caller may act on, with their membership in its team.
pub(crate) struct HeapAccess {
    pub heap: Heap,
    pub membership: Membership,
}

/// Resolves `heap_id` to its team and checks the caller holds `permission` there.
///
/// Non-members get [`ServiceError::NotFound`], exactly like a missing heap, so IDs can't be probed.
pub(crate) async fn authorize_heap(
    db: &impl ConnectionTrait,
    principal: Principal,
    heap_id: HeapId,
    permission: Permission,
) -> Result<HeapAccess, ServiceError> {
    let heap = store::heaps::find_heap(db, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let membership = store::teams::find_membership(db, heap.team_id, principal.user_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if !permission.is_granted_to(membership.role) {
        return Err(ServiceError::Forbidden(permission));
    }
    Ok(HeapAccess { heap, membership })
}

/// For writes to a heap's items and tags: [`authorize_heap`], then locks the heap row and refuses the write if the
/// heap is archived.
///
/// The archive check runs on the heap as read under the lock, so a write can't land in a heap that was archived
/// while it waited.
pub(crate) async fn authorize_heap_write(
    db: &impl ConnectionTrait,
    principal: Principal,
    heap_id: HeapId,
    permission: Permission,
) -> Result<HeapAccess, ServiceError> {
    let access = authorize_heap(db, principal, heap_id, permission).await?;
    let heap = store::heaps::lock_heap(db, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    ensure_active(&heap)?;
    Ok(HeapAccess {
        heap,
        membership: access.membership,
    })
}

/// Loads the caller's membership in `team_id` and checks it grants `permission`.
///
/// Non-members get [`ServiceError::NotFound`], exactly like a missing team.
pub(crate) async fn authorize_team(
    db: &impl ConnectionTrait,
    principal: Principal,
    team_id: TeamId,
    permission: Permission,
) -> Result<Membership, ServiceError> {
    let membership = store::teams::find_membership(db, team_id, principal.user_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if !permission.is_granted_to(membership.role) {
        return Err(ServiceError::Forbidden(permission));
    }
    Ok(membership)
}

/// Writes to an archived heap are refused for everyone.
pub(crate) const fn ensure_active(heap: &Heap) -> Result<(), ServiceError> {
    if heap.is_archived() {
        return Err(ServiceError::HeapArchived { heap_id: heap.id });
    }
    Ok(())
}
