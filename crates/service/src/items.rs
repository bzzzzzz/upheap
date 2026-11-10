//! Item use cases: the ranked list, the closed list, adding and editing.

use std::collections::{BTreeSet, HashMap, HashSet};

use domain::audit::{AuditAction, EntityType};
use domain::handle::ItemKey;
use domain::heap::{Heap, HeapPolicy, HeapPolicyPatch};
use domain::item::{Item, ItemPatch, ItemSummary, NewItem, Stage, StateKind};
use domain::list_view::{Group, ListView};
use domain::parameter::{OptionLabel, Parameter, ParameterError, ParameterName, Weight, check_values, current_values};
use domain::permission::{
    ItemAction, Permission, StepAction, allowed_item_actions, allowed_stage_choices, allowed_step_actions,
};
use domain::ranking::{CURRENT_FORMULA, Contributor, ItemForRanking, rank};
use domain::step::Step;
use domain::tag::Tag;
use domain::team::Role;
use domain::user::User;
use domain::{Duration, HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, UserId, ValidationError, Version};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

pub use store::items::ClosedCursor;

use crate::access::{authorize_heap, authorize_heap_write};
use crate::audit::{describe_new_item, diff_items};
use crate::{Principal, RandomSource, ServiceError, Services};

/// A client may keep ranking at an `as_of` for at most this long. Older values are replaced by the current time.
pub const MAX_AS_OF_AGE: Duration = Duration::from_hours(24);

/// Largest page of closed items.
pub const MAX_CLOSED_PAGE: u64 = 200;

/// How many items a ranking preview lists when the request doesn't say.
pub const DEFAULT_PREVIEW_LIMIT: usize = 5;

/// Most items a ranking preview lists.
pub const MAX_PREVIEW_LIMIT: usize = 20;

/// An item with its tags, its parameter values, its blocker, and what the caller may do with it.
#[derive(Debug)]
pub struct ItemView {
    pub item: Item,
    /// The item's tags, by name.
    pub tags: Vec<Tag>,
    /// Its value for each of the heap's parameters in use, set or not, and for each archived parameter it still
    /// has a value for, by parameter name.
    pub parameters: Vec<ParameterValue>,
    /// The item it waits for, when it's blocked and one was picked.
    pub blocked_by: Option<ItemSummary>,
    /// Who added it, when that is known (see [`Item::created_by`]).
    pub created_by: Option<User>,
    /// Who closed it. `None` when it's open or was closed before this was recorded (see [`domain::item::ItemState::closed_by`]).
    pub closed_by: Option<User>,
    pub allowed_actions: Vec<ItemAction>,
    /// The stages the caller may set on it now, in table order (see [`domain::permission::allowed_stage_choices`]).
    pub stage_choices: Vec<Stage>,
    /// A task's steps in order; empty for a note.
    pub steps: Vec<Step>,
    /// What the caller may do with each of the steps, the same for all of them (see
    /// [`domain::permission::allowed_step_actions`]).
    pub step_actions: Vec<StepAction>,
}

/// An item's value for one custom parameter, ready to show.
#[derive(Debug)]
pub struct ParameterValue {
    pub parameter_id: ParameterId,
    pub name: ParameterName,
    /// Whether the parameter is archived. Only closed items, and items reopened since, have values for one.
    pub parameter_archived: bool,
    /// `None` when unset.
    pub option: Option<OptionValue>,
}

/// The option an item has for a parameter.
#[derive(Debug)]
pub struct OptionValue {
    pub id: OptionId,
    pub label: OptionLabel,
    /// Whether the option is archived. Closed items keep archived options.
    pub archived: bool,
}

/// An item in the ranked list.
#[derive(Debug)]
pub struct RankedEntry {
    pub view: ItemView,
    /// 1-based position within its group, in the order the group is listed.
    pub position: u32,
    pub contributors: Vec<Contributor>,
}

