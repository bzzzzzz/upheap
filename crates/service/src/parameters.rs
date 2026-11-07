//! Custom parameter use cases: listing a heap's parameters, and the admin's changes to them (Req Manage parameters).
//!
//! Every write locks the heap first, like every other heap write, then the parameter, and reads the parameter
//! under its lock. Names are unique among the heap's parameters in use, which the heap lock makes safe to check.
//! Parameters and options are archived, never deleted.

use std::collections::BTreeSet;

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::parameter::{NewParameter, OptionLabel, Parameter, ParameterName, ParameterOption, ParameterPatch, Weight};
use domain::permission::{
    ParameterAction, ParameterListAction, Permission, allowed_parameter_actions, allowed_parameter_list_actions,
};
use domain::team::Role;
use domain::{HeapId, OptionId, ParameterId, TagId, Timestamp, Version};
use serde_json::Value;
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, DatabaseTransaction, StoreError, TransactionTrait};

use crate::access::{authorize_heap, authorize_heap_write};
use crate::audit::{describe_new_parameter, diff_parameters, diff_switched_value};
use crate::items::load_heap_tags;
use crate::{Principal, ServiceError, Services};

/// A parameter with what the caller may do with it.
#[derive(Debug)]
pub struct ParameterEntry {
    pub parameter: Parameter,
    pub allowed_actions: Vec<ParameterAction>,
}

/// A heap's parameters, each list by name.
#[derive(Debug)]
pub struct ParameterList {
    pub heap: Heap,
    pub parameters: Vec<ParameterEntry>,
    pub archived: Vec<ParameterEntry>,
    pub allowed_actions: Vec<ParameterListAction>,
}

/// What a new parameter starts with. The service gives each option its ID.
#[derive(Clone, Debug)]
pub struct ParameterDraft {
    pub name: ParameterName,
    pub weight: Weight,
    /// Highest priority first.
    pub options: Vec<OptionLabel>,
    pub tags: BTreeSet<TagId>,
}

