//! Heap use cases: creating, renaming, archiving and unarchiving. Heaps are never deleted.
//!
//! Every write locks the heap row, like item and tag writes do, so archiving waits for writes in flight and writes
//! that come after it see the heap archived ([`crate::access::authorize_heap_write`]).

use domain::audit::{AuditAction, EntityType};
use domain::heap::{Heap, HeapPolicy};
use domain::permission::{Permission, allowed_heap_actions};
use domain::text::Name;
use domain::{HeapId, TeamId, Timestamp, Version};
use serde_json::json;
use store::audit::NewAuditEntry;
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap, authorize_team, ensure_active};
use crate::teams::HeapView;
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Adds an active heap with the default ranking policy to the team.
    pub async fn create_heap(
        &self,
        principal: Principal,
        team_id: TeamId,
        name: Name,
    ) -> Result<HeapView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let membership = authorize_team(&txn, principal, team_id, Permission::ManageHeaps).await?;
        let now = self.now();
        let heap = Heap {
            id: HeapId::from_uuid(self.new_id(now)),
            team_id,
            name,
            policy: HeapPolicy::default(),
            archived_at: None,
            version: Version::INITIAL,
        };
        store::heaps::insert_heap(&txn, &heap, now).await?;
        let entry = self.heap_audit_entry(
            principal,
            &heap,
            AuditAction::HeapCreated,
            json!({ "name": heap.name.as_str() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, heap_id = %heap.id, user_id = %principal.user_id, "heap created");
        Ok(HeapView {
            allowed_actions: allowed_heap_actions(&heap, membership.role),
            heap,
        })
    }

    /// The heap with what the caller may do with it. Non-members get [`ServiceError::NotFound`].
    pub async fn load_heap(&self, principal: Principal, heap_id: HeapId) -> Result<HeapView, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        Ok(HeapView {
            allowed_actions: allowed_heap_actions(&access.heap, access.membership.role),
            heap: access.heap,
        })
    }

    /// Renames the heap if it is still at version `expected`. An archived heap is read-only, so renaming it fails
    /// with [`ServiceError::HeapArchived`]. The same name succeeds without an audit entry.
    pub async fn rename_heap(
        &self,
        principal: Principal,
        heap_id: HeapId,
        name: Name,
        expected: Version,
    ) -> Result<HeapView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap(&txn, principal, heap_id, Permission::ManageHeaps).await?;
        let current = lock(&txn, heap_id).await?;
        ensure_active(&current)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        let role = access.membership.role;
        if current.name == name {
            return Ok(HeapView {
                allowed_actions: allowed_heap_actions(&current, role),
                heap: current,
            });
        }

        let renamed = Heap {
            name,
            ..current.clone()
        };
        let now = self.now();
        let diff = json!({ "name": { "before": current.name.as_str(), "after": renamed.name.as_str() } });
        let saved = self
            .save_heap(txn, principal, &renamed, expected, AuditAction::HeapUpdated, diff, now)
            .await?;
        tracing::info!(%heap_id, user_id = %principal.user_id, "heap renamed");
        Ok(HeapView {
            allowed_actions: allowed_heap_actions(&saved, role),
            heap: saved,
        })
    }

    /// Makes the heap read-only for everyone. Archiving an archived heap succeeds and changes nothing.
    pub async fn archive_heap(&self, principal: Principal, heap_id: HeapId) -> Result<HeapView, ServiceError> {
        self.set_archived(principal, heap_id, true).await
    }

    /// Makes an archived heap writable again. Unarchiving an active heap succeeds and changes nothing.
    pub async fn unarchive_heap(&self, principal: Principal, heap_id: HeapId) -> Result<HeapView, ServiceError> {
        self.set_archived(principal, heap_id, false).await
    }

    /// No `If-Match` here: both requests say what the heap should end up as, so repeating or racing them is safe.
    async fn set_archived(
        &self,
        principal: Principal,
        heap_id: HeapId,
        archived: bool,
    ) -> Result<HeapView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap(&txn, principal, heap_id, Permission::ManageHeaps).await?;
        let current = lock(&txn, heap_id).await?;
        let role = access.membership.role;
        if current.is_archived() == archived {
            return Ok(HeapView {
                allowed_actions: allowed_heap_actions(&current, role),
                heap: current,
            });
        }

        let now = self.now();
        let changed = Heap {
            archived_at: archived.then_some(now),
            ..current.clone()
        };
        let action = if archived {
            AuditAction::HeapArchived
        } else {
            AuditAction::HeapUnarchived
        };
        let diff = json!({ "archived_at": {
            "before": current.archived_at.map(Timestamp::as_millis),
            "after": changed.archived_at.map(Timestamp::as_millis),
        } });
        let saved = self
            .save_heap(txn, principal, &changed, current.version, action, diff, now)
            .await?;
        tracing::info!(%heap_id, user_id = %principal.user_id, archived, "heap archive state changed");
        Ok(HeapView {
            allowed_actions: allowed_heap_actions(&saved, role),
            heap: saved,
        })
    }

    /// Writes `heap` and its audit entry, and commits. The caller holds the heap lock.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    async fn save_heap(
        &self,
        txn: store::DatabaseTransaction,
        principal: Principal,
        heap: &Heap,
        expected: Version,
        action: AuditAction,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> Result<Heap, ServiceError> {
        let saved = store::heaps::update_heap(&txn, heap, expected)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let entry = self.heap_audit_entry(principal, &saved, action, diff, now);
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(saved)
    }

    fn heap_audit_entry(
        &self,
        principal: Principal,
        heap: &Heap,
        action: AuditAction,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::Heap,
            entity_id: *heap.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

/// The heap as read under its row lock.
async fn lock(txn: &store::DatabaseTransaction, heap_id: HeapId) -> Result<Heap, ServiceError> {
    store::heaps::lock_heap(txn, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)
}