/// The heap's non-closed items, ranked at `as_of`, then filtered, grouped and sorted by a [`ListView`].
#[derive(Debug)]
pub struct RankedList {
    pub heap: Heap,
    pub role: Role,
    /// The time the ranking used. Clients send it back to keep the order stable.
    pub as_of: Timestamp,
    /// Items in pinned states, shown above the rest.
    pub pinned: Vec<RankedEntry>,
    /// Items in shown states, including blocked items mixed in.
    pub ranked: Vec<RankedEntry>,
    /// Shown blocked items, when they're grouped.
    pub blocked: Vec<RankedEntry>,
    /// Items the filters keep but whose state is hidden.
    pub hidden_count: u32,
    /// Items the view would list without its filters: those in pinned and shown states.
    pub unfiltered_count: u32,
    /// Items of the heap in progress, whatever the view and its filters leave out.
    pub in_progress_count: u32,
}

/// Weights to rank a heap with instead of the stored ones, without saving them. Weights left out keep their stored
/// value.
#[derive(Clone, Debug, Default)]
pub struct WeightsDraft {
    pub heap: HeapPolicyPatch,
    pub parameters: Vec<ParameterWeightDraft>,
}

/// A draft weight for one parameter in use.
#[derive(Clone, Copy, Debug)]
pub struct ParameterWeightDraft {
    pub id: ParameterId,
    pub weight: Weight,
}

/// An item near the top of the list, with where the stored weights put it.
#[derive(Debug)]
pub struct PreviewEntry {
    pub item: ItemSummary,
    /// 1-based position within the default view's ranked group, under the draft weights.
    pub position: u32,
    /// Its 1-based position in the same group under the stored weights.
    pub current_position: u32,
}

/// The top of a heap's list under draft weights.
#[derive(Debug)]
pub struct RankingPreview {
    /// The time both rankings used.
    pub as_of: Timestamp,
    /// The first items of the ranked group under the draft, in order.
    pub items: Vec<PreviewEntry>,
}

/// A preview's item count: `limit`, or [`DEFAULT_PREVIEW_LIMIT`] when `None`. Fails with
/// [`ValidationError::LimitOutOfRange`] outside 1 to [`MAX_PREVIEW_LIMIT`].
pub fn parse_preview_limit(limit: Option<i64>) -> Result<usize, ValidationError> {
    let Some(limit) = limit else {
        return Ok(DEFAULT_PREVIEW_LIMIT);
    };
    usize::try_from(limit)
        .ok()
        .filter(|limit| (1..=MAX_PREVIEW_LIMIT).contains(limit))
        .ok_or(ValidationError::LimitOutOfRange {
            min: 1,
            max: MAX_PREVIEW_LIMIT,
        })
}

#[derive(Debug)]
pub struct ClosedPage {
    pub heap: Heap,
    pub items: Vec<ItemView>,
    /// Where the next page starts, or `None` on the last page.
    pub next: Option<ClosedCursor>,
}

impl Services {
    /// Ranks every non-closed item of the heap, then shows them as `view` says.
    ///
    /// `requested_as_of` freezes the time input so the order stays stable within a session. It's ignored when
    /// it's in the future or more than [`MAX_AS_OF_AGE`] old, and the current time is used instead. Every tag and
    /// option in the view's filter, and the parameter it sorts by, must belong to the heap, or it fails with
    /// [`ServiceError::TagNotInHeap`], [`ServiceError::OptionNotInHeap`] or [`ParameterError::NotInHeap`].
    pub async fn list_ranked(
        &self,
        principal: Principal,
        heap_id: HeapId,
        requested_as_of: Option<Timestamp>,
        view: &ListView,
    ) -> Result<RankedList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let now = self.now();
        let as_of = requested_as_of
            .filter(|t| *t <= now && now.since(*t) <= MAX_AS_OF_AGE)
            .unwrap_or(now);
        let states = [StateKind::Open, StateKind::InProgress, StateKind::Blocked];
        let mut items = store::items::list_items_in_states(&self.db, heap_id, &states).await?;
        let heap_tags = store::tags::list_tags(&self.db, heap_id).await?;
        let parameters = store::parameters::list_parameters(&self.db, heap_id).await?;
        ensure_view_in_heap(&heap_tags, &parameters, view)?;

