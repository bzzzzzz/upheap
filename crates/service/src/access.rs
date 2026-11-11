//! Resource scoping and permission checks shared by the use cases.

use domain::heap::Heap;
use domain::item::Item;
use domain::permission::Permission;
use domain::team::Membership;
use domain::{HeapId, ItemId, TeamId, Version};
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

/// Reads the item again with locking reads, once the lock of `heap_id` is held, and checks it is still in that heap.
///
/// The caller found `heap_id` with a read taken before the lock. A move out of that heap can commit while the caller
/// waited for the lock, and a write would then land under the old heap's lock only. The item changed under the
/// caller, so it gets [`ServiceError::VersionConflict`] with the version it read (`expected`) and the current one,
/// and the client reads the item again. [`ServiceError::NotFound`] if the item is gone.
pub(crate) async fn find_item_locking_in(
    db: &impl ConnectionTrait,
    item_id: ItemId,
    heap_id: HeapId,
    expected: Version,
) -> Result<Item, ServiceError> {
    let item = store::items::find_item_locking(db, item_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if item.heap_id != heap_id {
        return Err(ServiceError::VersionConflict {
            expected,
            current: item.version,
        });
    }
    Ok(item)
}
