//! Custom parameter use cases (product.md, Manage parameters): listing a heap's parameters, and the admin's changes
//! to them.
//!
//! Every write locks the heap first, like every other heap write, then the parameter, and reads the parameter
//! under its lock. Names are unique among the heap's parameters in use, which the heap lock makes safe to check.
//! Parameters and options are archived, never deleted.

use std::collections::{BTreeMap, BTreeSet};

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::heap::Heap;
use domain::parameter::{
    NewParameter, OptionLabel, Parameter, ParameterError, ParameterName, ParameterOption, ParameterOwner,
    ParameterPatch, Weight,
};
use domain::permission::{
    ParameterAction, ParameterListAction, Permission, allowed_parameter_actions, allowed_parameter_list_actions,
    allowed_team_parameter_actions, allowed_team_parameter_list_actions,
};
use domain::team::Role;
use domain::{HeapId, OptionId, ParameterId, TagId, TeamId, Timestamp, Version};
use serde_json::Value;
use store::audit::NewAuditEntry;
use store::parameters::HeapUse;
use store::{ConnectionTrait, DatabaseTransaction, StoreError, TransactionTrait};

use crate::access::{
    authorize_heap, authorize_heap_write, authorize_heap_write_under_team, authorize_team, begin_team_write,
    lower_team_heap,
};
use crate::audit::{describe_new_parameter, diff_heap_use, diff_parameters, diff_switched_value};
use crate::items::{ensure_tags_in_use, load_heap_tags};
use crate::{Principal, ServiceError, Services};

/// A parameter with what the caller may do with it.
#[derive(Debug)]
pub struct ParameterEntry {
    pub parameter: Parameter,
    pub allowed_actions: Vec<ParameterAction>,
}

/// A team's parameter with the heaps of the team that use it now.
#[derive(Debug)]
pub struct TeamParameterEntry {
    pub team_id: TeamId,
    pub parameter: Parameter,
    /// By name.
    pub heaps: Vec<Heap>,
    pub allowed_actions: Vec<ParameterAction>,
}

/// A heap's parameters, each list by name: its own and the team parameters it uses together, and the team's other
/// parameters in use apart.
#[derive(Debug)]
pub struct ParameterList {
    pub heap: Heap,
    pub parameters: Vec<ParameterEntry>,
    pub archived: Vec<ParameterEntry>,
    pub team_available: Vec<TeamParameterEntry>,
    pub allowed_actions: Vec<ParameterListAction>,
}

/// A team's parameters, each list by name.
#[derive(Debug)]
pub struct TeamParameterList {
    pub parameters: Vec<TeamParameterEntry>,
    pub archived: Vec<TeamParameterEntry>,
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

/// What a new team parameter starts with. It has no weight: each heap weights it for itself.
#[derive(Clone, Debug)]
pub struct TeamParameterDraft {
    pub name: ParameterName,
    /// Highest priority first.
    pub options: Vec<OptionLabel>,
}

/// Whose parameter a write is about: a heap's, or a team's.
#[derive(Clone, Copy)]
enum ParameterScope {
    Heap(HeapId),
    Team(TeamId),
}

/// What a write holds once its locks are taken.
enum HeldScope {
    Heap(Heap),
    /// Every heap of the team, locked.
    Team {
        team_id: TeamId,
        heaps: Vec<Heap>,
    },
}

/// A write in progress: its transaction with the locks, and the parameter as read under its own lock.
struct Held {
    txn: DatabaseTransaction,
    role: Role,
    scope: HeldScope,
    current: Parameter,
}

impl Held {
    const fn team_id(&self) -> TeamId {
        match &self.scope {
            HeldScope::Heap(heap) => heap.team_id,
            HeldScope::Team { team_id, .. } => *team_id,
        }
    }

    /// The heaps the write reaches: the heap, or every heap of the team.
    fn heaps(&self) -> &[Heap] {
        match &self.scope {
            HeldScope::Heap(heap) => std::slice::from_ref(heap),
            HeldScope::Team { heaps, .. } => heaps,
        }
    }

    /// The heap whose log records the write: a team parameter's is in the team's log, with no heap.
    fn audit_heap_id(&self) -> Option<HeapId> {
        match &self.scope {
            HeldScope::Heap(heap) => Some(heap.id),
            HeldScope::Team { .. } => None,
        }
    }