        // Every item is ranked, because a blocker inherits from what it blocks even when that is left out below.
        let ranking = {
            let inputs: Vec<ItemForRanking<'_>> = items.iter().map(ItemForRanking::from).collect();
            rank(CURRENT_FORMULA, &access.heap.policy, &parameters, &inputs, as_of)
        };
        let item_count = items.len();
        let unfiltered_count = count_items(&items, |item| view.lists(item.state.kind()));
        let in_progress_count = count_items(&items, |item| item.state.kind() == StateKind::InProgress);
        let blockers = blocker_summaries(&self.db, heap_id, &items).await?;
        let users = load_item_users(&self.db, &items).await?;

        let role = access.membership.role;
        let (mut pinned, mut ranked, mut blocked) = (Vec::new(), Vec::new(), Vec::new());
        let mut hidden_count = 0_u32;
        for entry in ranking {
            let Some(index) = items.iter().position(|item| item.id == entry.item_id) else {
                continue;
            };
            let item = items.swap_remove(index);
            if !view.filter.matches(&item, &parameters) {
                continue;
            }
            let group = match view.group_of(item.state.kind()) {
                Some(Group::Pinned) => &mut pinned,
                Some(Group::Ranked) => &mut ranked,
                Some(Group::Blocked) => &mut blocked,
                None => {
                    hidden_count = hidden_count.saturating_add(1);
                    continue;
                }
            };
            let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
            let stage_choices = allowed_stage_choices(&item, &access.heap, role);
            let step_actions = allowed_step_actions(&item, &access.heap, role);
            let tags = select_tags(&heap_tags, &item.tags);
            let values = describe_values(&item, &parameters);
            let blocked_by = item.blocked_by().and_then(|id| blockers.get(&id).cloned());
            let created_by = pick_user(&users, item.created_by);
            group.push(RankedEntry {
                view: ItemView {
                    item,
                    tags,
                    parameters: values,
                    blocked_by,
                    created_by,
                    // Ranked items aren't closed, so they have no closer.
                    closed_by: None,
                    allowed_actions,
                    stage_choices,
                    // Loaded below for the entries that are listed, not for the ones filtered out.
                    steps: Vec::new(),
                    step_actions,
                },
                position: 0,
                contributors: entry.contributors,
            });
        }
        for (group, entries) in [
            (Group::Pinned, &mut pinned),
            (Group::Ranked, &mut ranked),
            (Group::Blocked, &mut blocked),
        ] {
            view.sort(group, entries, |entry| &entry.view.item, &parameters);
            for (index, entry) in entries.iter_mut().enumerate() {
                entry.position = u32::try_from(index + 1).unwrap_or(u32::MAX);
            }
        }

        let mut steps = {
            let listed: Vec<ItemId> = [&pinned, &ranked, &blocked]
                .into_iter()
                .flatten()
                .map(|entry| entry.view.item.id)
                .collect();
            store::steps::list_steps_for_items(&self.db, &listed).await?
        };
        for entry in pinned.iter_mut().chain(&mut ranked).chain(&mut blocked) {
            entry.view.steps = steps.remove(&entry.view.item.id).unwrap_or_default();
        }

