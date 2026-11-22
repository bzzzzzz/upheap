//! Custom parameter use cases (product.md, Manage parameters): listing a heap's parameters, and the admin's changes
//! to them.
//!
//! Every write locks the heap first, like every other heap write, then the parameter, and reads the parameter
//! under its lock. Names are unique among the heap's parameters in use, which the heap lock makes safe to check.
//! Parameters and options are archived, never deleted.
//!
//! A heap parameter can be made its team's, and the team's same-named heap parameters merge into it, once the caller
//! has confirmed the reach: the parameters it takes in, and how many items have a value for each. Like a team tag
//! merge, that writes in every heap of the team, so it takes every heap lock (design/architecture.md, Cycle detection
//! and locking).

use std::collections::{BTreeMap, BTreeSet};

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::heap::Heap;
use domain::item::Item;
use domain::list_view::SortKey;
use domain::parameter::{
    NewParameter, OptionLabel, OptionMerge, OptionSwitch, Parameter, ParameterError, ParameterName, ParameterOption,
    ParameterOwner, ParameterPatch, Weight, reach_text,
};
use domain::permission::{
    ParameterAction, ParameterListAction, Permission, allowed_parameter_actions, allowed_parameter_list_actions,
    allowed_team_parameter_actions, allowed_team_parameter_list_actions,
};
use domain::tag::MAX_TAGS_PER_ITEM;
use domain::team::Role;
use domain::validation::ValidationError;
use domain::{HeapId, ItemId, OptionId, ParameterId, TagId, TeamId, Timestamp, Version};
use serde_json::Value;
use store::audit::NewAuditEntry;
use store::parameters::HeapUse;
use store::{ConnectionTrait, DatabaseTransaction, StoreError, TransactionTrait};

use crate::access::{
    authorize_heap, authorize_heap_write, authorize_heap_write_under_team, authorize_team, begin_team_write,
    ensure_active, lower_team_heap,
};
use crate::audit::{
    describe_new_parameter, diff_heap_links, diff_heap_use, diff_items, diff_parameters, diff_switched_value,
};
use crate::auth::sha256_hex;
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

/// The heap parameters a merge into a team parameter takes in, and what they hold: what the caller confirms before it
/// (architecture.md, API §11).
#[derive(Clone, Debug)]
pub struct ParameterMergeReach {
    /// Names this reach: a client that confirms with it has seen exactly these parameters, counts and options.
    pub token: String,
    /// The parameter the merge starts from first, then the others by heap name, then heap ID.
    pub entries: Vec<ParameterReachEntry>,
}

/// One heap parameter in a [`ParameterMergeReach`].
#[derive(Clone, Debug)]
pub struct ParameterReachEntry {
    pub heap: Heap,
    /// As written, before the merge.
    pub parameter: Parameter,
    /// Items with a value for it, whatever their state.
    pub items: u32,
    /// The labels of its options the team parameter doesn't have yet. Empty for the one the merge starts from.
    pub added: Vec<OptionLabel>,
    /// The options of it in use that match an option the team parameter has archived: open items on them get the
    /// option in use that one resolves to. Empty for the one the merge starts from.
    pub switched: Vec<OptionSwitch>,
    /// The names of the tags it is linked to, sorted: each becomes its heap's link to the team parameter.
    pub linked_tags: Vec<String>,
    /// Whether it is the one the merge starts from.
    pub starting: bool,
}

/// A heap parameter that merges into the one a merge starts from, with its heap and how its options map.
struct MergedParameter {
    heap: Heap,
    parameter: Parameter,
    merge: OptionMerge,
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