    fn entry(&self, parameter: Parameter) -> ParameterEntry {
        match &self.scope {
            HeldScope::Heap(heap) => with_actions(parameter, heap, self.role),
            HeldScope::Team { .. } => with_team_actions(parameter, self.role),
        }
    }
}

impl Services {
    /// Every parameter of the heap: its own, the team parameters it uses among them, and the team's other parameters
    /// apart. Any member may read them, since members set their values on items.
    pub async fn list_parameters(&self, principal: Principal, heap_id: HeapId) -> Result<ParameterList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        load_parameter_list(&self.db, access.heap, access.membership.role).await
    }

    /// Adds a parameter to the heap. Fails with [`ServiceError::ParameterNameTaken`] if a parameter in use of the
    /// heap or of its team has the name, ignoring case, and with [`ServiceError::TagNotInHeap`] for a tag of another
    /// heap.
    pub async fn create_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        draft: ParameterDraft,
    ) -> Result<ParameterEntry, ServiceError> {
        let lower = lower_team_heap(&self.db, heap_id).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access =
            authorize_heap_write_under_team(&txn, principal, heap_id, lower, Permission::ConfigureHeap).await?;
        load_heap_tags(&txn, heap_id, &draft.tags).await?;
        ensure_name_free(&txn, access.heap.team_id, &[heap_id], &draft.name, None).await?;
        ensure_tags_in_use(&txn, &draft.tags).await?;

        let now = self.now();
        let options = self.new_options(draft.options, now);
        let fields = NewParameter {
            name: draft.name,
            weight: draft.weight,
            options,
            tags: draft.tags,
        };
        let id = ParameterId::from_uuid(self.new_id(now));
        let parameter = Parameter::new(id, ParameterOwner::Heap(heap_id), fields, now)?;
        store::parameters::insert_parameter(&txn, &parameter).await?;
        let entry = self.parameter_audit_entry(
            access.heap.team_id,
            Some(heap_id),
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

    /// Renames the parameter or changes its weight, if it is still at version `expected`. A parameter of the heap's
    /// team is [`ServiceError::NotFound`] here: the heap's weight for one is saved with the heap's weights.
    pub async fn update_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        patch: ParameterPatch,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let lower = if patch.name.is_some() {
            lower_team_heap(&self.db, heap_id).await?
        } else {
            None
        };
        let scope = ParameterScope::Heap(heap_id);
        let held = self
            .lock_for_write(principal, scope, parameter_id, Some(expected), lower)
            .await?;
        self.update_name_or_weight(held, principal, &patch).await
    }

    /// Adds an option with `label` as the parameter's lowest-priority option, if it is still at version `expected`.
    pub async fn add_option(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        label: OptionLabel,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        self.add_option_in(principal, ParameterScope::Heap(heap_id), parameter_id, label, expected)
            .await
    }

    /// Relabels the parameter's options in use and puts them in the order given, highest priority first, if it is
    /// still at version `expected`. Every option in use must be listed exactly once.
    pub async fn arrange_options(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        options: Vec<ParameterOption>,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        self.arrange_options_in(
            principal,
            ParameterScope::Heap(heap_id),
            parameter_id,
            options,
            expected,
        )
        .await
    }

    /// Suggests the parameter for exactly `tags`, tags of its heap, if it is still at version `expected`.
    pub async fn link_parameter_tags(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        tags: BTreeSet<TagId>,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let held = self
            .lock_for_write(
                principal,
                ParameterScope::Heap(heap_id),
                parameter_id,
                Some(expected),
                None,
            )
            .await?;
        load_heap_tags(&held.txn, heap_id, &tags).await?;
        ensure_tags_in_use(&held.txn, &tags).await?;
        let edited = held.current.link_tags(tags)?;
        self.finish_edit(held, principal, edited).await
    }

    /// Takes the parameter out of use. Fails with [`ServiceError::ParameterInUse`] while an open item has a value for
    /// it. Archiving an archived parameter succeeds and changes nothing.
    ///
    /// No `If-Match`: the request says what the parameter should end up as, so repeating or racing it is safe.
    pub async fn archive_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
    ) -> Result<ParameterEntry, ServiceError> {
        self.archive_parameter_in(principal, ParameterScope::Heap(heap_id), parameter_id)
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
        heap_id: HeapId,
        option_id: OptionId,
        replacement: Option<OptionId>,
    ) -> Result<ParameterEntry, ServiceError> {
        self.archive_option_in(principal, ParameterScope::Heap(heap_id), option_id, replacement)
            .await
    }

    /// Every parameter of the team, by name. Any member may read them.
    pub async fn list_team_parameters(
        &self,
        principal: Principal,
        team_id: TeamId,
    ) -> Result<TeamParameterList, ServiceError> {
        let role = authorize_team(&self.db, principal, team_id, Permission::ViewTeam)
            .await?
            .role;
        let parameters = store::parameters::list_team_parameters(&self.db, team_id).await?;
        let (archived, in_use): (Vec<_>, Vec<_>) = team_parameter_entries(&self.db, team_id, parameters, role)
            .await?
            .into_iter()
            .partition(|entry| entry.parameter.is_archived());
        Ok(TeamParameterList {
            parameters: in_use,
            archived,
            allowed_actions: allowed_team_parameter_list_actions(role),
        })
    }

    /// Adds a parameter to the team, for its heaps to use. Fails with [`ServiceError::ParameterNameTaken`] if a
    /// parameter in use of the team or of any of its heaps has the name, ignoring case.
    pub async fn create_team_parameter(
        &self,
        principal: Principal,
        team_id: TeamId,
        draft: TeamParameterDraft,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let (txn, membership, heaps) =
            begin_team_write(&self.db, principal, team_id, Permission::ConfigureHeap).await?;
        let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
        ensure_name_free(&txn, team_id, &heap_ids, &draft.name, None).await?;

        let now = self.now();
        let options = self.new_options(draft.options, now);
        let fields = NewParameter {
            name: draft.name,
            weight: Weight::OFF,
            options,
            tags: BTreeSet::new(),
        };
        let id = ParameterId::from_uuid(self.new_id(now));
        let parameter = Parameter::new(id, ParameterOwner::Team(team_id), fields, now)?;
        store::parameters::insert_parameter(&txn, &parameter).await?;
        let entry = self.parameter_audit_entry(
            team_id,
            None,
            principal,
            AuditAction::ParameterCreated,
            &parameter,
            describe_new_parameter(&parameter),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, parameter_id = %parameter.id, user_id = %principal.user_id, "team parameter created");
        let allowed_actions = allowed_team_parameter_actions(&parameter, membership.role);
        Ok(TeamParameterEntry {
            team_id,
            parameter,
            heaps: Vec::new(),
            allowed_actions,
        })
    }

    /// Renames a team's parameter, if it is still at version `expected`.
    pub async fn update_team_parameter(
        &self,
        principal: Principal,
        team_id: TeamId,
        parameter_id: ParameterId,
        name: ParameterName,
        expected: Version,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let held = self
            .lock_for_write(
                principal,
                ParameterScope::Team(team_id),
                parameter_id,
                Some(expected),
                None,
            )
            .await?;
        let patch = ParameterPatch {
            name: Some(name),
            weight: None,
        };
        let entry = self.update_name_or_weight(held, principal, &patch).await?;
        self.with_heaps(team_id, entry).await
    }

    /// [`Self::add_option`] for a team's parameter.
    pub async fn add_team_option(
        &self,
        principal: Principal,
        team_id: TeamId,
        parameter_id: ParameterId,
        label: OptionLabel,
        expected: Version,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let entry = self
            .add_option_in(principal, ParameterScope::Team(team_id), parameter_id, label, expected)
            .await?;
        self.with_heaps(team_id, entry).await
    }

    /// [`Self::arrange_options`] for a team's parameter.
    pub async fn arrange_team_options(
        &self,
        principal: Principal,
        team_id: TeamId,
        parameter_id: ParameterId,
        options: Vec<ParameterOption>,
        expected: Version,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let entry = self
            .arrange_options_in(
                principal,
                ParameterScope::Team(team_id),
                parameter_id,
                options,
                expected,
            )
            .await?;
        self.with_heaps(team_id, entry).await
    }

    /// [`Self::archive_parameter`] for a team's parameter: refused with [`ServiceError::ParameterInUse`] while an
    /// open item in any heap of the team has a value for it, and the count is the total.
    pub async fn archive_team_parameter(
        &self,
        principal: Principal,
        team_id: TeamId,
        parameter_id: ParameterId,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let entry = self
            .archive_parameter_in(principal, ParameterScope::Team(team_id), parameter_id)
            .await?;
        self.with_heaps(team_id, entry).await
    }

    /// [`Self::archive_option`] for a team's parameter: open items in every heap of the team, archived heaps
    /// included, switch to the replacement.
    pub async fn archive_team_option(
        &self,
        principal: Principal,
        team_id: TeamId,
        option_id: OptionId,
        replacement: Option<OptionId>,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let entry = self
            .archive_option_in(principal, ParameterScope::Team(team_id), option_id, replacement)
            .await?;
        self.with_heaps(team_id, entry).await
    }

    /// Makes the heap use a parameter of its team, and returns the heap's parameters. The first use gives the parameter
    /// the default weight; using it again after stopping restores the weight it had. A parameter the heap uses
    /// already succeeds and changes nothing. An archived parameter ([`ParameterError::Archived`]), a parameter that
    /// isn't a parameter of the heap's team ([`ServiceError::NotFound`]) and an archived heap are refused.
    ///
    /// No `If-Match`, as for archiving: the request says what the use should end up as.
    pub async fn use_team_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
    ) -> Result<ParameterList, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::ConfigureHeap).await?;
        let parameter = lock_team_parameter_of(&txn, &access.heap, parameter_id).await?;
        if parameter.is_archived() {
            return Err(ParameterError::Archived { parameter_id }.into());
        }
        let now = self.now();
        let first = HeapUse {
            heap_id,
            parameter_id,
            weight: Weight::DEFAULT,
            stopped_at: Some(now),
        };
        // Written before it is read, stopped (see `store::parameters::ensure_heap_use`), so a first use starts at the
        // default weight and its entry says only that the heap now uses it.
        store::parameters::ensure_heap_use(&txn, &first, now).await?;
        let before = store::parameters::lock_heap_use(&txn, heap_id, parameter_id)
            .await?
            .ok_or_else(|| {
                StoreError::corrupt(
                    "heap_parameters",
                    &parameter_id.to_string(),
                    "a use row just written is gone",
                )
            })?;
        if before.stopped_at.is_none() {
            return load_parameter_list(&txn, access.heap, access.membership.role).await;
        }
        let after = HeapUse {
            stopped_at: None,
            ..before
        };
        store::parameters::update_heap_use(&txn, &after).await?;
        self.record_heap_use(&txn, &access.heap, principal, &parameter, Some(&before), &after, now)
            .await?;
        let list = load_parameter_list(&txn, access.heap, access.membership.role).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(%heap_id, %parameter_id, user_id = %principal.user_id, "team parameter used");
        Ok(list)
    }

    /// Makes the heap stop using a team parameter, and returns the heap's parameters. Refused with
    /// [`ServiceError::ParameterInUse`] while an open item of the heap has a value for it. A parameter the heap
    /// doesn't use succeeds and changes nothing. Its weight is kept for when the heap uses it again, and closed
    /// items keep their values, which show as for an archived parameter.
    pub async fn stop_using_team_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
    ) -> Result<ParameterList, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::ConfigureHeap).await?;
        let parameter = lock_team_parameter_of(&txn, &access.heap, parameter_id).await?;
        let Some(before) = store::parameters::lock_heap_use(&txn, heap_id, parameter_id)
            .await?
            .filter(|heap_use| heap_use.stopped_at.is_none())
        else {
            return load_parameter_list(&txn, access.heap, access.membership.role).await;
        };
        let items = store::parameters::list_open_items_with_value(&txn, heap_id, parameter_id)
            .await?
            .len();
        if items > 0 {
            return Err(ServiceError::ParameterInUse { parameter_id, items });
        }
        let now = self.now();
        let after = HeapUse {
            stopped_at: Some(now),
            ..before
        };
        store::parameters::update_heap_use(&txn, &after).await?;
        self.record_heap_use(&txn, &access.heap, principal, &parameter, Some(&before), &after, now)
            .await?;
        let list = load_parameter_list(&txn, access.heap, access.membership.role).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(%heap_id, %parameter_id, user_id = %principal.user_id, "team parameter stopped");
        Ok(list)
    }

    /// Saves a new weight for a team parameter in `heap`, from the heap's weights save, and records it in the heap's
    /// log. `current` is the parameter as the heap sees it, with the old weight, and `edited` has the new one. The
    /// team parameter's own row, and so its version, doesn't change. The caller holds the heap's lock.
    pub(crate) async fn write_heap_weight(
        &self,
        txn: &DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        current: &Parameter,
        edited: &Parameter,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let before = store::parameters::lock_heap_use(txn, heap.id, current.id)
            .await?
            .ok_or(ParameterError::NotInHeap {
                parameter_id: current.id,
            })?;
        let after = HeapUse {
            weight: edited.weight,
            ..before
        };
        store::parameters::update_heap_use(txn, &after).await?;
        self.record_heap_use(txn, heap, principal, current, Some(&before), &after, now)
            .await
    }

    /// Records a heap's use of a team parameter, or a new weight for it, in the heap's log.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    async fn record_heap_use(
        &self,
        txn: &DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        parameter: &Parameter,
        before: Option<&HeapUse>,
        after: &HeapUse,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let entry = self.parameter_audit_entry(
            heap.team_id,
            Some(heap.id),
            principal,
            AuditAction::ParameterUpdated,
            parameter,
            diff_heap_use(before, after),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
    }

    fn new_options(&self, labels: Vec<OptionLabel>, now: Timestamp) -> Vec<ParameterOption> {
        labels
            .into_iter()
            .map(|label| ParameterOption {
                id: OptionId::from_uuid(self.new_id(now)),
                label,
            })
            .collect()
    }

    /// Adds the heaps that use `entry`'s team parameter, after the write that made it.
    async fn with_heaps(&self, team_id: TeamId, entry: ParameterEntry) -> Result<TeamParameterEntry, ServiceError> {
        let ParameterEntry {
            parameter,
            allowed_actions,
        } = entry;
        let heaps = heaps_using(&self.db, team_id, &[parameter.id])
            .await?
            .remove(&parameter.id)
            .unwrap_or_default();
        Ok(TeamParameterEntry {
            team_id,
            parameter,
            heaps,
            allowed_actions,
        })
    }

    async fn update_name_or_weight(
        &self,
        held: Held,
        principal: Principal,
        patch: &ParameterPatch,
    ) -> Result<ParameterEntry, ServiceError> {
        let edited = held.current.apply_patch(patch)?;
        if let Some(edited) = &edited
            && edited.name != held.current.name
        {
            let heap_ids: Vec<HeapId> = held.heaps().iter().map(|heap| heap.id).collect();
            ensure_name_free(
                &held.txn,
                held.team_id(),
                &heap_ids,
                &edited.name,
                Some(held.current.id),
            )
            .await?;
        }
        self.finish_edit(held, principal, edited).await
    }

    async fn add_option_in(
        &self,
        principal: Principal,
        scope: ParameterScope,
        parameter_id: ParameterId,
        label: OptionLabel,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let option = ParameterOption {
            id: OptionId::from_uuid(self.new_id(self.now())),
            label,
        };
        let held = self
            .lock_for_write(principal, scope, parameter_id, Some(expected), None)
            .await?;
        let edited = held.current.add_option(option)?;
        self.finish_edit(held, principal, Some(edited)).await
    }

    async fn arrange_options_in(
        &self,
        principal: Principal,
        scope: ParameterScope,
        parameter_id: ParameterId,
        options: Vec<ParameterOption>,
        expected: Version,
    ) -> Result<ParameterEntry, ServiceError> {
        let held = self
            .lock_for_write(principal, scope, parameter_id, Some(expected), None)
            .await?;
        let edited = held.current.arrange_options(options)?;
        self.finish_edit(held, principal, edited).await
    }

    async fn archive_parameter_in(
        &self,
        principal: Principal,
        scope: ParameterScope,
        parameter_id: ParameterId,
    ) -> Result<ParameterEntry, ServiceError> {
        let held = self.lock_for_write(principal, scope, parameter_id, None, None).await?;
        let Some(edited) = held.current.archive(self.now()) else {
            let entry = held.entry(held.current.clone());
            return Ok(entry);
        };
        let mut items = 0;
        for heap in held.heaps() {
            items += store::parameters::list_open_items_with_value(&held.txn, heap.id, parameter_id)
                .await?
                .len();
        }
        if items > 0 {
            return Err(ServiceError::ParameterInUse { parameter_id, items });
        }
        self.finish_edit(held, principal, Some(edited)).await
    }

    async fn archive_option_in(
        &self,
        principal: Principal,
        scope: ParameterScope,
        option_id: OptionId,
        replacement: Option<OptionId>,
    ) -> Result<ParameterEntry, ServiceError> {
        // An option never changes its parameter, so it is read before the locks.
        let parameter_id = store::parameters::find_option_parameter(&self.db, option_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let held = self.lock_for_write(principal, scope, parameter_id, None, None).await?;
        let now = self.now();
        // An option is only archived with a replacement, so a change always comes with one.
        let (Some(edited), Some(replaced_by)) =
            (held.current.archive_option(option_id, replacement, now)?, replacement)
        else {
            let entry = held.entry(held.current.clone());
            return Ok(entry);
        };

        let team_id = held.team_id();
        tracing::info!(%team_id, %parameter_id, %option_id, %replaced_by, "archiving option");
        let mut switched = Vec::new();
        for heap in held.heaps() {
            let items = store::parameters::switch_open_items(&held.txn, heap.id, option_id, replaced_by).await?;
            // Each switched item gets an entry of its own too, so its history accounts for every version.
            let item_entries: Vec<NewAuditEntry> = items
                .iter()
                .map(|item_id| NewAuditEntry {
                    id: self.new_id(now),
                    team_id,
                    heap_id: Some(heap.id),
                    actor: principal.user_id,
                    action: AuditAction::ItemUpdated,
                    entity_type: EntityType::Item,
                    entity_id: *item_id.as_uuid(),
                    diff: diff_switched_value(parameter_id, option_id, replaced_by),
                    run_id: None,
                    created_at: now,
                })
                .collect();
            store::audit::insert_audit_entries(&held.txn, &item_entries).await?;
            switched.extend(items);
        }
        let mut diff = diff_parameters(&held.current, &edited);
        if let Value::Object(fields) = &mut diff {
            let ids: Vec<String> = switched.iter().map(ToString::to_string).collect();
            fields.insert(AuditField::ItemsSwitched.code().into(), serde_json::json!(ids));
        }
        let entry = self
            .save_parameter(held, principal, &edited, AuditAction::OptionArchived, diff, now)
            .await?;
        tracing::info!(%team_id, %parameter_id, %option_id, items_switched = switched.len(), user_id = %principal.user_id, "option archived");
        Ok(entry)
    }

    /// Saves `edited`, the edit of the held parameter, with a `parameter_updated` or `parameter_archived` entry and
    /// commits. `None` means nothing changes: the parameter comes back as it is, without an entry.
    async fn finish_edit(
        &self,
        held: Held,
        principal: Principal,
        edited: Option<Parameter>,
    ) -> Result<ParameterEntry, ServiceError> {
        let Some(edited) = edited else {
            let entry = held.entry(held.current.clone());
            return Ok(entry);
        };
        let action = if edited.is_archived() && !held.current.is_archived() {
            AuditAction::ParameterArchived
        } else {
            AuditAction::ParameterUpdated
        };
        let diff = diff_parameters(&held.current, &edited);
        let team_id = held.team_id();
        let entry = self
            .save_parameter(held, principal, &edited, action, diff, self.now())
            .await?;
        tracing::info!(%team_id, parameter_id = %entry.parameter.id, user_id = %principal.user_id, version = entry.parameter.version.get(), action = action.code(), "parameter changed");
        Ok(entry)
    }

    /// Writes `parameter` over the version it was read at, records `action` where the held scope's log is, and
    /// commits. The held locks mean the version can't have moved.
    async fn save_parameter(
        &self,
        held: Held,
        principal: Principal,
        parameter: &Parameter,
        action: AuditAction,
        diff: Value,
        now: Timestamp,
    ) -> Result<ParameterEntry, ServiceError> {
        let saved = self
            .write_parameter(
                &held.txn,
                held.team_id(),
                held.audit_heap_id(),
                principal,
                parameter,
                action,
                diff,
                now,
            )
            .await?;
        let result = held.entry(saved);
        held.txn.commit().await.map_err(StoreError::from)?;
        Ok(result)
    }

    /// Writes `parameter` over the version it was read at and records `action` in the log of `heap_id`, or of the team
    /// when it is `None`, without committing. The caller holds the locks, so the version can't have moved.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    pub(crate) async fn write_parameter(
        &self,
        txn: &DatabaseTransaction,
        team_id: TeamId,
        heap_id: Option<HeapId>,
        principal: Principal,
        parameter: &Parameter,
        action: AuditAction,
        diff: Value,
        now: Timestamp,
    ) -> Result<Parameter, ServiceError> {
        let saved = store::parameters::update_parameter(txn, parameter, parameter.version, now)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected: parameter.version,
                current: parameter.version.next(),
            })?;
        let entry = self.parameter_audit_entry(team_id, heap_id, principal, action, &saved, diff, now);
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(saved)
    }

    /// Takes the locks a write to a parameter of `scope` needs, then reads the parameter under its own lock, for the
    /// same reason as saved views (see `crate::saved_views`), and checks it is at version `expected` when given.
    ///
    /// A heap write locks the heap, after the shared lock of `lower`, the team's lowest heap, for a rename (see
    /// [`authorize_heap_write_under_team`]); a team write locks every heap of the team. A parameter the scope doesn't
    /// own, a team's seen from a heap or the other way round, is [`ServiceError::NotFound`].
    async fn lock_for_write(
        &self,
        principal: Principal,
        scope: ParameterScope,
        parameter_id: ParameterId,
        expected: Option<Version>,
        lower: Option<HeapId>,
    ) -> Result<Held, ServiceError> {
        let (txn, role, scope, owner) = match scope {
            ParameterScope::Heap(heap_id) => {
                let txn = self.db.begin().await.map_err(StoreError::from)?;
                let access =
                    authorize_heap_write_under_team(&txn, principal, heap_id, lower, Permission::ConfigureHeap).await?;
                (
                    txn,
                    access.membership.role,
                    HeldScope::Heap(access.heap),
                    ParameterOwner::Heap(heap_id),
                )
            }
            ParameterScope::Team(team_id) => {
                let (txn, membership, heaps) =
                    begin_team_write(&self.db, principal, team_id, Permission::ConfigureHeap).await?;
                (
                    txn,
                    membership.role,
                    HeldScope::Team { team_id, heaps },
                    ParameterOwner::Team(team_id),
                )
            }
        };
        let current = store::parameters::lock_parameter(&txn, parameter_id)
            .await?
            .filter(|parameter| parameter.owner == owner)
            .ok_or(ServiceError::NotFound)?;
        if let Some(expected) = expected
            && current.version != expected
        {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }
        Ok(Held {
            txn,
            role,
            scope,
            current,
        })
    }

    /// The audit entry of a parameter write: the heap's log for a heap parameter or a heap's use of a team parameter,
    /// the team's alone (`heap_id` is `None`) for a team parameter's own changes.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    fn parameter_audit_entry(
        &self,
        team_id: TeamId,
        heap_id: Option<HeapId>,
        principal: Principal,
        action: AuditAction,
        parameter: &Parameter,
        diff: Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id,
            heap_id,
            actor: principal.user_id,
            action,
            entity_type: EntityType::Parameter,
            entity_id: *parameter.id.as_uuid(),
            diff,
            run_id: None,
            created_at: now,
        }
    }
}

