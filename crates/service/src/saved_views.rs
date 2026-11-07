//! Saved view use cases: listing a heap's views, saving a new one, changing one and archiving it.
//!
//! Every write locks the heap first, like item and tag writes, so a view can't be saved into a heap archived while
//! the write waited. Views are archived, never deleted.

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::list_view::ListView;
use domain::permission::{
    Permission, SavedViewAction, SavedViewListAction, allowed_saved_view_actions, allowed_saved_view_list_actions,
};
use domain::saved_view::{NewSavedView, SavedView, SavedViewPatch};
use domain::team::Role;
use domain::{HeapId, SavedViewId, Timestamp, Version};
use serde_json::json;
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, DatabaseTransaction, StoreError, TransactionTrait};

use crate::access::{authorize_heap, authorize_heap_write};
use crate::audit::{describe_new_saved_view, diff_saved_views};
use crate::items::ensure_view_in_heap;
use crate::{Principal, ServiceError, Services};

/// A saved view with what the caller may do with it.
#[derive(Debug)]
pub struct SavedViewEntry {
    pub view: SavedView,
    pub allowed_actions: Vec<SavedViewAction>,
}

/// The saved views of a heap that aren't archived, by name.
#[derive(Debug)]
pub struct SavedViewList {
    pub heap: Heap,
    pub views: Vec<SavedViewEntry>,
    pub allowed_actions: Vec<SavedViewListAction>,
}

impl Services {
    pub async fn list_saved_views(&self, principal: Principal, heap_id: HeapId) -> Result<SavedViewList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let role = access.membership.role;
        let views = store::saved_views::list_saved_views(&self.db, heap_id)
            .await?
            .into_iter()
            .map(|view| with_actions(view, &access.heap, role))
            .collect();
        Ok(SavedViewList {
            allowed_actions: allowed_saved_view_list_actions(&access.heap, role),
            heap: access.heap,
            views,
        })
    }

    /// Saves a view of the heap. Every tag and option the view filters by, and the parameter it sorts by, must be of
    /// the heap, or it fails with [`ServiceError::TagNotInHeap`], [`ServiceError::OptionNotInHeap`] or
    /// [`domain::parameter::ParameterError::NotInHeap`].
    pub async fn create_saved_view(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewSavedView,
    ) -> Result<SavedViewEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::EditItems).await?;
        ensure_view_belongs_to_heap(&txn, heap_id, &fields.list_view).await?;

        let now = self.now();
        let view = SavedView::new(
            SavedViewId::from_uuid(self.new_id(now)),
            heap_id,
            fields,
            principal.user_id,
            now,
        );
        store::saved_views::insert_saved_view(&txn, &view).await?;
        let audit_entry = self.saved_view_audit_entry(
            &access.heap,
            principal,
            AuditAction::SavedViewCreated,
            &view,
            describe_new_saved_view(&view),
            now,
        );
        store::audit::insert_audit_entry(&txn, &audit_entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, view_id = %view.id, user_id = %principal.user_id, "saved view created");
        Ok(with_actions(view, &access.heap, access.membership.role))
    }

    /// Renames a view or replaces what it shows, if it is still at version `expected`. An archived view fails with
    /// [`ServiceError::SavedViewArchived`].
    ///
    /// A patch that changes nothing succeeds and returns the view unchanged, without an audit entry.
    pub async fn update_saved_view(
        &self,
        principal: Principal,
        view_id: SavedViewId,
        patch: SavedViewPatch,
        expected: Version,
    ) -> Result<SavedViewEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, view_id).await?;
        if current.is_archived() {
            return Err(ServiceError::SavedViewArchived { view_id });
        }
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        let Some(edited) = current.apply_patch(&patch) else {
            return Ok(with_actions(current, &heap, role));
        };
        if edited.list_view != current.list_view {
            ensure_view_belongs_to_heap(&txn, heap.id, &edited.list_view).await?;
        }

        let now = self.now();
        let diff = diff_saved_views(&current, &edited);
        let saved = self
            .save_saved_view(txn, &heap, principal, &edited, AuditAction::SavedViewUpdated, diff, now)
            .await?;
        tracing::info!(heap_id = %heap.id, %view_id, user_id = %principal.user_id, version = saved.version.get(), "saved view updated");
        Ok(with_actions(saved, &heap, role))
    }

    /// Takes the view out of the heap's list. Archiving an archived view succeeds and changes nothing.
    ///
    /// No `If-Match`: the request says what the view should end up as, so repeating or racing it is safe.
    pub async fn archive_saved_view(
        &self,
        principal: Principal,
        view_id: SavedViewId,
    ) -> Result<SavedViewEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, view_id).await?;
        if current.is_archived() {
            return Ok(with_actions(current, &heap, role));
        }

        let now = self.now();
        let archived = SavedView {
            archived_at: Some(now),
            ..current.clone()
        };
        let diff = json!({ "archived_at": { "before": null, "after": now.as_millis() } });
        let saved = self
            .save_saved_view(
                txn,
                &heap,
                principal,
                &archived,
                AuditAction::SavedViewArchived,
                diff,
                now,
            )
            .await?;
        tracing::info!(heap_id = %heap.id, %view_id, user_id = %principal.user_id, "saved view archived");
        Ok(with_actions(saved, &heap, role))
    }

    /// Writes `view` over the version it was read at, records `action`, and commits. The caller holds the heap
    /// and view locks, so the version can't have moved.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    async fn save_saved_view(
        &self,
        txn: DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        view: &SavedView,
        action: AuditAction,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> Result<SavedView, ServiceError> {
        let saved = store::saved_views::update_saved_view(&txn, view, view.version)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected: view.version,
                current: view.version.next(),
            })?;
        let audit_entry = self.saved_view_audit_entry(heap, principal, action, &saved, diff, now);
        store::audit::insert_audit_entry(&txn, &audit_entry).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(saved)
    }

    fn saved_view_audit_entry(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        view: &SavedView,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::SavedView,
            entity_id: *view.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

fn with_actions(view: SavedView, heap: &Heap, role: Role) -> SavedViewEntry {
    SavedViewEntry {
        allowed_actions: allowed_saved_view_actions(&view, heap, role),
        view,
    }
}

/// Finds the view's heap, authorizes the write there and locks the heap, then reads the view under its own lock.
///
/// The first read only finds the heap. The view is read again under the lock, because on MySQL a plain read sees
/// the snapshot from before the heap lock was granted, which can miss a write that committed while this one waited.
async fn lock_for_write(
    txn: &DatabaseTransaction,
    principal: Principal,
    view_id: SavedViewId,
) -> Result<(Heap, Role, SavedView), ServiceError> {
    let found = store::saved_views::find_saved_view(txn, view_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let access = authorize_heap_write(txn, principal, found.heap_id, Permission::EditItems).await?;
    let current = store::saved_views::lock_saved_view(txn, view_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok((access.heap, access.membership.role, current))
}

async fn ensure_view_belongs_to_heap(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    view: &ListView,
) -> Result<(), ServiceError> {
    domain::saved_view::ensure_savable(view)?;
    let heap_tags = store::tags::list_tags(db, heap_id).await?;
    let parameters = store::parameters::list_parameters(db, heap_id).await?;
    ensure_view_in_heap(&heap_tags, &parameters, view)
}