    /// Suggests the team's parameter in every heap that uses it for exactly `tags`, tags of the team in use, if it is
    /// still at version `expected`. A tag that isn't the team's is [`ServiceError::TagNotInTeam`].
    pub async fn link_team_parameter_tags(
        &self,
        principal: Principal,
        team_id: TeamId,
        parameter_id: ParameterId,
        tags: BTreeSet<TagId>,
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
        let team_tags = store::tags::list_team_tags(&held.txn, team_id).await?;
        if let Some(&tag_id) = tags.iter().find(|id| !team_tags.iter().any(|tag| tag.id == **id)) {
            return Err(ServiceError::TagNotInTeam { tag_id });
        }
        ensure_tags_in_use(&held.txn, &tags).await?;
        let edited = held.current.link_tags(tags)?;
        let entry = self.finish_edit(held, principal, edited).await?;
        self.with_heaps(team_id, entry).await
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

    /// Makes a heap parameter its team's, in place: the same parameter, with its options and values, which the team's
    /// heaps then use, each at the weight it had. Every other parameter in use of the team's heaps (archived heaps
    /// included) with the same name, ignoring case, merges into it: each item's value moves to the option of the
    /// same label, ignoring case, options it lacks are added, and the merged parameter is archived. The heap the
    /// parameter was made in, and the heaps of those merged, use the team parameter at their old weights.
    ///
    /// That can't be undone, so it needs `confirm`, the token of the reach: without it, or with a stale one, the write
    /// fails with [`ServiceError::ParameterMergeUnconfirmed`] and the current reach, even when there is nothing to
    /// merge. Each parameter's tag links become its heap's own links to the team parameter, for that heap only; the
    /// team parameter has no links of its own.
    ///
    /// Repeating the write once it went through, on a team parameter the heap uses, changes nothing and returns it,
    /// without a confirmation.
    ///
    /// The parameter must be a parameter of `heap_id`, else [`ServiceError::NotFound`], in use
    /// ([`ParameterError::Archived`]), in a heap that isn't archived ([`ServiceError::HeapArchived`]).
    ///
    /// Each item whose value moves gets an entry in its heap's log; the team's log gets one listing what was taken in.
    /// Saved views of the merged parameters' heaps filter and sort by the team parameter's options. Size assumption: a
    /// handful of queries per item reached under every heap lock of the team, so item writes in the team wait for it.
    /// That is fine for up to a few hundred items; there is no limit.
    pub async fn make_team_parameter(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        confirm: Option<&str>,
    ) -> Result<TeamParameterEntry, ServiceError> {
        let team_id = store::heaps::find_heap(&self.db, heap_id)
            .await?
            .ok_or(ServiceError::NotFound)?
            .team_id;
        let (txn, membership, heaps) =
            begin_team_write(&self.db, principal, team_id, Permission::ConfigureHeap).await?;
        let heap = heaps
            .iter()
            .find(|heap| heap.id == heap_id)
            .ok_or(ServiceError::NotFound)?;
        ensure_active(heap)?;
        let found = store::parameters::lock_parameter(&txn, parameter_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        if found.owner == ParameterOwner::Team(team_id) {
            // Repeating the write after it went through: the parameter is the team's and the heap uses it. Nothing
            // changes and no confirmation is needed.
            if found.is_archived() {
                return Err(ParameterError::Archived { parameter_id }.into());
            }
            let used = store::parameters::lock_heap_use(&txn, heap_id, parameter_id)
                .await?
                .is_some_and(|heap_use| heap_use.stopped_at.is_none());
            if !used {
                return Err(ServiceError::NotFound);
            }
            txn.rollback().await.map_err(StoreError::from)?;
            let allowed_actions = allowed_team_parameter_actions(&found, membership.role);
            return self
                .with_heaps(
                    team_id,
                    ParameterEntry {
                        parameter: found,
                        allowed_actions,
                    },
                )
                .await;
        }
        let start = Some(found)
            .filter(|parameter| parameter.owner == ParameterOwner::Heap(heap_id))
            .ok_or(ServiceError::NotFound)?;
        if start.is_archived() {
            return Err(ParameterError::Archived { parameter_id }.into());
        }

        let now = self.now();
        let (team_parameter, others) = self
            .plan_parameter_merge(&txn, team_id, &heaps, heap_id, &start, now)
            .await?;
        let (reach, values) = parameter_reach_of(&txn, heap, &start, &others).await?;
        if confirm != Some(reach.token.as_str()) {
            return Err(ServiceError::ParameterMergeUnconfirmed { reach });
        }

        let saved = store::parameters::update_parameter(&txn, &team_parameter, start.version, now)
            .await?
            .ok_or_else(|| locked_row_changed("parameters", &parameter_id.to_string()))?;
        self.merge_into_team_parameter(
            &txn,
            principal,
            team_id,
            &saved,
            start.weight,
            heap,
            &others,
            &values,
            &reach,
            now,
        )
        .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, %heap_id, %parameter_id, user_id = %principal.user_id, merged = others.len(), "heap parameter made a team parameter");
        let allowed_actions = allowed_team_parameter_actions(&saved, membership.role);
        self.with_heaps(
            team_id,
            ParameterEntry {
                parameter: saved,
                allowed_actions,
            },
        )
        .await
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
        // Written before it is read, stopped (see `store::parameters::ensure_heap_use`), so a first use starts at the
        // default weight and its entry says only that the heap now uses it.
        store::parameters::ensure_heap_use(&txn, heap_id, parameter_id, now).await?;
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
        self.record_heap_use(&txn, &access.heap, principal, &parameter, &before, &after, now)
            .await?;
        let list = load_parameter_list(&txn, access.heap, access.membership.role).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(%heap_id, %parameter_id, user_id = %principal.user_id, "team parameter used");
        Ok(list)
    }

    /// Links the tags in `link` to a team parameter the heap uses and unlinks those in `unlink`, so it is suggested
    /// in this heap for them besides the team's links, and returns the heap's parameters. A tag to link that isn't
    /// one of the heap's in use is [`ServiceError::TagNotInHeap`]; a tag in both sets, or more than the most a
    /// parameter may be linked to once done, is invalid. A parameter that isn't a team parameter the heap has a use
    /// row for is [`ServiceError::NotFound`]; one archived in the heap's view is [`ParameterError::Archived`]. Each
    /// tag ends up linked or unlinked whatever it was, and a change that changes nothing writes nothing.
    ///
    /// No `If-Match`: the links are written per tag, so two writes that race each do what they say and neither
    /// undoes the other, and there is no version for a heap's links. The parameter is only read, under a shared
    /// lock, so its version doesn't move.
    pub async fn link_heap_tags(
        &self,
        principal: Principal,
        heap_id: HeapId,
        parameter_id: ParameterId,
        link: BTreeSet<TagId>,
        unlink: BTreeSet<TagId>,
    ) -> Result<ParameterList, ServiceError> {
        if !link.is_disjoint(&unlink) {
            return Err(ValidationError::TagLinkedAndUnlinked.into());
        }
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::ConfigureHeap).await?;
        let parameter = lock_team_parameter_of(&txn, &access.heap, parameter_id).await?;
        let heap_use = store::parameters::lock_heap_use(&txn, heap_id, parameter_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        if parameter.is_archived() || heap_use.stopped_at.is_some() {
            return Err(ParameterError::Archived { parameter_id }.into());
        }
        load_heap_tags(&txn, heap_id, &link).await?;
        ensure_tags_in_use(&txn, &link).await?;
        let now = self.now();
        let (before, after) =
            store::parameters::change_heap_links(&txn, heap_id, parameter_id, &link, &unlink, now).await?;
        if after.len() > MAX_TAGS_PER_ITEM {
            return Err(ValidationError::TooManyTags { max: MAX_TAGS_PER_ITEM }.into());
        }
        if before == after {
            return load_parameter_list(&txn, access.heap, access.membership.role).await;
        }
        let entry = self.parameter_audit_entry(
            access.heap.team_id,
            Some(heap_id),
            principal,
            AuditAction::ParameterUpdated,
            &parameter,
            diff_heap_links(&before, &after),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let list = load_parameter_list(&txn, access.heap, access.membership.role).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(%heap_id, %parameter_id, user_id = %principal.user_id, links = after.len(), "team parameter tags linked in heap");
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
        self.record_heap_use(&txn, &access.heap, principal, &parameter, &before, &after, now)
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
        self.record_heap_use(txn, heap, principal, current, &before, &after, now)
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
        before: &HeapUse,
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

    /// The team parameter `start` becomes, with the options of every other parameter in use of `heaps` that has its
    /// name taken in, and those others, by heap name then heap ID. Read under the locks, so the others are locked too.
    async fn plan_parameter_merge(
        &self,
        txn: &DatabaseTransaction,
        team_id: TeamId,
        heaps: &[Heap],
        start_heap: HeapId,
        start: &Parameter,
        now: Timestamp,
    ) -> Result<(Parameter, Vec<MergedParameter>), ServiceError> {
        let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
        let key = start.name.to_key();
        let mut others = Vec::new();
        for (id, name) in store::parameters::list_names_in_use(txn, team_id, &heap_ids).await? {
            if id == start.id || name.to_key() != key {
                continue;
            }
            // The query and the name rule leave only the other heaps' parameters in use here: anything else is a row
            // the rule should have kept out.
            let parameter = store::parameters::lock_parameter(txn, id)
                .await?
                .ok_or_else(|| StoreError::corrupt("parameters", &id.to_string(), "a listed parameter is gone"))?;
            let ParameterOwner::Heap(heap_id) = parameter.owner else {
                return Err(
                    StoreError::corrupt("parameters", &id.to_string(), "a team parameter named like a heap's").into(),
                );
            };
            if heap_id == start_heap || parameter.is_archived() {
                return Err(StoreError::corrupt(
                    "parameters",
                    &id.to_string(),
                    "two parameters in use with one name in a heap",
                )
                .into());
            }
            let heap = heaps.iter().find(|heap| heap.id == heap_id).ok_or_else(|| {
                StoreError::corrupt("parameters", &id.to_string(), "a heap parameter of no heap of its team")
            })?;
            others.push((heap.clone(), parameter));
        }
        others.sort_by(|a, b| (a.0.name.as_str(), a.0.id).cmp(&(b.0.name.as_str(), b.0.id)));

        let mut team_parameter = start.into_team(team_id)?;
        let mut merged = Vec::with_capacity(others.len());
        for (heap, parameter) in others {
            let merge = team_parameter.merge_options(&parameter, || OptionId::from_uuid(self.new_id(now)))?;
            team_parameter = merge.merged.clone();
            merged.push(MergedParameter { heap, parameter, merge });
        }
        Ok((team_parameter, merged))
    }

    /// Writes everything a merge does besides the team parameter itself, which the caller has saved as `team_parameter`:
    /// the heaps' use rows, the items' values, the heaps' saved views, the merged parameters archived, and the
    /// team's log entry. `values` are the values of the reach's parameters; `start_weight` is the weight the team
    /// parameter had in `start_heap`.
    #[allow(clippy::too_many_arguments)] // The merge's inputs, all read by the caller under the locks.
    async fn merge_into_team_parameter(
        &self,
        txn: &DatabaseTransaction,
        principal: Principal,
        team_id: TeamId,
        team_parameter: &Parameter,
        start_weight: Weight,
        start_heap: &Heap,
        others: &[MergedParameter],
        values: &[(ItemId, ParameterId, OptionId)],
        reach: &ParameterMergeReach,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        // The team parameter has no links of its own: each parameter's links continue as its heap's.
        let start_tags = &reach.entries[0].parameter.tags;
        write_heap_use(txn, start_heap.id, team_parameter.id, start_weight, now).await?;
        store::parameters::set_heap_links(txn, start_heap.id, team_parameter.id, start_tags, now).await?;
        for other in others {
            write_heap_use(txn, other.heap.id, team_parameter.id, other.parameter.weight, now).await?;
            store::parameters::set_heap_links(txn, other.heap.id, team_parameter.id, &other.parameter.tags, now)
                .await?;
            self.move_values(txn, principal, team_id, team_parameter, other, values, now)
                .await?;
            move_views(txn, team_parameter.id, other).await?;
            let archived = other.parameter.archive(now).ok_or_else(|| {
                StoreError::corrupt(
                    "parameters",
                    &other.parameter.id.to_string(),
                    "a merged parameter is archived",
                )
            })?;
            store::parameters::update_parameter(txn, &archived, other.parameter.version, now)
                .await?
                .ok_or_else(|| locked_row_changed("parameters", &other.parameter.id.to_string()))?;
        }
        let listed: Vec<Value> = reach
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "heap": entry.heap.id.to_string(),
                    "parameter": entry.parameter.id.to_string(),
                    "name": entry.parameter.name.as_str(),
                    "weight": entry.parameter.weight.get(),
                    "items": entry.items,
                    "added": entry.added.iter().map(OptionLabel::as_str).collect::<Vec<_>>(),
                    "tags": entry.parameter.tags.iter().map(ToString::to_string).collect::<Vec<_>>(),
                })
            })
            .collect();
        let entry = self.parameter_audit_entry(
            team_id,
            None,
            principal,
            AuditAction::ParameterMerged,
            team_parameter,
            serde_json::json!({ AuditField::Merged.code(): listed }),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
    }

