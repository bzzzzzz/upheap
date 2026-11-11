//! Heap use cases: creating, renaming, archiving and unarchiving. Heaps are never deleted.
//!
//! Every write locks the heap row, like item and tag writes do, so archiving waits for writes in flight and writes
//! that come after it see the heap archived ([`crate::access::authorize_heap_write`]).

use domain::audit::{AuditAction, EntityType};
use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy, HeapPolicyPatch};
use domain::parameter::{ParameterPatch, Weight};
use domain::permission::{Permission, allowed_heap_actions};
use domain::text::{Context, Name};
use domain::turn::TakeBackWhy;
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

/// A change to a heap's name, slug, context and prepare setting. `None` leaves a field as it is; `Some(None)` clears
/// the context.
#[derive(Clone, Debug, Default)]
pub struct HeapPatch {
    pub name: Option<Name>,
    pub slug: Option<Handle>,
    pub context: Option<Option<Context>>,
    /// Whether agents prepare every new note (Work with agents, Opening a turn).
    pub agents_prepare: Option<bool>,
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
            context: None,
            agents_prepare: false,
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

    /// Changes the heap's name, slug, context or prepare setting if it is still at version `expected`. The name and
    /// slug need [`Permission::ManageHeaps`], the prepare setting [`Permission::ConfigureHeap`] and the context
    /// [`Permission::EditItems`]; a patch that sets a field the caller may not change fails with [`ServiceError::Forbidden`] and saves nothing. An archived heap is read-only, so
    /// this fails with [`ServiceError::HeapArchived`]. A patch that changes nothing succeeds without an audit entry
    /// or a new version. A save moves the version once; a name, slug or prepare change writes `heap_updated` and a context
    /// change `heap_context_updated`, both when both change. A slug another heap of the team has fails with
    /// [`ServiceError::HandleTaken`].
    pub async fn update_heap(
        &self,
        principal: Principal,
        heap_id: HeapId,
        patch: HeapPatch,
        expected: Version,
    ) -> Result<HeapView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap(&txn, principal, heap_id, Permission::ViewHeap).await?;
        let role = access.membership.role;
        let renames = patch.name.is_some() || patch.slug.is_some();
        if renames || (patch.context.is_none() && patch.agents_prepare.is_none()) {
            require(role, Permission::ManageHeaps)?;
        }
        if patch.agents_prepare.is_some() {
            require(role, Permission::ConfigureHeap)?;
        }
        if patch.context.is_some() {
            require(role, Permission::EditItems)?;
        }
        let current = lock(&txn, heap_id).await?;
        ensure_active(&current)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        let name = patch.name.filter(|name| *name != current.name);
        let slug = patch.slug.filter(|slug| *slug != current.slug);
        let context = patch.context.filter(|context| *context != current.context);
        let agents_prepare = patch
            .agents_prepare
            .filter(|prepare| *prepare != current.agents_prepare);
        if name.is_none() && slug.is_none() && context.is_none() && agents_prepare.is_none() {
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
        if let Some(after) = agents_prepare {
            diff.insert(
                "agents_prepare".into(),
                json!({ "before": current.agents_prepare, "after": after }),
            );
        }
        let context_diff = context.as_ref().map(|after| {
            json!({ "context": {
                "before": current.context.as_ref().map(Context::as_str),
                "after": after.as_ref().map(Context::as_str),
            } })
        });
        let edited = Heap {
            name: name.unwrap_or_else(|| current.name.clone()),
            slug: slug.unwrap_or_else(|| current.slug.clone()),
            context: context.unwrap_or_else(|| current.context.clone()),
            agents_prepare: agents_prepare.unwrap_or(current.agents_prepare),
            ..current.clone()
        };
        // Counted before the commit, so a saved change is never reported as failed. Renaming doesn't change it.
        let open_count = count_heap_open_items(&txn, heap_id).await?;
        let now = self.now();
        // The heap's row is written once, with the first entry; a second entry for the context joins it.
        let (saved, context_diff) = if diff.is_empty() {
            let Some(context_diff) = context_diff else {
                return load_heap_view(&txn, current, role).await;
            };
            let saved = self
                .write_heap(
                    &txn,
                    principal,
                    &edited,
                    expected,
                    AuditAction::HeapContextUpdated,
                    context_diff,
                    now,
                )
                .await?;
            (saved, None)
        } else {
            let saved = self
                .write_heap(
                    &txn,
                    principal,
                    &edited,
                    expected,
                    AuditAction::HeapUpdated,
                    diff.into(),
                    now,
                )
                .await?;
            (saved, context_diff)
        };
        if let Some(context_diff) = context_diff {
            let entry = self.heap_audit_entry(principal, &saved, AuditAction::HeapContextUpdated, context_diff, now);
            store::audit::insert_audit_entry(&txn, &entry).await?;
        }
        txn.commit().await.map_err(StoreError::from)?;
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
        // Archiving takes back every open turn of the heap, one entry each: nothing is done in an archived heap, so
        // no agent keeps an item in it. Unarchiving opens nothing.
        let mut turns_taken_back = 0_usize;
        if archived {
            for turn in store::turns::list_open_turns_in_heap_locking(&txn, heap_id).await? {
                if self
                    .take_back_turn(&txn, &current, principal, &turn, TakeBackWhy::HeapArchived, now)
                    .await?
                {
                    turns_taken_back += 1;
                }
            }
        }
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
        tracing::info!(%heap_id, user_id = %principal.user_id, archived, turns_taken_back, "heap archive state changed");
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
            run_id: None,
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

/// `Forbidden` unless `role` has `permission`.
fn require(role: domain::team::Role, permission: Permission) -> Result<(), ServiceError> {
    if permission.is_granted_to(role) {
        Ok(())
    } else {
        Err(ServiceError::Forbidden(permission))
    }
}
