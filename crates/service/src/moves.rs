//! Moving an item to another heap of its team (architecture.md, Handles, Cycle detection and locking, Audit log).

use domain::audit::AuditAction;
use domain::item::ItemError;
use domain::item_move::carry_over;
use domain::permission::Permission;
use domain::run::{RunError, RunForbidden};
use domain::{HeapId, ItemId, Version};
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap, ensure_active, find_item_locking_in};
use crate::audit::diff_moved;
use crate::items::{ItemChildren, ItemView, build_item_view, new_item_key};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Moves the item to `to_heap`, another heap of its team, if the item is still at version `expected`. It keeps
    /// its context, steps, questions, comments, turn, runs and history. It keeps its key unless the target heap has an item
    /// with it, and then gets a new one. Its tags and values carry over where the target heap has a tag of the same
    /// name, or a parameter in use and an option in use of the same names ([`domain::item_move::carry_over`]); the
    /// rest are dropped. It stays in its state, and a blocked item stays blocked with nobody picked. Items of the
    /// heap it leaves that waited for it stay blocked by it, and wait for the person, as their blocker is gone
    /// ([`domain::item::Item::blocker_gone`]); if it comes back, it holds them again.
    ///
    /// Moving an item to the heap it is in changes nothing: it succeeds, after the version check, without an entry.
    ///
    /// Fails with [`ServiceError::HeapNotInTeam`] unless `to_heap` is a heap of the item's team, with
    /// [`ServiceError::HeapArchived`] when either heap is archived, and with [`ItemError::Closed`] for a closed item.
    /// A run may not move items.
    pub async fn move_item(
        &self,
        principal: Principal,
        item_id: ItemId,
        to_heap: HeapId,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        if principal.run.is_some() {
            return Err(RunError::Forbidden(RunForbidden::Person).into());
        }
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let from_heap = current.heap_id;
        let access = authorize_heap(&txn, principal, from_heap, Permission::EditItems).await?;
        if to_heap == from_heap {
            if current.version != expected {
                return Err(ServiceError::VersionConflict {
                    expected,
                    current: current.version,
                });
            }
            drop(txn);
            return self.find_item(principal, item_id).await;
        }
        // The caller is a member of the item's team, so a heap of that team is one they can see. Every other heap
        // gets the same error, whether it is missing, someone else's or in another team.
        let not_in_team = || ServiceError::HeapNotInTeam { heap_id: to_heap };
        let target = store::heaps::find_heap(&txn, to_heap).await?.ok_or_else(not_in_team)?;
        if target.team_id != access.heap.team_id {
            return Err(not_in_team());
        }

        // Both heap locks first, in heap ID order as `claim_run` takes them, so two moves in opposite directions
        // can't wait for each other. Every read after them sees what committed while this waited.
        let (low, high) = (from_heap.min(to_heap), from_heap.max(to_heap));
        let first = store::heaps::lock_heap(&txn, low)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let second = store::heaps::lock_heap(&txn, high)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let (source, target) = if first.id == from_heap {
            (first, second)
        } else {
            (second, first)
        };

        let item = find_item_locking_in(&txn, item_id, from_heap, expected).await?;
        if item.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: item.version,
            });
        }
        ensure_active(&source)?;
        ensure_active(&target)?;
        if item.is_closed() {
            return Err(ItemError::Closed { item_id }.into());
        }

        let source_tags = store::tags::list_tags(&txn, from_heap).await?;
        let source_parameters = store::parameters::list_parameters_locking(&txn, from_heap).await?;
        let target_tags = store::tags::list_tags(&txn, to_heap).await?;
        let target_parameters = store::parameters::list_parameters_locking(&txn, to_heap).await?;
        let carried = carry_over(
            &item.tags,
            &item.parameters,
            (&source_tags, &source_parameters),
            (&target_tags, &target_parameters),
        );

        let now = self.now();
        let key = if store::items::is_key_taken(&txn, to_heap, item.key).await? {
            new_item_key(&txn, &*self.random, to_heap).await?
        } else {
            item.key
        };

        let moved = item.moved_to(to_heap, key, carried.tags, carried.parameters);
        let saved =
            store::items::update_item(&txn, &moved, expected, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        // Recorded in the heap the item is in after the write, so its history reads by entity across the team's heaps.
        let entry = self.item_audit_entry(
            &target,
            principal,
            AuditAction::ItemMoved,
            &saved,
            diff_moved(&item, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, &target, access.membership.role, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            item_id = %item_id,
            from_heap_id = %from_heap,
            to_heap_id = %to_heap,
            user_id = %principal.user_id,
            key_changed = key != item.key,
            tags_dropped = carried.tags_dropped,
            values_dropped = carried.values_dropped,
            "item moved"
        );
        Ok(view)
    }
}