    /// Moves each item's value for the merged parameter to the team parameter's option it became, and records it in
    /// the item's heap's log. A value on an option that is archived in the team parameter goes to the option in use
    /// it resolves to when the item is open, as archiving an option does for open items.
    #[allow(clippy::too_many_arguments)] // The merge's inputs, all read by the caller under the locks.
    async fn move_values(
        &self,
        txn: &DatabaseTransaction,
        principal: Principal,
        team_id: TeamId,
        team_parameter: &Parameter,
        other: &MergedParameter,
        values: &[(ItemId, ParameterId, OptionId)],
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let mut item_ids: Vec<ItemId> = values
            .iter()
            .filter(|(_, parameter, _)| *parameter == other.parameter.id)
            .map(|(item, _, _)| *item)
            .collect();
        item_ids.sort();
        for item_id in item_ids {
            let item = store::items::find_item(txn, item_id)
                .await?
                .ok_or_else(|| StoreError::corrupt("items", &item_id.to_string(), "an item with a value is missing"))?;
            let Some(&old) = item.parameters.get(&other.parameter.id) else {
                continue;
            };
            let mapped = other.merge.option_map.get(&old).copied().ok_or_else(|| {
                StoreError::corrupt(
                    "item_parameter_values",
                    &item_id.to_string(),
                    "a value of no option of its parameter",
                )
            })?;
            let option = if item.state.kind().is_closed() {
                mapped
            } else {
                team_parameter
                    .resolve(mapped)
                    .map(|resolved| resolved.option.id)
                    .ok_or(ParameterError::OptionNotInParameter {
                        parameter_id: team_parameter.id,
                        option_id: mapped,
                    })?
            };
            let mut parameters = item.parameters.clone();
            parameters.remove(&other.parameter.id);
            parameters.insert(team_parameter.id, option);
            let edited = Item {
                parameters,
                ..item.clone()
            };
            store::items::update_item(txn, &edited, item.version, now)
                .await?
                .ok_or_else(|| locked_row_changed("items", &item_id.to_string()))?;
            let entry = NewAuditEntry {
                id: self.new_id(now),
                team_id,
                heap_id: Some(item.heap_id),
                actor: principal.user_id,
                action: AuditAction::ParameterMerged,
                entity_type: EntityType::Item,
                entity_id: *item.id.as_uuid(),
                diff: diff_items(&item, &edited),
                run_id: None,
                created_at: now,
            };
            store::audit::insert_audit_entry(txn, &entry).await?;
        }
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

/// A row the merge holds the lock for changed under it, which the locks rule out.
fn locked_row_changed(table: &'static str, id: &str) -> ServiceError {
    StoreError::corrupt(table, id, "a row changed under the lock of its heap").into()
}

/// The reach of a merge starting from `start` in `start_heap` into `others`, and the values of the parameters in it.
async fn parameter_reach_of(
    db: &impl ConnectionTrait,
    start_heap: &Heap,
    start: &Parameter,
    others: &[MergedParameter],
) -> Result<(ParameterMergeReach, Vec<(ItemId, ParameterId, OptionId)>), ServiceError> {
    let ids: Vec<ParameterId> = std::iter::once(start.id)
        .chain(others.iter().map(|other| other.parameter.id))
        .collect();
    let values = store::items::list_parameter_values(db, &ids).await?;
    let count = |id: ParameterId| {
        let items: BTreeSet<ItemId> = values
            .iter()
            .filter(|(_, parameter, _)| *parameter == id)
            .map(|(item, _, _)| *item)
            .collect();
        u32::try_from(items.len()).unwrap_or(u32::MAX)
    };
    let mut entries = vec![ParameterReachEntry {
        heap: start_heap.clone(),
        parameter: start.clone(),
        items: count(start.id),
        added: Vec::new(),
        switched: Vec::new(),
        linked_tags: linked_tag_names(db, start).await?,
        starting: true,
    }];
    for other in others {
        entries.push(ParameterReachEntry {
            heap: other.heap.clone(),
            parameter: other.parameter.clone(),
            items: count(other.parameter.id),
            added: other.merge.added.clone(),
            switched: other.merge.switched.clone(),
            linked_tags: linked_tag_names(db, &other.parameter).await?,
            starting: false,
        });
    }
    let text = reach_text(entries.iter().map(|entry| {
        (
            entry.parameter.id,
            entry.items,
            entry.added.as_slice(),
            entry.switched.as_slice(),
        )
    }));
    // The first 8 bytes of the SHA-256: 16 hex digits, enough to tell one reach from another.
    let token = sha256_hex(&text).chars().take(16).collect();
    Ok((ParameterMergeReach { token, entries }, values))
}

/// The names of the tags `parameter` is linked to, sorted.
async fn linked_tag_names(db: &impl ConnectionTrait, parameter: &Parameter) -> Result<Vec<String>, ServiceError> {
    let mut names = Vec::new();
    for tag_id in &parameter.tags {
        let tag = store::tags::find_tag(db, *tag_id).await?.ok_or_else(|| {
            StoreError::corrupt("parameter_tags", &parameter.id.to_string(), "a link to a missing tag")
        })?;
        names.push(tag.name.as_str().to_owned());
    }
    names.sort();
    Ok(names)
}

/// Makes `heap_id` use the team parameter `parameter_id` at `weight`, from `now`.
async fn write_heap_use(
    txn: &DatabaseTransaction,
    heap_id: HeapId,
    parameter_id: ParameterId,
    weight: Weight,
    now: Timestamp,
) -> Result<(), ServiceError> {
    store::parameters::ensure_heap_use(txn, heap_id, parameter_id, now).await?;
    let before = store::parameters::lock_heap_use(txn, heap_id, parameter_id)
        .await?
        .ok_or_else(|| {
            StoreError::corrupt(
                "heap_parameters",
                &parameter_id.to_string(),
                "a use row just written is gone",
            )
        })?;
    let after = HeapUse {
        weight,
        stopped_at: None,
        ..before
    };
    store::parameters::update_heap_use(txn, &after).await?;
    Ok(())
}

/// Points the saved views of the merged parameter's heap, archived ones included, at the team parameter: the options
/// it filters by become the ones they merged into, and a sort by the merged parameter becomes a sort by the team
/// parameter. Like a tag merge, views get no entries.
async fn move_views(
    txn: &DatabaseTransaction,
    team_parameter: ParameterId,
    other: &MergedParameter,
) -> Result<(), ServiceError> {
    for mut view in store::saved_views::list_all_saved_views(txn, other.heap.id).await? {
        let filter = &view.list_view.filter;
        let sorted = view.list_view.sort == SortKey::Parameter(other.parameter.id);
        if !sorted
            && filter
                .options
                .is_disjoint(&other.merge.option_map.keys().copied().collect())
        {
            continue;
        }
        let expected = view.version;
        view.list_view.filter.options = view
            .list_view
            .filter
            .options
            .iter()
            .map(|option| other.merge.option_map.get(option).copied().unwrap_or(*option))
            .collect();
        if sorted {
            view.list_view.sort = SortKey::Parameter(team_parameter);
        }
        store::saved_views::update_saved_view(txn, &view, expected)
            .await?
            .ok_or_else(|| locked_row_changed("saved_views", &view.id.to_string()))?;
    }
    Ok(())
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
