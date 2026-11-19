//! Moving an item to another heap, of its team or another (architecture.md, Handles, Cycle detection and locking,
//! Audit log).

use domain::audit::AuditAction;
use domain::heap::Heap;
use domain::item::{ItemError, Kind};
use domain::item_move::carry_over;
use domain::permission::Permission;
use domain::run::{RunError, RunForbidden};
use domain::turn::TakeBackWhy;
use domain::{HeapId, ItemId, Timestamp, Version};
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap, ensure_active, find_item_locking_in};
use crate::audit::{diff_moved, diff_moved_in, diff_moved_out};
use crate::items::{ItemChildren, ItemView, build_item_view, ensure_tags_in_use, new_item_key};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Moves the item to `to_heap`, a heap the caller may edit items in, of the item's team or another, if the item is
    /// still at version `expected`. It keeps its context, steps, questions, comments, turn, runs and history. It keeps
    /// its key unless the target heap has an item with it, and then gets a new one. Its tags and values carry over
    /// where the target heap has a tag of the same name, or a parameter in use and an option in use of the same names
    /// ([`domain::item_move::carry_over`]); the rest are dropped. It stays in its state, and a blocked item stays
    /// blocked with nobody picked. Items of the heap it leaves that waited for it stay blocked by it, and wait for
    /// the person, as their blocker is gone ([`domain::item::Item::blocker_gone`]); if it comes back, it holds them
    /// again.
    ///
    /// A move within the team records one `item_moved` entry, in the target heap. A move to another team records two,
    /// each in the team's own log and none copied: in the source heap that the item left, with its title so that log
    /// keeps naming it, and in the target heap that it came from another team, with nothing of the source team's.
    /// The item's open turn stays when its person may edit items in the target heap, and is taken back, in the source
    /// heap's log, otherwise.
    ///
    /// Moving an item to the heap it is in changes nothing: it succeeds, after the version check, without an entry.
    ///
    /// Fails with [`ServiceError::MoveTargetNotFound`] when `to_heap` is missing or in a team the caller isn't in,
    /// with [`ServiceError::HeapArchived`] when either heap is archived, with [`ItemError::Closed`] for a closed item,
    /// and with [`ServiceError::NoteHasHeaps`] for a note that heaps were made from, archived ones included, moved to
    /// another team. A run may not
    /// move items.
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
        authorize_heap(&txn, principal, from_heap, Permission::EditItems).await?;
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
        // A heap that is missing and one in a team the caller isn't in get the same error, so IDs can't be probed.
        let target_access = match authorize_heap(&txn, principal, to_heap, Permission::EditItems).await {
            Err(ServiceError::NotFound) => return Err(ServiceError::MoveTargetNotFound { heap_id: to_heap }),
            other => other?,
        };

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
        let across_teams = target.team_id != source.team_id;
        if across_teams
            && item.kind == Kind::Note
            && !store::heaps::list_heaps_made_from(&txn, &[item_id]).await?.is_empty()
        {
            return Err(ServiceError::NoteHasHeaps { item_id });
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

        ensure_tags_in_use(&txn, &carried.tags).await?;

        let now = self.now();
        if across_teams {
            self.take_back_unfit_turn(&txn, &source, &target, principal, item_id, now)
                .await?;
        }
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
        store::items::insert_former_handle(&txn, item_id, from_heap, item.key, now).await?;
        // Recorded in the heap the item is in after the write, so its history reads by entity across the team's heaps.
        // A move to another team also leaves an entry in the heap it left, for that team's log.
        if across_teams {
            let left = self.item_audit_entry(
                &source,
                principal,
                AuditAction::ItemMoved,
                &saved,
                diff_moved_out(&item),
                now,
            );
            store::audit::insert_audit_entry(&txn, &left).await?;
        }
        let diff = if across_teams {
            diff_moved_in(&saved)
        } else {
            diff_moved(&item, &saved)
        };
        let entry = self.item_audit_entry(&target, principal, AuditAction::ItemMoved, &saved, diff, now);
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, &target, &target_access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            item_id = %item_id,
            from_heap_id = %from_heap,
            to_heap_id = %to_heap,
            user_id = %principal.user_id,
            across_teams,
            key_changed = key != item.key,
            tags_dropped = carried.tags_dropped,
            values_dropped = carried.values_dropped,
            "item moved"
        );
        Ok(view)
    }

    /// Takes back the item's open turn, with its entry in `source`, the heap the turn belonged to, unless its person
    /// may edit items in `target`'s team. A turn whose opener can't be traced goes too.
    async fn take_back_unfit_turn(
        &self,
        txn: &store::DatabaseTransaction,
        source: &Heap,
        target: &Heap,
        principal: Principal,
        item_id: ItemId,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let Some(turn) = store::turns::find_open_turn_locking(txn, item_id).await? else {
            return Ok(());
        };
        if let Some(person) = turn.opened_by {
            let membership = store::teams::find_membership(txn, target.team_id, person).await?;
            if membership.is_some_and(|membership| Permission::EditItems.is_granted_to(membership.role)) {
                return Ok(());
            }
        }
        self.take_back_turn(txn, source, principal, &turn, TakeBackWhy::Moved, now)
            .await?;
        Ok(())
    }
}
