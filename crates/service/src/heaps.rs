//! Heap use cases: creating, renaming, archiving and unarchiving. Heaps are never deleted.
//!
//! Every write locks the heap row, like item and tag writes do, so archiving waits for writes in flight and writes
//! that come after it see the heap archived ([`crate::access::authorize_heap_write`]).

use domain::audit::{AuditAction, EntityType};
use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy, HeapPolicyPatch};
use domain::parameter::{ParameterPatch, Weight};
use domain::permission::{Permission, allowed_heap_actions};
use domain::text::Name;
use domain::validation::Field;
use domain::{HeapId, ParameterId, TeamId, Timestamp, ValidationError, Version};
use serde_json::json;
use std::collections::BTreeSet;
use store::audit::NewAuditEntry;
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap, authorize_heap_write, authorize_team, ensure_active};
use crate::audit::{diff_parameters, diff_policies};
use crate::error::handle_taken_on_duplicate;
use crate::handles::{free_heap_slug, retry_derived_slug};
use crate::parameters::{ParameterEntry, list_parameters_in_use, lock_listed};
use crate::teams::{HeapView, count_heap_open_items, load_heap_view, to_heap_view};
use crate::{Principal, ServiceError, Services};

/// A change to a heap's name and slug. `None` leaves it as it is.
#[derive(Clone, Debug, Default)]
pub struct HeapPatch {
    pub name: Option<Name>,
    pub slug: Option<Handle>,
}

/// New weights for a heap and its parameters, saved together. `None` and unlisted parameters keep their weights.
#[derive(Clone, Debug, Default)]
pub struct WeightsChange {
    pub heap: HeapPolicyPatch,
    pub parameters: Vec<ParameterWeightChange>,
}

/// A new weight for one parameter, if it is still at `version`.
#[derive(Clone, Copy, Debug)]
pub struct ParameterWeightChange {
    pub id: ParameterId,
    pub weight: Weight,
    pub version: Version,
}

/// A heap and its parameters in use, as they are after a weights save.
#[derive(Debug)]
pub struct HeapWeightsView {
    pub heap: HeapView,
    /// By name.
    pub parameters: Vec<ParameterEntry>,
}

impl Services {
    /// Adds an active heap with the default ranking policy to the team. Without a `slug`, one is made from the name.
    /// Fails with [`ServiceError::HandleTaken`] when another heap of the team has the slug.
    pub async fn create_heap(
        &self,
        principal: Principal,
        team_id: TeamId,
        name: Name,
        slug: Option<Handle>,
    ) -> Result<HeapView, ServiceError> {
        let derived = slug.is_none();
        retry_derived_slug(derived, || {
            self.try_create_heap(principal, team_id, name.clone(), slug.clone())
        })
        .await
    }

    async fn try_create_heap(
        &self,
        principal: Principal,
        team_id: TeamId,
        name: Name,
        slug: Option<Handle>,
    ) -> Result<HeapView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let membership = authorize_team(&txn, principal, team_id, Permission::ManageHeaps).await?;
        let slug = match slug {
            Some(slug) => slug,
            None => free_heap_slug(&txn, team_id, &name).await?,
        };
        let now = self.now();
        let heap = Heap {
            id: HeapId::from_uuid(self.new_id(now)),
            team_id,
            slug,
            name,
            policy: HeapPolicy::default(),
            archived_at: None,
            version: Version::INITIAL,
        };
        store::heaps::insert_heap(&txn, &heap, now)
            .await
            .map_err(|e| handle_taken_on_duplicate(e, Field::Slug))?;
        let entry = self.heap_audit_entry(
            principal,
            &heap,
            AuditAction::HeapCreated,
            json!({ "slug": heap.slug.as_str(), "name": heap.name.as_str() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, heap_id = %heap.id, user_id = %principal.user_id, "heap created");
        Ok(HeapView {
            allowed_actions: allowed_heap_actions(&heap, membership.role),
            heap,
            // A new heap has no items.
            open_count: 0,
        })
    }

    /// The heap with what the caller may do with it. Non-members get [`ServiceError::NotFound`].
    pub async fn load_heap(&self, principal: Principal, heap_id: HeapId) -> Result<HeapView, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        load_heap_view(&self.db, access.heap, access.membership.role).await
    }

    /// Changes the heap's name or slug if it is still at version `expected`. An archived heap is read-only, so this
    /// fails with [`ServiceError::HeapArchived`]. A patch that changes nothing succeeds without an audit entry. A
    /// slug another heap of the team has fails with [`ServiceError::HandleTaken`].
    pub async fn update_heap(
        &self,
        principal: Principal,
        heap_id: HeapId,
        patch: HeapPatch,
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
        let name = patch.name.filter(|name| *name != current.name);
        let slug = patch.slug.filter(|slug| *slug != current.slug);
        if name.is_none() && slug.is_none() {
            return load_heap_view(&txn, current, role).await;
        }

        let mut diff = serde_json::Map::new();
        if let Some(name) = &name {
            diff.insert(
                "name".into(),
                json!({ "before": current.name.as_str(), "after": name.as_str() }),
            );
        }
        if let Some(slug) = &slug {
            diff.insert(
                "slug".into(),
                json!({ "before": current.slug.as_str(), "after": slug.as_str() }),
            );
        }
        let renamed = Heap {
            name: name.unwrap_or_else(|| current.name.clone()),
            slug: slug.unwrap_or_else(|| current.slug.clone()),
            ..current.clone()
        };
        // Counted before the commit, so a saved change is never reported as failed. Renaming doesn't change it.
        let open_count = count_heap_open_items(&txn, heap_id).await?;
        let now = self.now();
        let saved = self
            .save_heap(
                txn,
                principal,
                &renamed,
                expected,
                AuditAction::HeapUpdated,
                diff.into(),
                now,
            )
            .await?;
        tracing::info!(%heap_id, user_id = %principal.user_id, "heap updated");
        Ok(to_heap_view(saved, role, open_count))
    }