/// The heap's parameters in use, by name, with what `role` may do with each.
pub(crate) async fn list_parameters_in_use(
    db: &impl ConnectionTrait,
    heap: &Heap,
    role: Role,
) -> Result<Vec<ParameterEntry>, ServiceError> {
    Ok(store::parameters::list_parameters(db, heap.id)
        .await?
        .into_iter()
        .filter(|parameter| !parameter.is_archived())
        .map(|parameter| with_actions(parameter, heap, role))
        .collect())
}

/// The heap's parameters as [`ParameterList`]: those in use and archived, which include the team parameters it uses
/// and has stopped using, and the team's other parameters in use apart.
async fn load_parameter_list(db: &impl ConnectionTrait, heap: Heap, role: Role) -> Result<ParameterList, ServiceError> {
    let (archived, in_use): (Vec<_>, Vec<_>) = store::parameters::list_parameters(db, heap.id)
        .await?
        .into_iter()
        .map(|parameter| with_actions(parameter, &heap, role))
        .partition(|entry| entry.parameter.is_archived());
    let used: BTreeSet<ParameterId> = in_use.iter().map(|entry| entry.parameter.id).collect();
    let available: Vec<Parameter> = store::parameters::list_team_parameters(db, heap.team_id)
        .await?
        .into_iter()
        .filter(|parameter| !parameter.is_archived() && !used.contains(&parameter.id))
        .collect();
    Ok(ParameterList {
        allowed_actions: allowed_parameter_list_actions(&heap, role),
        team_available: team_parameter_entries(db, heap.team_id, available, role).await?,
        heap,
        parameters: in_use,
        archived,
    })
}