        tracing::debug!(%heap_id, items = item_count, as_of = as_of.as_millis(), "ranked heap");
        Ok(RankedList {
            heap: access.heap,
            role,
            as_of,
            pinned,
            ranked,
            blocked,
            hidden_count,
            unfiltered_count,
            in_progress_count,
        })
    }

    /// The first `limit` items of the default list view's ranked group (open items by priority) under the heap's
    /// stored weights changed by `draft`, each with its position under the stored weights. Nothing is saved. Any
    /// member may ask, and an archived heap can be previewed.
    ///
    /// A parameter in the draft must be one in use of the heap ([`ParameterError::NotInHeap`],
    /// [`ParameterError::Archived`]) and listed once ([`ValidationError::DuplicateParameter`]).
    pub async fn preview_ranking(
        &self,
        principal: Principal,
        heap_id: HeapId,
        draft: WeightsDraft,
        limit: usize,
    ) -> Result<RankingPreview, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let as_of = self.now();
        let states = [StateKind::Open, StateKind::InProgress, StateKind::Blocked];
        let items = store::items::list_items_in_states(&self.db, heap_id, &states).await?;
        let stored_parameters = store::parameters::list_parameters(&self.db, heap_id).await?;
        let drafted_parameters = apply_parameter_draft(&stored_parameters, &draft.parameters)?;
        let drafted_policy = draft.heap.apply(&access.heap.policy);

        let view = ListView::default();
        let current = ranked_group(&view, &items, &access.heap.policy, &stored_parameters, as_of);
        let drafted = ranked_group(&view, &items, &drafted_policy, &drafted_parameters, as_of);
        let current_positions: HashMap<ItemId, usize> =
            current.iter().enumerate().map(|(i, id)| (*id, i + 1)).collect();
        let by_id: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
        let entries = drafted
            .iter()
            .take(limit)
            .enumerate()
            .filter_map(|(index, id)| {
                // Both groups hold the same items, so each has a position and an item.
                let item = by_id.get(id)?;
                let current_position = current_positions.get(id)?;
                Some(PreviewEntry {
                    item: ItemSummary::from(*item),
                    position: to_position(index + 1),
                    current_position: to_position(*current_position),
                })
            })
            .collect();

        tracing::debug!(%heap_id, items = items.len(), as_of = as_of.as_millis(), "ranked heap preview");
        Ok(RankingPreview { as_of, items: entries })
    }

    /// Closed items, most recently closed first.
    pub async fn list_closed(
        &self,
        principal: Principal,
        heap_id: HeapId,
        after: Option<ClosedCursor>,
        limit: u64,
    ) -> Result<ClosedPage, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let limit = limit.clamp(1, MAX_CLOSED_PAGE);
        // One extra row tells whether another page follows.
        let mut items = store::items::list_closed_items(&self.db, heap_id, after, limit + 1).await?;
        let has_more = items.len() as u64 > limit;
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = has_more
            .then(|| {
                items.last().and_then(|last| {
                    last.state
                        .closed_at()
                        .map(|closed_at| ClosedCursor { closed_at, id: last.id })
                })
            })
            .flatten();

        let heap_tags = store::tags::list_tags(&self.db, heap_id).await?;
        let parameters = store::parameters::list_parameters(&self.db, heap_id).await?;
        let users = load_item_users(&self.db, &items).await?;
        let item_ids: Vec<ItemId> = items.iter().map(|item| item.id).collect();
        let mut steps = store::steps::list_steps_for_items(&self.db, &item_ids).await?;
        let now = self.now();
        let role = access.membership.role;
        let items = items
            .into_iter()
            .map(|item| {
                let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
                let stage_choices = allowed_stage_choices(&item, &access.heap, role);
                let step_actions = allowed_step_actions(&item, &access.heap, role);
                let steps = steps.remove(&item.id).unwrap_or_default();
                let tags = select_tags(&heap_tags, &item.tags);
                let values = describe_values(&item, &parameters);
                let created_by = pick_user(&users, item.created_by);
                let closed_by = pick_user(&users, item.state.closed_by());
                // Only blocked items have a blocker, and closed items aren't blocked.
                ItemView {
                    item,
                    tags,
                    parameters: values,
                    blocked_by: None,
                    created_by,
                    closed_by,
                    allowed_actions,
                    stage_choices,
                    steps,
                    step_actions,
                }
            })
            .collect();
        Ok(ClosedPage {
            heap: access.heap,
            items,
            next,
        })
    }

    /// Adds an open item to the heap. Every tag must belong to the heap, or it fails with
    /// [`ServiceError::TagNotInHeap`]. Every value must be an option in use of a parameter in use of the heap
    /// ([`ParameterError::NotInHeap`], [`ParameterError::OptionNotInParameter`]).
    pub async fn create_item(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewItem,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::EditItems).await?;
        let tags = load_heap_tags(&txn, heap_id, &fields.tags).await?;
        let parameters = store::parameters::list_parameters_locking(&txn, heap_id).await?;
        check_values(&parameters, fields.parameters.iter().map(|(p, o)| (*p, Some(*o))))?;

        let now = self.now();
        let key = new_item_key(&txn, &*self.random, heap_id).await?;
        let item = Item::new(
            ItemId::from_uuid(self.new_id(now)),
            heap_id,
            key,
            fields,
            principal.user_id,
            now,
        );
        store::items::insert_item(&txn, &item).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::ItemCreated,
            &item,
            describe_new_item(&item),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let created_by = store::users::find_user(&txn, principal.user_id).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, item_id = %item.id, user_id = %principal.user_id, "item created");
        let allowed_actions = allowed_item_actions(&item, &access.heap, access.membership.role, now);
        let stage_choices = allowed_stage_choices(&item, &access.heap, access.membership.role);
        let step_actions = allowed_step_actions(&item, &access.heap, access.membership.role);
        let values = describe_values(&item, &parameters);
        // New items are open notes, so they have no blocker, no closer and no steps.
        Ok(ItemView {
            item,
            tags,
            parameters: values,
            blocked_by: None,
            created_by,
            closed_by: None,
            allowed_actions,
            stage_choices,
            steps: Vec::new(),
            step_actions,
        })
    }

    pub async fn find_item(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let item = store::items::find_item(&self.db, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap(&self.db, principal, item.heap_id, Permission::ViewHeap).await?;
        let steps = store::steps::list_steps(&self.db, item.id).await?;
        build_item_view(&self.db, &access.heap, access.membership.role, item, steps, self.now()).await
    }

    /// Applies a partial edit if the item is still at version `expected`. New tags must belong to the item's
    /// heap, or it fails with [`ServiceError::TagNotInHeap`]. A new blocker must be a non-closed item of the same
    /// heap that doesn't already wait for this one ([`ServiceError::BlockerNotInHeap`],
    /// [`ServiceError::BlockerClosed`], [`ServiceError::BlockingCycle`]). A value set must be an option in use of a
    /// parameter in use of the heap; a value cleared may be of any of its parameters.
    ///
    /// A patch that changes nothing succeeds and returns the item unchanged, without an audit entry.
    pub async fn update_item(
        &self,
        principal: Principal,
        item_id: ItemId,
        patch: ItemPatch,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, current.heap_id, Permission::EditItems).await?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        if let Some(tags) = &patch.tags {
            load_heap_tags(&txn, current.heap_id, tags).await?;
        }
        let parameters = store::parameters::list_parameters_locking(&txn, current.heap_id).await?;

        let now = self.now();
        let role = access.membership.role;
        // The patch is applied first, so editing an item that stays closed fails with `item_closed`.
        let mut edited = current.apply_patch(&patch, principal.user_id, now)?;
        check_values(&parameters, patch.parameters.iter().map(|(p, o)| (*p, *o)))?;
        // A reopened item gets the values archiving would have given it had it been open.
        if let Some(reopened) = edited.as_mut().filter(|e| current.is_closed() && !e.is_closed()) {
            reopened.parameters = current_values(&parameters, &reopened.parameters);
        }
        // Checked after the patch, so an item that doesn't end up blocked gets `item_not_blocked` first.
        if let Some(blocker) = edited.as_ref().and_then(Item::blocked_by)
            && current.blocked_by() != Some(blocker)
        {
            check_blocker(&txn, &current, blocker).await?;
        }
        let Some(edited) = edited else {
            let steps = store::steps::list_steps(&txn, current.id).await?;
            return build_item_view(&txn, &access.heap, role, current, steps, now).await;
        };
        let saved =
            store::items::update_item(&txn, &edited, expected, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::ItemUpdated,
            &saved,
            diff_items(&current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let steps = store::steps::list_steps(&txn, saved.id).await?;
        let view = build_item_view(&txn, &access.heap, role, saved, steps, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(heap_id = %view.item.heap_id, item_id = %view.item.id, user_id = %principal.user_id, version = view.item.version.get(), "item updated");
        Ok(view)
    }

    pub(crate) fn item_audit_entry(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        item: &Item,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::Item,
            entity_id: *item.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

/// The view of `item`, with its `steps`, as `role` sees it in `heap` at `now`. It reads the heap's tags and
/// parameters, the blocker and the users, so it can run inside the transaction that wrote the item.
pub(crate) async fn build_item_view(
    db: &impl ConnectionTrait,
    heap: &Heap,
    role: Role,
    item: Item,
    steps: Vec<Step>,
    now: Timestamp,
) -> Result<ItemView, ServiceError> {
    let allowed_actions = allowed_item_actions(&item, heap, role, now);
    let stage_choices = allowed_stage_choices(&item, heap, role);
    let step_actions = allowed_step_actions(&item, heap, role);
    let tags = load_heap_tags(db, item.heap_id, &item.tags).await?;
    let parameters = store::parameters::list_parameters(db, item.heap_id).await?;
    let blocked_by = find_blocker(db, &item).await?;
    let (created_by, closed_by) = find_item_users(db, &item).await?;
    Ok(ItemView {
        parameters: describe_values(&item, &parameters),
        item,
        tags,
        blocked_by,
        created_by,
        closed_by,
        allowed_actions,
        stage_choices,
        steps,
        step_actions,
    })
}

/// The IDs of the items `view` lists in its ranked group, in order, with every item ranked first because a blocker
/// inherits from what it blocks even when that is left out.
fn ranked_group(
    view: &ListView,
    items: &[Item],
    policy: &HeapPolicy,
    parameters: &[Parameter],
    as_of: Timestamp,
) -> Vec<ItemId> {
    let inputs: Vec<ItemForRanking<'_>> = items.iter().map(ItemForRanking::from).collect();
    let ranking = rank(CURRENT_FORMULA, policy, parameters, &inputs, as_of);
    let by_id: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
    let mut group: Vec<&Item> = ranking
        .iter()
        .filter_map(|ranked| by_id.get(&ranked.item_id).copied())
        .filter(|item| view.group_of(item.state.kind()) == Some(Group::Ranked))
        .collect();
    view.sort(Group::Ranked, &mut group, |item| *item, parameters);
    group.into_iter().map(|item| item.id).collect()
}

/// `parameters` with the weights of `draft` in place of the stored ones. The draft's parameters must be unique and
/// in use.
fn apply_parameter_draft(
    parameters: &[Parameter],
    draft: &[ParameterWeightDraft],
) -> Result<Vec<Parameter>, ServiceError> {
    let listed: BTreeSet<ParameterId> = draft.iter().map(|d| d.id).collect();
    if listed.len() != draft.len() {
        return Err(ValidationError::DuplicateParameter.into());
    }
    let mut drafted = parameters.to_vec();
    for weight in draft {
        let parameter_id = weight.id;
        let parameter = drafted
            .iter_mut()
            .find(|p| p.id == parameter_id)
            .ok_or(ParameterError::NotInHeap { parameter_id })?;
        if parameter.is_archived() {
            return Err(ParameterError::Archived { parameter_id }.into());
        }
        parameter.weight = weight.weight;
    }
    Ok(drafted)
}

fn to_position(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

/// Checks that `item` may wait for `blocker`: a non-closed item of the same heap that doesn't wait, directly or
/// through others, for `item`.
///
/// The caller must hold the heap lock ([`store::heaps::lock_heap`]), so two writes that would close a cycle between
/// them can't both pass the check.
async fn check_blocker(db: &impl ConnectionTrait, item: &Item, blocker: ItemId) -> Result<(), ServiceError> {
    let mut link = store::items::find_item_summary_locking(db, item.heap_id, blocker)
        .await?
        .ok_or(ServiceError::BlockerNotInHeap { item_id: blocker })?;
    if link.state.kind().is_closed() {
        return Err(ServiceError::BlockerClosed { item_id: blocker });
    }
    let mut seen = HashSet::new();
    loop {
        if link.id == item.id {
            return Err(ServiceError::BlockingCycle { item_id: blocker });
        }
        // A cycle already in stored data that doesn't pass through `item` isn't this write's doing.
        if !seen.insert(link.id) {
            return Ok(());
        }
        let Some(next) = link.state.blocked_by() else {
            return Ok(());
        };
        match store::items::find_item_summary_locking(db, item.heap_id, next).await? {
            Some(found) => link = found,
            None => return Ok(()),
        }
    }
}

/// The item `item` waits for, if any.
async fn find_blocker(db: &impl ConnectionTrait, item: &Item) -> Result<Option<ItemSummary>, ServiceError> {
    let Some(id) = item.blocked_by() else {
        return Ok(None);
    };
    Ok(store::items::list_item_summaries(db, item.heap_id, [id]).await?.pop())
}

/// How many open items each of `heap_ids` has, in one query: those its list shows by default, before filters (the
/// `unfiltered_count` of [`ListView::default`]). Heaps with none are left out.
pub(crate) async fn count_open_items(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
) -> Result<HashMap<HeapId, u32>, StoreError> {
    store::items::count_items_by_heap(db, heap_ids, &ListView::default().listed_states()).await
}

/// How many of `items` `keep` is true for.
fn count_items(items: &[Item], keep: impl Fn(&Item) -> bool) -> u32 {
    u32::try_from(items.iter().filter(|item| keep(item)).count()).unwrap_or(u32::MAX)
}

/// Who added `item` and who closed it, each when it's known.
async fn find_item_users(db: &impl ConnectionTrait, item: &Item) -> Result<(Option<User>, Option<User>), ServiceError> {
    let users = load_item_users(db, std::slice::from_ref(item)).await?;
    Ok((
        pick_user(&users, item.created_by),
        pick_user(&users, item.state.closed_by()),
    ))
}

/// The users who added or closed `items`, by ID. A heap's items are added and closed by its team's members, so
/// these are few.
async fn load_item_users(db: &impl ConnectionTrait, items: &[Item]) -> Result<HashMap<UserId, User>, ServiceError> {
    let ids: Vec<UserId> = items
        .iter()
        .flat_map(|item| [item.created_by, item.state.closed_by()])
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let users = store::users::list_users(db, &ids).await?;
    Ok(users.into_iter().map(|user| (user.id, user)).collect())
}

/// The user `id` names among `users`, from [`load_item_users`].
fn pick_user(users: &HashMap<UserId, User>, id: Option<UserId>) -> Option<User> {
    id.and_then(|id| users.get(&id).cloned())
}

/// Every blocker of `items`, by ID. Blockers among `items` are taken from there; closed ones are loaded.
async fn blocker_summaries(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    items: &[Item],
) -> Result<HashMap<ItemId, ItemSummary>, ServiceError> {
    let loaded: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
    let mut found = HashMap::new();
    let mut missing = BTreeSet::new();
    for id in items.iter().filter_map(Item::blocked_by) {
        match loaded.get(&id) {
            Some(item) => {
                found.insert(id, ItemSummary::from(*item));
            }
            None => {
                missing.insert(id);
            }
        }
    }
    for summary in store::items::list_item_summaries(db, heap_id, missing).await? {
        found.insert(summary.id, summary);
    }
    Ok(found)
}

/// Fails unless every tag and option `view` filters by, and the parameter it sorts by, belong to the heap whose
/// tags are `heap_tags` and whose parameters, archived ones included, are `parameters`.
///
/// Archived parameters and options are allowed, so a saved view keeps working after they are archived.
pub(crate) fn ensure_view_in_heap(
    heap_tags: &[Tag],
    parameters: &[Parameter],
    view: &ListView,
) -> Result<(), ServiceError> {
    ensure_tags_in_heap(heap_tags, &view.filter.tags)?;
    let known = |option: &OptionId| parameters.iter().any(|p| p.find_option(*option).is_some());
    if let Some(&option_id) = view.filter.options.iter().find(|option| !known(option)) {
        return Err(ServiceError::OptionNotInHeap { option_id });
    }
    if let Some(parameter_id) = view.sort.parameter()
        && !parameters.iter().any(|p| p.id == parameter_id)
    {
        return Err(ParameterError::NotInHeap { parameter_id }.into());
    }
    Ok(())
}

/// The item's value for each of `parameters` in use, and for each archived one it has a value for, in the order of
/// `parameters`.
pub(crate) fn describe_values(item: &Item, parameters: &[Parameter]) -> Vec<ParameterValue> {
    parameters
        .iter()
        .filter_map(|parameter| {
            let option = item.parameters.get(&parameter.id).and_then(|id| {
                parameter.find_option(*id).map(|(label, archived)| OptionValue {
                    id: *id,
                    label: label.clone(),
                    archived,
                })
            });
            if parameter.is_archived() && option.is_none() {
                return None;
            }
            Some(ParameterValue {
                parameter_id: parameter.id,
                name: parameter.name.clone(),
                parameter_archived: parameter.is_archived(),
                option,
            })
        })
        .collect()
}

/// The tags `ids` names, by name. Fails with [`ServiceError::TagNotInHeap`] if any of them isn't a tag of
/// `heap_id`, which also covers IDs that don't exist at all.
pub(crate) async fn load_heap_tags(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: &BTreeSet<TagId>,
) -> Result<Vec<Tag>, ServiceError> {
    let tags = store::tags::list_tags_in_heap(db, heap_id, ids).await?;
    let found: BTreeSet<TagId> = tags.iter().map(|t| t.id).collect();
    if let Some(&tag_id) = ids.difference(&found).next() {
        return Err(ServiceError::TagNotInHeap { tag_id });
    }
    Ok(tags)
}

/// Fails with [`ServiceError::TagNotInHeap`] unless every tag in `ids` is one of `heap_tags`, the heap's tags.
///
/// Checked in memory, so a request with many IDs never becomes a query with as many parameters.
pub(crate) fn ensure_tags_in_heap(heap_tags: &[Tag], ids: &BTreeSet<TagId>) -> Result<(), ServiceError> {
    match ids.iter().find(|id| !heap_tags.iter().any(|tag| tag.id == **id)) {
        Some(&tag_id) => Err(ServiceError::TagNotInHeap { tag_id }),
        None => Ok(()),
    }
}

/// The tags among `heap_tags` (already sorted by name) that are in `ids`, in the same order.
fn select_tags(heap_tags: &[Tag], ids: &BTreeSet<TagId>) -> Vec<Tag> {
    heap_tags.iter().filter(|tag| ids.contains(&tag.id)).cloned().collect()
}

/// A random key no item of `heap_id` has. The caller holds the heap lock, so no other insert can take it first.
///
/// With 5,000 items in a heap about one draw in 5,000 is taken, so a second draw is rare.
async fn new_item_key(
    db: &impl ConnectionTrait,
    random: &dyn RandomSource,
    heap_id: HeapId,
) -> Result<ItemKey, ServiceError> {
    loop {
        let mut bytes = [0u8; ItemKey::LEN];
        random.fill(&mut bytes)?;
        let Some(key) = ItemKey::from_random(bytes) else {
            continue;
        };
        if !store::items::is_key_taken(db, heap_id, key).await? {
            return Ok(key);
        }
        tracing::debug!(%heap_id, "item key taken, drawing another");
    }
}
