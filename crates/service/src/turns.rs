//! Turn use cases: sending an item to an agent, and taking it back.
//!
//! A turn write never changes the item's version, and an item write never changes a turn's, so sending an item
//! doesn't conflict with someone editing it (architecture.md, Data model). Both use cases return the whole item, so
//! a client re-renders from one response. Starting or closing an item, and archiving its heap, take its turn back
//! too, through [`Services::take_back_open_turn`].

use domain::audit::AuditAction;
use domain::heap::Heap;
use domain::item::ItemError;
use domain::permission::Permission;
use domain::turn::{Reach, TakeBackWhy, Turn, TurnError, TurnLine, TurnSource};
use domain::{ItemId, Timestamp, TurnId};
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::access::authorize_heap_write;
use crate::audit::{describe_new_turn, describe_take_back};
use crate::items::{ItemChildren, ItemView, build_item_view};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Sends an item to an agent: opens a turn of `reach` with an optional `line` from the person, going to the
    /// next stable state. Any member may.
    ///
    /// Fails with [`ItemError::Closed`] on a closed item, [`ServiceError::HeapArchived`] in an archived heap, and
    /// [`TurnError::AlreadyOpen`] when an agent already has the item. An item with an open question or blocked may
    /// be sent: the turn opens, and the item waits or is blocked as before.
    pub async fn send_to_agent(
        &self,
        principal: Principal,
        item_id: ItemId,
        reach: Reach,
        line: Option<TurnLine>,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = store::items::find_item_locking(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        if item.is_closed() {
            return Err(ItemError::Closed { item_id: item.id }.into());
        }
        if store::turns::find_open_turn_locking(&txn, item.id).await?.is_some() {
            return Err(TurnError::AlreadyOpen { item_id: item.id }.into());
        }

        let now = self.now();
        let turn = Turn::open(
            TurnId::from_uuid(self.new_id(now)),
            &item,
            reach,
            line,
            TurnSource::GoAhead,
            principal.user_id,
            now,
        );
        store::turns::insert_turn(&txn, &turn).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::TurnOpened,
            &item,
            describe_new_turn(&turn),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            reach = turn.reach.code(),
            goal = turn.goal.code(),
            user_id = %principal.user_id,
            "turn opened"
        );
        Ok(view)
    }

    /// Takes an item back from the agent that has it. An item with no open turn is returned unchanged, without an
    /// audit entry, so two clients taking back at once both succeed. Fails with [`ServiceError::HeapArchived`] in an
    /// archived heap.
    pub async fn take_back(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = store::items::find_item_locking(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;

        let now = self.now();
        let taken = self
            .take_back_open_turn(&txn, &access.heap, principal, item.id, TakeBackWhy::TakeBack, now)
            .await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        if !taken {
            // Nothing was written.
            return Ok(view);
        }
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            user_id = %principal.user_id,
            "turn taken back"
        );
        Ok(view)
    }

    /// Ends the open turn of `item_id`, if it has one, with its `turn_taken_back` entry, and says whether there was
    /// one. The caller holds the heap lock, which keeps two writes from ending or opening a turn at once.
    pub(crate) async fn take_back_open_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        item_id: ItemId,
        why: TakeBackWhy,
        now: Timestamp,
    ) -> Result<bool, ServiceError> {
        let Some(turn) = store::turns::find_open_turn_locking(txn, item_id).await? else {
            return Ok(false);
        };
        self.take_back_turn(txn, heap, principal, &turn, why, now).await
    }

    /// Ends `turn`, which the caller read under the heap lock, with its `turn_taken_back` entry. Says whether it
    /// was still open.
    pub(crate) async fn take_back_turn(
        &self,
        txn: &impl ConnectionTrait,
        heap: &Heap,
        principal: Principal,
        turn: &Turn,
        why: TakeBackWhy,
        now: Timestamp,
    ) -> Result<bool, ServiceError> {
        let ended = turn.take_back(principal.user_id, now);
        if !store::turns::end_turn(txn, &ended).await? {
            return Ok(false);
        }
        let entry = self.item_audit_entry_for(
            heap,
            principal,
            AuditAction::TurnTakenBack,
            turn.item_id,
            describe_take_back(turn, why),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(true)
    }
}