/// `parameters` of the team with the heaps that use each now.
async fn team_parameter_entries(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    parameters: Vec<Parameter>,
    role: Role,
) -> Result<Vec<TeamParameterEntry>, ServiceError> {
    let ids: Vec<ParameterId> = parameters.iter().map(|parameter| parameter.id).collect();
    let mut heaps = heaps_using(db, team_id, &ids).await?;
    Ok(parameters
        .into_iter()
        .map(|parameter| TeamParameterEntry {
            team_id,
            heaps: heaps.remove(&parameter.id).unwrap_or_default(),
            allowed_actions: allowed_team_parameter_actions(&parameter, role),
            parameter,
        })
        .collect())
}

/// The heaps of the team that use each of `parameter_ids` now, by name.
async fn heaps_using(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    parameter_ids: &[ParameterId],
) -> Result<BTreeMap<ParameterId, Vec<Heap>>, ServiceError> {
    let uses = store::parameters::list_heap_uses(db, parameter_ids).await?;
    let team_heaps = store::heaps::list_heaps_for_teams(db, &[team_id]).await?;
    let mut using: BTreeMap<ParameterId, Vec<Heap>> = BTreeMap::new();
    for heap in team_heaps {
        for heap_use in uses.iter().filter(|u| u.heap_id == heap.id && u.stopped_at.is_none()) {
            using.entry(heap_use.parameter_id).or_default().push(heap.clone());
        }
    }
    Ok(using)
}