    /// Sets the heap's own weights and its parameters' weights in one transaction, if the heap is still at version
    /// `expected`. The heap's version orders saves through here, even those that change only parameter weights, and
    /// each listed parameter must also be at its given version ([`ServiceError::VersionConflict`]). Edits through
    /// the parameter use cases are guarded by each parameter's version alone. A parameter must be one in use of this heap
    /// ([`domain::parameter::ParameterError::NotInHeap`], [`domain::parameter::ParameterError::Archived`]) and
    /// listed once ([`ValidationError::DuplicateParameter`]). An archived heap is read-only
    /// ([`ServiceError::HeapArchived`]). Any failure saves nothing.
    ///
    /// Weights equal to the stored ones are skipped. The heap's version moves on whenever any weight changes, with a
    /// `heap_updated` entry only when one of its own weights changes; each parameter whose weight changes gets a
    /// `parameter_updated` entry and its next version. A request that changes nothing succeeds and writes nothing.
    pub async fn update_weights(
        &self,
        principal: Principal,
        heap_id: HeapId,
        change: WeightsChange,
        expected: Version,
    ) -> Result<HeapWeightsView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::ConfigureHeap).await?;
        let (heap, role) = (access.heap, access.membership.role);
        if heap.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: heap.version,
            });
        }
        let listed: BTreeSet<ParameterId> = change.parameters.iter().map(|p| p.id).collect();
        if listed.len() != change.parameters.len() {
            return Err(ValidationError::DuplicateParameter.into());
        }

        let now = self.now();
        let mut parameters_changed = 0_usize;
        for requested in &change.parameters {
            let current = lock_listed(&txn, heap_id, requested.id, requested.version).await?;
            let patch = ParameterPatch {
                weight: Some(requested.weight),
                ..ParameterPatch::default()
            };
            let Some(edited) = current.apply_patch(&patch)? else {
                continue;
            };
            let diff = diff_parameters(&current, &edited);
            self.write_parameter(
                &txn,
                &heap,
                principal,
                &edited,
                AuditAction::ParameterUpdated,
                diff,
                now,
            )
            .await?;
            parameters_changed += 1;
        }

        let policy = change.heap.apply(&heap.policy);
        let heap_changed = policy != heap.policy;
        let heap = if heap_changed {
            let diff = diff_policies(&heap.policy, &policy);
            let edited = Heap { policy, ..heap.clone() };
            self.write_heap(&txn, principal, &edited, expected, AuditAction::HeapUpdated, diff, now)
                .await?
        } else if parameters_changed > 0 {
            // A save of parameter weights alone moves the heap's version on too, so another weights save made
            // against the policy as it was conflicts. The parameters' own entries record what changed.
            store::heaps::update_heap(&txn, &heap, expected)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?
        } else {
            heap
        };

        let open_count = count_heap_open_items(&txn, heap_id).await?;
        let parameters = list_parameters_in_use(&txn, &heap, role).await?;
        if heap_changed || parameters_changed > 0 {
            txn.commit().await.map_err(StoreError::from)?;
            tracing::info!(%heap_id, user_id = %principal.user_id, heap_changed, parameters_changed, "weights updated");
        }
        Ok(HeapWeightsView {
            heap: to_heap_view(heap, role, open_count),
            parameters,
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
            return load_heap_view(&txn, current, role).await;
        }

        // Counted before the commit, as in `update_heap`. Archiving doesn't change it.
        let open_count = count_heap_open_items(&txn, heap_id).await?;
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
        Ok(to_heap_view(saved, role, open_count))
    }

    /// [`Self::write_heap`], then commits.
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
        let saved = self
            .write_heap(&txn, principal, heap, expected, action, diff, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(saved)
    }

    /// Writes `heap` and its audit entry, without committing. The caller holds the heap lock.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    async fn write_heap(
        &self,
        txn: &store::DatabaseTransaction,
        principal: Principal,
        heap: &Heap,
        expected: Version,
        action: AuditAction,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> Result<Heap, ServiceError> {
        let saved = store::heaps::update_heap(txn, heap, expected)
            .await
            .map_err(|e| handle_taken_on_duplicate(e, Field::Slug))?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let entry = self.heap_audit_entry(principal, &saved, action, diff, now);
        store::audit::insert_audit_entry(txn, &entry).await?;
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