impl Services {
    /// Every parameter of the heap. Any member may read them, since members set their values on items.
    pub async fn list_parameters(&self, principal: Principal, heap_id: HeapId) -> Result<ParameterList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let role = access.membership.role;
        let (archived, in_use): (Vec<_>, Vec<_>) = store::parameters::list_parameters(&self.db, heap_id)
            .await?
            .into_iter()
            .map(|parameter| with_actions(parameter, &access.heap, role))
            .partition(|entry| entry.parameter.is_archived());
        Ok(ParameterList {
            allowed_actions: allowed_parameter_list_actions(&access.heap, role),
            heap: access.heap,
            parameters: in_use,
            archived,
        })
    }

    /// Adds a parameter to the heap. Fails with [`ServiceError::ParameterNameTaken`] if a parameter in use has the
    /// name, ignoring case, and with [`ServiceError::TagNotInHeap`] for a tag of another heap.
    pub async fn create_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        draft: ParameterDraft,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::ConfigureHeap).await?;
        load_heap_tags(&txn, heap_id, &draft.tags).await?;
        ensure_name_free(&txn, heap_id, &draft.name, None).await?;

        let now = self.now();
        let options = draft
            .options
            .into_iter()
            .map(|label| ParameterOption {
                id: OptionId::from_uuid(self.new_id(now)),
                label,
            })
            .collect();
        let fields = NewParameter {
            name: draft.name,
            weight: draft.weight,
            options,
            tags: draft.tags,
        };
        let parameter = Parameter::new(ParameterId::from_uuid(self.new_id(now)), heap_id, fields, now)?;
        store::parameters::insert_parameter(&txn, &parameter).await?;
        let entry = self.parameter_audit_entry(
            &access.heap,
            principal,
            AuditAction::ParameterCreated,
            &parameter,
            describe_new_parameter(&parameter),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, parameter_id = %parameter.id, user_id = %principal.user_id, "parameter created");
        Ok(with_actions(parameter, &access.heap, access.membership.role))
    }

    /// Renames the parameter or changes its weight, if it is still at version `expected`.
    pub async fn update_parameter(
        &self,
        principal: Principal,
        parameter_id: ParameterId,
        patch: ParameterPatch,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, Some(expected)).await?;
        let edited = current.apply_patch(&patch)?;
        if let Some(edited) = &edited
            && edited.name != current.name
        {
            ensure_name_free(&txn, current.heap_id, &edited.name, Some(current.id)).await?;
        }
        self.finish_edit(txn, &heap, role, principal, current, edited).await
    }

    /// Adds an option with `label` as the parameter's lowest-priority option, if it is still at version `expected`.
    pub async fn add_option(
        &self,
        principal: Principal,
        parameter_id: ParameterId,
        label: OptionLabel,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let option = ParameterOption {
            id: OptionId::from_uuid(self.new_id(self.now())),
            label,
        };
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, Some(expected)).await?;
        let edited = current.add_option(option)?;
        self.finish_edit(txn, &heap, role, principal, current, Some(edited))
            .await
    }

    /// Relabels the parameter's options in use and puts them in the order given, highest priority first, if it is
    /// still at version `expected`. Every option in use must be listed exactly once.
    pub async fn arrange_options(
        &self,
        principal: Principal,
        parameter_id: ParameterId,
        options: Vec<ParameterOption>,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, Some(expected)).await?;
        let edited = current.arrange_options(options)?;
        self.finish_edit(txn, &heap, role, principal, current, edited).await
    }

    /// Suggests the parameter for exactly `tags`, tags of its heap, if it is still at version `expected`.
    pub async fn link_parameter_tags(
        &self,
        principal: Principal,
        parameter_id: ParameterId,
        tags: BTreeSet<TagId>,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, Some(expected)).await?;
        load_heap_tags(&txn, current.heap_id, &tags).await?;
        let edited = current.link_tags(tags)?;
        self.finish_edit(txn, &heap, role, principal, current, edited).await
    }

    /// Takes the parameter out of use. Fails with [`ServiceError::ParameterInUse`] while an open item has a value for
    /// it. Archiving an archived parameter succeeds and changes nothing.
    ///
    /// No `If-Match`: the request says what the parameter should end up as, so repeating or racing it is safe.
    pub async fn archive_parameter(
        &self,
        principal: Principal,
        parameter_id: ParameterId,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, None).await?;
        let Some(edited) = current.archive(self.now()) else {
            return Ok(with_actions(current, &heap, role));
        };
        let users = store::parameters::list_open_items_with_value(&txn, current.heap_id, current.id).await?;
        if !users.is_empty() {
            return Err(ServiceError::ParameterInUse {
                parameter_id: current.id,
                items: users.len(),
            });
        }
        self.finish_edit(txn, &heap, role, principal, current, Some(edited))
            .await
    }

    /// Archives an option, and switches every open item that has it to `replacement`, another option of the same
    /// parameter in use ([`domain::parameter::ParameterError::ReplacementRequired`] otherwise). Closed items keep
    /// the archived option. Archiving an archived option succeeds and changes nothing.
    ///
    /// No `If-Match`, as for archiving a parameter. Each switched item's version goes up.
    pub async fn archive_option(
        &self,
        principal: Principal,
        option_id: OptionId,
        replacement: Option<OptionId>,
    ) -> Result<ParameterEntry, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let parameter_id = store::parameters::find_option_parameter(&txn, option_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let (heap, role, current) = lock_for_write(&txn, principal, parameter_id, None).await?;
        let now = self.now();
        // An option is only archived with a replacement, so a change always comes with one.
        let (Some(edited), Some(replaced_by)) = (current.archive_option(option_id, replacement, now)?, replacement)
        else {
            return Ok(with_actions(current, &heap, role));
        };

        tracing::info!(heap_id = %heap.id, %parameter_id, %option_id, %replaced_by, "archiving option");
        let switched = store::parameters::switch_open_items(&txn, heap.id, option_id, replaced_by).await?;
        // Each switched item gets an entry of its own too, so its history accounts for every version.
        let item_entries: Vec<NewAuditEntry> = switched
            .iter()
            .map(|item_id| NewAuditEntry {
                id: self.new_id(now),
                team_id: heap.team_id,
                heap_id: Some(heap.id),
                actor: principal.user_id,
                action: AuditAction::ItemUpdated,
                entity_type: EntityType::Item,
                entity_id: *item_id.as_uuid(),
                diff: diff_switched_value(parameter_id, option_id, replaced_by),
                created_at: now,
            })
            .collect();
        store::audit::insert_audit_entries(&txn, &item_entries).await?;
        let mut diff = diff_parameters(&current, &edited);
        if let Value::Object(fields) = &mut diff {
            let ids: Vec<String> = switched.iter().map(ToString::to_string).collect();
            fields.insert("items_switched".into(), serde_json::json!(ids));
        }
        let saved = self
            .save_parameter(txn, &heap, principal, &edited, AuditAction::OptionArchived, diff, now)
            .await?;
        tracing::info!(heap_id = %heap.id, %parameter_id, %option_id, items_switched = switched.len(), user_id = %principal.user_id, "option archived");
        Ok(with_actions(saved, &heap, role))
    }

    /// Saves `edited`, the edit of `current`, with a `parameter_updated` or `parameter_archived` entry and commits.
    /// `None` means nothing changes: `current` comes back as it is, without an entry.
    async fn finish_edit(
        &self,
        txn: DatabaseTransaction,
        heap: &Heap,
        role: Role,
        principal: Principal,
        current: Parameter,
        edited: Option<Parameter>,
    ) -> Result<ParameterEntry, ServiceError> {
        let Some(edited) = edited else {
            return Ok(with_actions(current, heap, role));
        };
        let action = if edited.is_archived() && !current.is_archived() {
            AuditAction::ParameterArchived
        } else {
            AuditAction::ParameterUpdated
        };
        let diff = diff_parameters(&current, &edited);
        let saved = self
            .save_parameter(txn, heap, principal, &edited, action, diff, self.now())
            .await?;
        tracing::info!(heap_id = %heap.id, parameter_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), action = action.as_str(), "parameter changed");
        Ok(with_actions(saved, heap, role))
    }

    /// Writes `parameter` over the version it was read at, records `action`, and commits. The caller holds the
    /// heap and parameter locks, so the version can't have moved.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    async fn save_parameter(
        &self,
        txn: DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        parameter: &Parameter,
        action: AuditAction,
        diff: Value,
        now: Timestamp,
    ) -> Result<Parameter, ServiceError> {
        let saved = store::parameters::update_parameter(&txn, parameter, parameter.version, now)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected: parameter.version,
                current: parameter.version.next(),
            })?;
        let entry = self.parameter_audit_entry(heap, principal, action, &saved, diff, now);
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(saved)
    }

    fn parameter_audit_entry(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        parameter: &Parameter,
        diff: Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::Parameter,
            entity_id: *parameter.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

fn with_actions(parameter: Parameter, heap: &Heap, role: Role) -> ParameterEntry {
    ParameterEntry {
        allowed_actions: allowed_parameter_actions(&parameter, heap, role),
        parameter,
    }
}

/// Finds the parameter's heap, authorizes configuring it and locks the heap, then reads the parameter under its own
/// lock, for the same reason as saved views (see `crate::saved_views`), and checks it is at version `expected` when
/// given.
async fn lock_for_write(
    txn: &DatabaseTransaction,
    principal: Principal,
    parameter_id: ParameterId,
    expected: Option<Version>,
) -> Result<(Heap, Role, Parameter), ServiceError> {
    let found = store::parameters::find_parameter(txn, parameter_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let access = authorize_heap_write(txn, principal, found.heap_id, Permission::ConfigureHeap).await?;
    let current = store::parameters::lock_parameter(txn, parameter_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if let Some(expected) = expected
        && current.version != expected
    {
        return Err(ServiceError::VersionConflict {
            expected,
            current: current.version,
        });
    }
    Ok((access.heap, access.membership.role, current))
}

/// Refuses a name another parameter of the heap in use has, ignoring case. `except` is the one being renamed.
async fn ensure_name_free(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    name: &ParameterName,
    except: Option<ParameterId>,
) -> Result<(), ServiceError> {
    let key = name.to_key();
    let parameters = store::parameters::list_parameters_locking(db, heap_id).await?;
    let taken = parameters
        .iter()
        .any(|p| !p.is_archived() && Some(p.id) != except && p.name.to_key() == key);
    if taken {
        return Err(ServiceError::ParameterNameTaken {
            name: name.as_str().to_owned(),
        });
    }
    Ok(())
}