/// Reads a parameter named by a weights save under its own lock, after the caller locked the heap. It must be a
/// parameter of `heap`, or a team parameter the heap uses ([`ParameterError::NotInHeap`]), in use
/// ([`ParameterError::Archived`]) and at version `expected` ([`ServiceError::VersionConflict`]). Archived is
/// checked first: archiving moves the version on, and the caller needs to know the parameter is gone rather than
/// that it changed. A team parameter comes back as the heap sees it, with the heap's weight, and its version is the
/// team parameter's own.
pub(crate) async fn lock_listed(
    txn: &DatabaseTransaction,
    heap: &Heap,
    parameter_id: ParameterId,
    expected: Version,
) -> Result<Parameter, ServiceError> {
    let not_in_heap = ParameterError::NotInHeap { parameter_id };
    let belongs = |parameter: &Parameter| match parameter.owner {
        ParameterOwner::Heap(heap_id) => heap_id == heap.id,
        ParameterOwner::Team(team_id) => team_id == heap.team_id,
    };
    let found = store::parameters::find_parameter(txn, parameter_id)
        .await?
        .filter(belongs)
        .ok_or_else(|| not_in_heap.clone())?;
    // A team parameter is only read here, so it is locked shared (see `store::parameters::lock_parameter_shared`).
    let locked = if found.is_team_owned() {
        store::parameters::lock_parameter_shared(txn, parameter_id).await?
    } else {
        store::parameters::lock_parameter(txn, parameter_id).await?
    };
    let mut current = locked.ok_or_else(|| not_in_heap.clone())?;
    if current.is_archived() {
        return Err(ParameterError::Archived { parameter_id }.into());
    }
    if current.version != expected {
        return Err(ServiceError::VersionConflict {
            expected,
            current: current.version,
        });
    }
    if current.is_team_owned() {
        let heap_use = store::parameters::lock_heap_use(txn, heap.id, parameter_id)
            .await?
            .filter(|heap_use| heap_use.stopped_at.is_none())
            .ok_or(not_in_heap)?;
        current.weight = heap_use.weight;
    }
    Ok(current)
}

