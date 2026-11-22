//! Placing an item by hand in its heap's list, and undoing it (`domain::ranking`, Placement).

use domain::audit::AuditAction;
use domain::item::{Item, ItemError};
use domain::permission::Permission;
use domain::ranking::{CURRENT_FORMULA, ItemForRanking, PlaceSide, placement_for};
use domain::run::{RunError, RunForbidden};
use domain::{ItemId, Timestamp, Version};
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap_write, find_item_locking_in};
use crate::audit::diff_placed;
use crate::items::{ItemChildren, ItemView, build_item_view, load_open_items, ordered_tags};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Places the item right above or below `anchor`, another item of its heap that ranks, if the item is still at
    /// version `expected`. Its fields don't change: it gets a multiplier on its own score that puts it between the
    /// anchor and the item next to it, and keeps it until it is placed again or reset. The heap is ranked once, at
    /// `requested_as_of` as a list's is, with the item's own placement left out.
    ///
    /// Fails with [`PlaceError::AnchorNotRanked`](domain::ranking::PlaceError::AnchorNotRanked) when the anchor
    /// isn't an item of the heap that ranks (a missing item, the item itself, another heap's, a closed one and a
    /// reminder to come alike), and with [`ItemError::Closed`] for a closed item. A run may not place items.
    pub async fn place_item(
        &self,
        principal: Principal,
        item_id: ItemId,
        anchor: ItemId,
        side: PlaceSide,
        requested_as_of: Option<Timestamp>,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        if principal.run.is_some() {
            return Err(RunError::Forbidden(RunForbidden::Person).into());
        }
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, item_id, access.heap.id, expected).await?;
        check_placeable(&item, expected)?;

        let now = self.now();
        let as_of = self.effective_as_of(requested_as_of, now);
        let (ranked, _, parameters) = load_open_items(&txn, access.heap.id, now).await?;
        let tags = store::tags::list_tags(&txn, access.heap.id).await?;
        let inputs: Vec<ItemForRanking<'_>> = ranked.iter().map(ItemForRanking::from).collect();
        let placement = placement_for(
            CURRENT_FORMULA,
            &access.heap.policy,
            &parameters,
            &ordered_tags(&access.heap.tag_order, &tags),
            &inputs,
            &ItemForRanking::from(&item),
            anchor,
            side,
            as_of,
        )?;

        let placed = Item {
            placement: Some(placement),
            ..item.clone()
        };
        let saved = self
            .save_placement(&txn, &access.heap, principal, &item, placed, now)
            .await?;
        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, &access.heap, &access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            anchor_id = %anchor,
            above = side == PlaceSide::Above,
            user_id = %principal.user_id,
            "item placed"
        );
        Ok(view)
    }

    /// Clears the item's placement, so it ranks by its fields again, if it is still at version `expected`.
    ///
    /// Resetting an item that isn't placed succeeds, after the version check, without an entry. Fails with
    /// [`ItemError::Closed`] for a closed item. A run may not reset placements.
    pub async fn reset_placement(
        &self,
        principal: Principal,
        item_id: ItemId,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        if principal.run.is_some() {
            return Err(RunError::Forbidden(RunForbidden::Person).into());
        }
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, item_id, access.heap.id, expected).await?;
        check_placeable(&item, expected)?;

        let now = self.now();
        let saved = if item.placement.is_some() {
            let reset = Item {
                placement: None,
                ..item.clone()
            };
            self.save_placement(&txn, &access.heap, principal, &item, reset, now)
                .await?
        } else {
            item
        };
        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, &access.heap, &access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            user_id = %principal.user_id,
            "item placement reset"
        );
        Ok(view)
    }

    /// Writes `after`, which differs from `before`, the item as it was read at its version, in its placement, with its `item_placed` entry.
    async fn save_placement(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &domain::heap::Heap,
        principal: Principal,
        before: &Item,
        after: Item,
        now: Timestamp,
    ) -> Result<Item, ServiceError> {
        let expected = before.version;
        let saved =
            store::items::update_item(txn, &after, expected, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::ItemPlaced,
            &saved,
            diff_placed(before.placement, saved.placement),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(saved)
    }
}

/// The version check and the closed check both writes of a placement start with.
fn check_placeable(item: &Item, expected: Version) -> Result<(), ServiceError> {
    if item.version != expected {
        return Err(ServiceError::VersionConflict {
            expected,
            current: item.version,
        });
    }
    if item.is_closed() {
        return Err(ItemError::Closed { item_id: item.id }.into());
    }
    Ok(())
}
