//! Resource scoping and permission checks shared by the use cases.

use domain::heap::Heap;
use domain::item::Item;
use domain::permission::Permission;
use domain::team::Membership;
use domain::{HeapId, ItemId, TeamId, Version};
use store::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, StoreError, TransactionTrait};

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

/// Begins a write to a team's tags or parameters: a transaction holding the lock of every heap of the team, in heap
/// ID order, with the caller's membership and the heaps as read under those locks.
///
/// The heap IDs are read before the transaction, so the locks come before every other read in it (on MySQL a
/// transaction's first plain read fixes its snapshot). Under the locks they are read again: a heap made in between
/// could already hold a heap tag or parameter of the name this write takes, so if the set changed, the transaction
/// is dropped and the write starts again with the new set. A heap made after that read meets this write on the team's
/// lowest heap, which a heap tag or parameter write locks shared first (see [`authorize_heap_write_under_team`]). A
/// team with no heaps has no lowest heap, so there the write locks the team row instead: making a heap checks the team row's key, so it waits for
/// this write, and with no heaps there is no item write to deadlock with.
pub(crate) async fn begin_team_write(
    db: &DatabaseConnection,
    principal: Principal,
    team_id: TeamId,
    permission: Permission,
) -> Result<(DatabaseTransaction, Membership, Vec<Heap>), ServiceError> {
    let mut heap_ids = team_heap_ids(db, team_id).await?;
    loop {
        let txn = db.begin().await.map_err(StoreError::from)?;
        if heap_ids.is_empty() {
            store::teams::lock_team(&txn, team_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
        }
        let (membership, heaps) = authorize_team_write(&txn, principal, team_id, &heap_ids, permission).await?;
        let now_ids = team_heap_ids(&txn, team_id).await?;
        if now_ids == heap_ids {
            return Ok((txn, membership, heaps));
        }
        txn.rollback().await.map_err(StoreError::from)?;
        heap_ids = now_ids;
    }
}

/// The IDs of the team's heaps, archived ones included, in heap ID order (see [`begin_team_write`]).
pub(crate) async fn team_heap_ids(db: &impl ConnectionTrait, team_id: TeamId) -> Result<Vec<HeapId>, ServiceError> {
    let mut ids: Vec<HeapId> = store::heaps::list_heaps_for_teams(db, &[team_id])
        .await?
        .into_iter()
        .map(|heap| heap.id)
        .collect();
    ids.sort();
    Ok(ids)
}

/// The team's lowest heap ID when it isn't `heap_id` itself, read before a heap tag or parameter write's transaction
/// begins (see [`authorize_heap_write_under_team`]). `None` for the lowest heap itself, and for a heap that doesn't
/// exist. Heaps are never deleted and never change team, and their IDs are UUIDv7, rising with the time they were made, so the lowest one
/// stays the lowest, as long as the servers' clocks agree to within the age of the team's first heap.
pub(crate) async fn lower_team_heap(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
) -> Result<Option<HeapId>, ServiceError> {
    let Some(heap) = store::heaps::find_heap(db, heap_id).await? else {
        return Ok(None);
    };
    let lowest = team_heap_ids(db, heap.team_id).await?.into_iter().next();
    Ok(lowest.filter(|lowest| *lowest != heap_id))
}

/// For a write to a team's tags or parameters: locks each heap of the team (`heap_ids`, from [`team_heap_ids`]) in
/// heap ID order, then checks the caller holds `permission`. Returns the heaps as read under their locks.
///
/// A merge writes items, parameters, views and heaps in every heap of the team, so it waits for the item writes in
/// flight and holds the others off, each of which holds its heap row. The locks come before every other read, so
/// the plain reads after them see what a competing write committed (on MySQL a transaction's first plain read fixes
/// its snapshot). A non-member gets [`ServiceError::NotFound`], as for a missing team.
async fn authorize_team_write(
    txn: &impl ConnectionTrait,
    principal: Principal,
    team_id: TeamId,
    heap_ids: &[HeapId],
    permission: Permission,
) -> Result<(Membership, Vec<Heap>), ServiceError> {
    let mut heaps = Vec::with_capacity(heap_ids.len());
    for heap_id in heap_ids {
        heaps.push(
            store::heaps::lock_heap(txn, *heap_id)
                .await?
                .ok_or(ServiceError::NotFound)?,
        );
    }
    let membership = authorize_team(txn, principal, team_id, permission).await?;
    Ok((membership, heaps))
}

/// For a write to a heap's tags or parameters that checks names against the team's: locks the heap row, checks the
/// caller holds `permission`, and refuses an archived heap on the row read under the lock.
///
/// `lower` is the team's lowest heap when that isn't this heap ([`lower_team_heap`]); it is locked shared first. A
/// team write locks every heap it read before its transaction exclusively, the lowest among them, so a heap write and
/// a team write of the heap's team meet there even when this heap was made after the team write read the team's
/// heaps, and can't take one name at once. The locks stay in ascending heap ID order. The locks come first, so the
/// plain reads after them (the name checks, which take no locking reads, since on MySQL those take gap locks that
/// deadlock writes of two teams) see what a competing write committed.
pub(crate) async fn authorize_heap_write_under_team(
    txn: &impl ConnectionTrait,
    principal: Principal,
    heap_id: HeapId,
    lower: Option<HeapId>,
    permission: Permission,
) -> Result<HeapAccess, ServiceError> {
    if let Some(lower) = lower {
        store::heaps::lock_heap_shared(txn, lower).await?;
    }
    let heap = store::heaps::lock_heap(txn, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let access = authorize_heap(txn, principal, heap_id, permission).await?;
    ensure_active(&heap)?;
    Ok(HeapAccess {
        heap,
        membership: access.membership,
    })
}
