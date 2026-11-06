//! Resource scoping and permission checks shared by the use cases.

use domain::HeapId;
use domain::heap::Heap;
use domain::permission::Permission;
use domain::team::Membership;
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

/// Writes to an archived heap are refused for everyone.
pub(crate) const fn ensure_active(heap: &Heap) -> Result<(), ServiceError> {
    if heap.is_archived() {
        return Err(ServiceError::HeapArchived { heap_id: heap.id });
    }
    Ok(())
}
