//! Step use cases: adding a step to a task and editing one.
//!
//! A step write never changes the item's version, and an item write never changes a step's, so ticking a step
//! doesn't conflict with someone editing the item (architecture.md, Data model). Both use cases return the whole
//! item, so a client re-renders from one response.

use domain::audit::AuditAction;
use domain::permission::Permission;
use domain::step::{NewStep, Step, StepNumber, StepPatch, check_steps_editable, place_step};
use domain::{ItemId, StepId, Version};
use store::{StoreError, TransactionTrait};

use crate::access::authorize_heap_write;
use crate::audit::{describe_new_step, diff_steps};
use crate::items::{ItemView, build_item_view};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Adds a step to a task, at the end or right after step `fields.after`. The new step is the one with the
    /// highest number in the returned item.
    ///
    /// Fails with [`ItemError::Closed`](domain::item::ItemError::Closed) on a closed item, and with
    /// [`StepError::NeedsTask`](domain::step::StepError::NeedsTask), [`StepError::TooMany`](domain::step::StepError::TooMany)
    /// or [`StepError::AfterNotFound`](domain::step::StepError::AfterNotFound) when the step can't go there.
    pub async fn add_step(
        &self,
        principal: Principal,
        item_id: ItemId,
        fields: NewStep,
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
        check_steps_editable(&item)?;
        let steps = store::steps::list_steps_locking(&txn, item.id).await?;
        let placement = place_step(&item, &steps, fields.after)?;

        let now = self.now();
        // Makes room right after the step it follows. Nothing sits past the end, so appending moves no step.
        if let Some(previous) = placement.position.checked_sub(1) {
            store::steps::shift_positions_after(&txn, item.id, previous).await?;
        }
        let step = Step::new(StepId::from_uuid(self.new_id(now)), item.id, placement, fields, now);
        store::steps::insert_step(&txn, &step).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::StepAdded,
            &item,
            describe_new_step(&step),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let steps = store::steps::list_steps_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, steps, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            step = step.number.get(),
            user_id = %principal.user_id,
            "step added"
        );
        Ok(view)
    }

    /// Applies a partial edit to step `number` of an item if the step is still at version `expected`. Fails with
    /// [`ServiceError::NotFound`] when the item has no such step, and with
    /// [`ItemError::Closed`](domain::item::ItemError::Closed) on a closed item.
    ///
    /// A patch that changes nothing succeeds and returns the item unchanged, without an audit entry.
    pub async fn update_step(
        &self,
        principal: Principal,
        item_id: ItemId,
        number: StepNumber,
        patch: StepPatch,
        expected: Version,
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
        let steps = store::steps::list_steps_locking(&txn, item.id).await?;
        let current = steps
            .iter()
            .find(|step| step.number == number)
            .ok_or(ServiceError::NotFound)?;
        check_steps_editable(&item)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        let now = self.now();
        let role = access.membership.role;
        let Some(edited) = current.apply_patch(&patch) else {
            return build_item_view(&txn, &access.heap, role, item, steps, now).await;
        };
        let saved = store::steps::update_step(&txn, &edited, expected)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::StepUpdated,
            &item,
            diff_steps(current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let steps = store::steps::list_steps_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, role, item, steps, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            step = number.get(),
            user_id = %principal.user_id,
            version = saved.version.get(),
            "step updated"
        );
        Ok(view)
    }
}