/// Reads a parameter of the heap's team under a shared lock, after the caller locked the heap (see
/// `store::parameters::lock_parameter_shared` for why not exclusive). Any other parameter is
/// [`ServiceError::NotFound`].
async fn lock_team_parameter_of(
    txn: &DatabaseTransaction,
    heap: &Heap,
    parameter_id: ParameterId,
) -> Result<Parameter, ServiceError> {
    store::parameters::lock_parameter_shared(txn, parameter_id)
        .await?
        .filter(|parameter| parameter.owner == ParameterOwner::Team(heap.team_id))
        .ok_or(ServiceError::NotFound)
}

fn with_actions(parameter: Parameter, heap: &Heap, role: Role) -> ParameterEntry {
    ParameterEntry {
        allowed_actions: allowed_parameter_actions(&parameter, heap, role),
        parameter,
    }
}

fn with_team_actions(parameter: Parameter, role: Role) -> ParameterEntry {
    ParameterEntry {
        allowed_actions: allowed_team_parameter_actions(&parameter, role),
        parameter,
    }
}

/// Refuses a name another parameter in use has, ignoring case: of the team, or of one of `heap_ids`, which are the
/// heap being written to, or every heap of the team. `except` is the one being renamed. The caller holds the locks
/// that keep a competing write out, and the read is a plain one (see [`authorize_heap_write_under_team`]).
async fn ensure_name_free(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    heap_ids: &[HeapId],
    name: &ParameterName,
    except: Option<ParameterId>,
) -> Result<(), ServiceError> {
    let key = name.to_key();
    let taken = store::parameters::list_names_in_use(db, team_id, heap_ids)
        .await?
        .into_iter()
        .any(|(id, other)| Some(id) != except && other.to_key() == key);
    if taken {
        return Err(ServiceError::ParameterNameTaken {
            name: name.as_str().to_owned(),
        });
    }
    Ok(())
}
