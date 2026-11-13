//! Item use cases: the ranked list, the closed list, adding and editing.

use std::collections::{BTreeSet, HashMap, HashSet};

use domain::audit::{AuditAction, EntityType};
use domain::comment::Comment;
use domain::handle::ItemKey;
use domain::heap::{Heap, HeapPolicy, HeapPolicyPatch};
use domain::item::{BlockerGone, Item, ItemPatch, ItemSummary, Kind, NewItem, NewOrigin, Stage, StateKind};
use domain::list_view::{Group, ListView, RemindersMode};
use domain::parameter::{OptionLabel, Parameter, ParameterError, ParameterName, Weight, check_values, current_values};
use domain::permission::{
    ItemAction, Permission, QuestionAction, StepAction, allowed_item_actions, allowed_question_actions,
    allowed_stage_choices, allowed_step_actions, can_redo_turn,
};
use domain::question::{Question, QuestionState, waits_for_you};
use domain::ranking::{CURRENT_FORMULA, Contributor, HeldBack, ItemForRanking, RankedItem, rank};
use domain::run::{LAPSE_FAILURE, Run, RunError, RunForbidden};
use domain::step::{Step, StepProgress, progress};
use domain::tag::Tag;
use domain::team::{Membership, Role, Team};
use domain::turn::{Reach, TakeBackWhy, Turn, TurnSource, with_agent};
use domain::user::User;
use domain::validation::Field;
use domain::{
    Duration, HeapId, ItemId, OptionId, ParameterId, TagId, Timestamp, TurnId, UserId, ValidationError, Version,
};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

pub use store::items::ClosedCursor;

use crate::access::{authorize_heap, authorize_heap_write};
use crate::audit::{GoalMetHow, describe_new_item, describe_new_turn, describe_withdrawal, diff_freed, diff_items};
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
    /// The item it waits for, when it's blocked and one was picked, in whichever heap it is now.
    pub blocked_by: Option<ItemSummary>,
    /// How its blocker stopped holding it, when it did: closed, or moved to another heap. The item then waits for the
    /// person.
    pub blocker_gone: Option<GoneBlocker>,
    /// The items made from it, oldest first, in any state and heap: `Some` in a single-item response and in the
    /// closed list, `None` in the ranked list and other lists. A split makes a handful of items, and the closed list
    /// is at most [`MAX_CLOSED_PAGE`] rows, so one query reads them for the whole page.
    pub became: Option<Vec<ItemSummary>>,
    /// Who added it, when that is known (see [`Item::created_by`]).
    pub created_by: Option<User>,
    /// Who closed it. `None` when it's open or was closed before this was recorded (see [`domain::item::ItemState::closed_by`]).
    pub closed_by: Option<User>,
    pub allowed_actions: Vec<ItemAction>,
    /// The stages the caller may set on it now, in table order (see [`domain::permission::allowed_stage_choices`]).
    pub stage_choices: Vec<Stage>,
    /// A task's steps in order (empty for a note), or `None` in a list, which carries only `step_progress`: a
    /// context can be long, and a list row doesn't show it.
    pub steps: Option<Vec<Step>>,
    /// Ticked of open and ticked steps; `None` when there are none. Set from `steps` where they are loaded, and
    /// counted in storage where they aren't.
    pub step_progress: Option<StepProgress>,
    /// What the caller may do with each of the steps, the same for all of them (see
    /// [`domain::permission::allowed_step_actions`]).
    pub step_actions: Vec<StepAction>,
    /// The item's questions in number order, or `None` in a list, which doesn't carry them.
    pub questions: Option<Vec<QuestionView>>,
    /// The item's comments in number order, or `None` in a list, which doesn't carry them.
    pub comments: Option<Vec<CommentView>>,
    /// Whether the item is open, in an active heap, and has an open question, a stuck turn or a gone blocker (see
    /// [`domain::question::waits_for_you`]).
    pub waits_for_you: bool,
    /// The item's open turn, when an agent has been sent it and it hasn't been taken back. Closed items have none.
    pub turn: Option<TurnView>,
    /// Whether an agent has the item (see [`domain::turn::with_agent`]).
    pub with_agent: bool,
    /// The item this one was made from, when the caller may see the heap it is in. `None` otherwise, and in lists,
    /// which don't look it up: the item's own view does.
    pub origin: Option<ItemOrigin>,
}

/// The item an item was made from, with where it is.
#[derive(Clone, Debug)]
pub struct ItemOrigin {
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
}

/// A blocker that stopped holding the item it blocks, as Waits for you and Waiting for me show it.
#[derive(Clone, Debug)]
pub struct GoneBlocker {
    pub how: BlockerGone,
    /// The heap the blocker is in now, when that isn't the blocked item's heap: it was moved away.
    pub moved_to: Option<Heap>,
}

/// An open turn with the user who opened it.
#[derive(Clone, Debug)]
pub struct TurnView {
    pub turn: Turn,
    /// Who opened it, when that is known.
    pub opened_by: Option<User>,
    /// How many runs the turn has had, ended or not.
    pub runs: u32,
    /// Why the latest failed run failed, when the turn is stuck (see [`Turn::is_stuck`]).
    pub stuck_reason: Option<String>,
}

impl TurnView {
    /// Whether the turn is stuck.
    pub const fn is_stuck(&self) -> bool {
        self.turn.is_stuck()
    }
}

/// A question with the users it names and what the caller may do with it.
#[derive(Debug)]
pub struct QuestionView {
    pub question: Question,
    /// Who asked it, when that is known.
    pub asked_by: Option<User>,
    /// Who replied, when it's answered and that is known.
    pub replied_by: Option<User>,
    /// Who closed the item and so withdrew it, when it's withdrawn and that is known.
    pub withdrawn_by: Option<User>,
    pub allowed_actions: Vec<QuestionAction>,
}

/// A comment with the user who wrote it.
#[derive(Debug)]
pub struct CommentView {
    pub comment: Comment,
    /// Who wrote it. `None` only if the user can't be found, which users never being deleted rules out.
    pub created_by: Option<User>,
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
    /// The item's ranking score in its own heap, blocker inheritance included. Scores of different heaps compare
    /// as they are while every heap weighs the same ([`Services::list_ranked_across_heaps`]).
    pub score: f64,
    /// 1-based position within its group, in the order the group is listed.
    pub position: u32,
    pub contributors: Vec<Contributor>,
    pub held_back_by: Vec<HeldBack>,
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
    /// The heap's reminders to come that the filters keep, soonest first, when the view shows them. They have no
    /// position: they are not ranked until their date.
    pub reminders: Vec<ItemView>,
    /// Items the filters keep but whose state is hidden.
    pub hidden_count: u32,
    /// Items the view would list without its filters: those in pinned and shown states. Reminders to come aren't
    /// counted.
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
    /// Ranks every non-closed item of the heap except its reminders to come, then shows them as `view` says.
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
        let as_of = self.effective_as_of(requested_as_of, now);
        self.rank_heap_list(access.heap, access.membership.role, as_of, now, view)
            .await
    }

    /// The time a list ranks at: `requested_as_of` unless it's in the future or more than [`MAX_AS_OF_AGE`] old, then
    /// `now`.
    pub(crate) fn effective_as_of(&self, requested_as_of: Option<Timestamp>, now: Timestamp) -> Timestamp {
        requested_as_of
            .filter(|t| *t <= now && now.since(*t) <= MAX_AS_OF_AGE)
            .unwrap_or(now)
    }

    /// [`list_ranked`](Self::list_ranked) after the caller's right to view `heap` is settled: `role` is their role in
    /// its team, `as_of` the time to rank at and `now` the time their labels and actions are worked out at.
    pub(crate) async fn rank_heap_list(
        &self,
        heap: Heap,
        role: Role,
        as_of: Timestamp,
        now: Timestamp,
        view: &ListView,
    ) -> Result<RankedList, ServiceError> {
        let heap_id = heap.id;
        let OpenRanking {
            mut items,
            parameters,
            ranking,
            to_come,
        } = rank_open_items(&self.db, &heap, as_of, now).await?;
        let heap_tags = store::tags::list_tags(&self.db, heap_id).await?;
        ensure_view_in_heap(&heap_tags, &parameters, view)?;

        let item_count = items.len();
        let unfiltered_count = count_items(&items, |item| view.lists(item.state.kind()));
        let in_progress_count = count_items(&items, |item| item.state.kind() == StateKind::InProgress);
        // Reminders to come are listed only when the view shows them, by date, and the filters apply to them.
        let mut to_come: Vec<Item> = if view.reminders == RemindersMode::Shown {
            to_come
                .into_iter()
                .filter(|item| view.filter.matches(item, &parameters))
                .collect()
        } else {
            Vec::new()
        };
        to_come.sort_by(|a, b| {
            (a.arrives_at, a.created_at, a.key.as_str()).cmp(&(b.arrives_at, b.created_at, b.key.as_str()))
        });
        let mut blockers = blocker_summaries(&self.db, &items).await?;
        blockers.extend(blocker_summaries(&self.db, &to_come).await?);
        let gone = gone_blockers(&self.db, items.iter().chain(&to_come), &blockers).await?;
        let mut users = load_item_users(&self.db, &items).await?;
        users.extend(load_item_users(&self.db, &to_come).await?);

        // A list item's view, before the counts below are set on it.
        let make_view = |item: Item| {
            let stage_choices = allowed_stage_choices(&item, &heap, role);
            let step_actions = allowed_step_actions(&item, &heap, role);
            let tags = select_tags(&heap_tags, &item.tags);
            let values = describe_values(&item, &parameters);
            let blocked_by = item.blocked_by().and_then(|id| blockers.get(&id).cloned());
            let blocker_gone = gone.get(&item.id).cloned();
            let created_by = pick_user(&users, item.created_by);
            ItemView {
                item,
                tags,
                parameters: values,
                blocked_by,
                blocker_gone,
                // What an item became is read for one item or the closed list.
                became: None,
                created_by,
                // Listed items aren't closed, so they have no closer.
                closed_by: None,
                // Set below, once the open turns of the entries that are listed are known.
                allowed_actions: Vec::new(),
                stage_choices,
                // Lists carry the progress only, counted below for the entries that are listed.
                steps: None,
                step_progress: None,
                step_actions,
                questions: None,
                comments: None,
                // Set below, from one count for the entries that are listed.
                waits_for_you: false,
                turn: None,
                with_agent: false,
                origin: None,
            }
        };

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
            group.push(RankedEntry {
                view: make_view(item),
                score: entry.score,
                position: 0,
                contributors: entry.contributors,
                held_back_by: entry.held_back_by,
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
            // The item shown first is "Why it's #1", and nothing holds a first item back. Under another sort, such as
            // by due date, the fields don't move an item in the list, so none holds it back. The domain doesn't know
            // the list's filters, groups and sort, so this is where both are known.
            let cleared = if view.in_priority_order(group) {
                1
            } else {
                entries.len()
            };
            for entry in entries.iter_mut().take(cleared) {
                entry.held_back_by.clear();
            }
        }

        let mut reminders: Vec<ItemView> = to_come.into_iter().map(make_view).collect();
        let listed: Vec<ItemId> = [&pinned, &ranked, &blocked]
            .into_iter()
            .flatten()
            .map(|entry| entry.view.item.id)
            .chain(reminders.iter().map(|view| view.item.id))
            .collect();
        let mut step_progress = store::steps::count_step_progress(&self.db, &listed).await?;
        // Nothing waits, and no agent has anything, in an archived heap, so its items need no count.
        let (open_questions, mut open_turns) = if heap.is_archived() {
            (HashMap::new(), HashMap::new())
        } else {
            let turns = store::turns::list_open_turns(&self.db, &listed).await?;
            (
                store::questions::count_open_questions(&self.db, &listed).await?,
                turn_views(&self.db, turns.into_values(), now, false).await?,
            )
        };
        let views = pinned
            .iter_mut()
            .chain(&mut ranked)
            .chain(&mut blocked)
            .map(|entry| &mut entry.view)
            .chain(&mut reminders);
        for view in views {
            let id = view.item.id;
            let open_questions = open_questions.get(&id).copied().unwrap_or(0) as usize;
            let turn = open_turns.remove(&id);
            view.step_progress = step_progress.remove(&id);
            let stuck = turn.as_ref().is_some_and(TurnView::is_stuck);
            let blocker_gone = view.blocker_gone.is_some();
            view.waits_for_you = waits_for_you(&view.item, &heap, open_questions, stuck, blocker_gone);
            view.with_agent = with_agent(&view.item, &heap, open_questions, turn.is_some(), stuck, blocker_gone);
            view.allowed_actions = allowed_item_actions(&view.item, &heap, role, turn.is_some(), stuck, now);
            view.turn = turn;
        }

        tracing::debug!(%heap_id, items = item_count, as_of = as_of.as_millis(), "ranked heap");
        Ok(RankedList {
            heap,
            role,
            as_of,
            pinned,
            ranked,
            blocked,
            reminders,
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
        let (items, stored_parameters) = load_ranked_items(&self.db, heap_id, as_of).await?;
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
        let mut made = made_from_by_origin(&store::items::list_items_made_from(&self.db, &item_ids, false).await?);
        let mut step_progress = store::steps::count_step_progress(&self.db, &item_ids).await?;
        let now = self.now();
        let role = access.membership.role;
        let items = items
            .into_iter()
            .map(|item| {
                // Closed items have no open turn: closing ends it.
                let allowed_actions = allowed_item_actions(&item, &access.heap, role, false, false, now);
                let stage_choices = allowed_stage_choices(&item, &access.heap, role);
                let step_actions = allowed_step_actions(&item, &access.heap, role);
                let step_progress = step_progress.remove(&item.id);
                let tags = select_tags(&heap_tags, &item.tags);
                let values = describe_values(&item, &parameters);
                let created_by = pick_user(&users, item.created_by);
                let closed_by = pick_user(&users, item.state.closed_by());
                let became = made.remove(&item.id).unwrap_or_default();
                // Only blocked items have a blocker, and closed items aren't blocked.
                ItemView {
                    item,
                    tags,
                    parameters: values,
                    blocked_by: None,
                    blocker_gone: None,
                    became: Some(became),
                    created_by,
                    closed_by,
                    allowed_actions,
                    stage_choices,
                    steps: None,
                    step_progress,
                    step_actions,
                    questions: None,
                    comments: None,
                    // Closed items never wait, and no agent has them.
                    waits_for_you: false,
                    turn: None,
                    with_agent: false,
                    origin: None,
                }
            })
            .collect();
        Ok(ClosedPage {
            heap: access.heap,
            items,
            next,
        })
    }

    /// Adds an item to the heap: open, or blocked when `fields.blocked_by` names the item it waits for. Every tag must
    /// belong to the heap, or it fails with [`ServiceError::TagNotInHeap`]. Every value must be an option in use of a
    /// parameter in use of the heap ([`ParameterError::NotInHeap`], [`ParameterError::OptionNotInParameter`]). The
    /// blocker is checked as an edit's is ([`ServiceError::BlockerNotInHeap`], [`ServiceError::BlockerClosed`]).
    ///
    /// With `fields.origin` the item is made from another, in a split: the origin must be an open note of this heap
    /// ([`ServiceError::OriginNotInHeap`], [`ServiceError::OriginNotANote`], [`ServiceError::OriginClosed`]) and not
    /// made by a reminder (a validation error on the origin). With age [`Age::Keep`] the new item's age counts from the
    /// origin's.
    ///
    /// In a run (`check_run_add`) the item needs a context and isn't a reminder. Made from an origin, that must be
    /// the run's own item, with a need up to its own. It may wait only for an item a run of the same turn made from
    /// that item (Runs). It gets no prepare turn.
    pub async fn create_item(
        &self,
        principal: Principal,
        heap_id: HeapId,
        mut fields: NewItem,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let now = self.now();
        // In a run the heaps lock first, in heap ID order, then the run is read and checked live.
        let run = self.check_run_add(&txn, principal, heap_id, &fields, now).await?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::EditItems).await?;
        let origin_item = match fields.origin {
            Some(origin) => Some(self.find_origin(&txn, &fields, heap_id, origin).await?),
            None => None,
        };
        if let Some(run) = &run {
            // A run splits only its own item, and gives no need above it (Moves §2 and §3).
            if let Some(origin) = &origin_item {
                if origin.id != run.item_id {
                    let held = store::items::find_item(&txn, run.item_id)
                        .await?
                        .ok_or(ServiceError::NotFound)?;
                    return Err(RunError::WrongItem { key: held.key }.into());
                }
                if fields.need > origin.need {
                    return Err(run_forbidden(RunForbidden::NeedAboveOrigin { highest: origin.need }));
                }
            }
            if let Some(blocker) = fields.blocked_by {
                ensure_made_in_turn(&txn, blocker, run).await?;
            }
        }
        let tags = load_heap_tags(&txn, heap_id, &fields.tags).await?;
        let parameters = store::parameters::list_parameters_locking(&txn, heap_id).await?;
        check_values(&parameters, fields.parameters.iter().map(|(p, o)| (*p, Some(*o))))?;

        if let (Some(origin), Some(item)) = (fields.origin, &origin_item) {
            fields.arrives_at = origin.age.arrives_at(item);
        }
        let blocker = fields.blocked_by;
        let key = new_item_key(&txn, &*self.random, heap_id).await?;
        let item = Item::new(
            ItemId::from_uuid(self.new_id(now)),
            heap_id,
            key,
            fields,
            principal.user_id,
            now,
        );
        if let Some(blocker) = blocker {
            check_blocker(&txn, &item, blocker).await?;
        }
        let item = match &run {
            Some(run) => item.added_in_run(run.item_id, run.id),
            None => item,
        };
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
        // A heap set so agents prepare gives every new item a turn of one state, whoever added it (Work with agents,
        // Opening a turn). A new task's turn goes to defined, never to done. It acts only here, on create, so it never
        // opens a second turn on an item.
        // A reminder gets none: no turn opens on one. Nor does an item a run adds: it is already prepared (Opening a
        // turn).
        let opened = if access.heap.agents_prepare && item.kind != Kind::Reminder && run.is_none() {
            let turn = Turn::open(
                TurnId::from_uuid(self.new_id(now)),
                &item,
                Reach::One,
                None,
                TurnSource::Prepare,
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
            Some(turn)
        } else {
            None
        };
        let created_by = store::users::find_user(&txn, principal.user_id).await?;
        let blocked_by = find_blocker(&txn, &item).await?;
        let origin = find_origin(&txn, principal.user_id, &item).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            %heap_id,
            item_id = %item.id,
            user_id = %principal.user_id,
            run_id = ?item.created_run_id,
            origin = ?item.origin_id,
            blocked = blocked_by.is_some(),
            prepared = opened.is_some(),
            "item created"
        );
        // The creator opened the turn, if there is one.
        let turn = opened.map(|turn| TurnView {
            turn,
            opened_by: created_by.clone(),
            runs: 0,
            stuck_reason: None,
        });
        let allowed_actions =
            allowed_item_actions(&item, &access.heap, access.membership.role, turn.is_some(), false, now);
        let stage_choices = allowed_stage_choices(&item, &access.heap, access.membership.role);
        let step_actions = allowed_step_actions(&item, &access.heap, access.membership.role);
        let values = describe_values(&item, &parameters);
        // New items have no closer, no steps and nothing made from them, and their blocker is not gone.
        Ok(ItemView {
            became: Some(Vec::new()),
            item,
            tags,
            parameters: values,
            blocked_by,
            blocker_gone: None,
            created_by,
            closed_by: None,
            allowed_actions,
            stage_choices,
            steps: Some(Vec::new()),
            step_progress: None,
            step_actions,
            questions: Some(Vec::new()),
            comments: Some(Vec::new()),
            waits_for_you: false,
            // A new item has no questions, so an open turn is an agent having it.
            with_agent: turn.is_some(),
            turn,
            origin,
        })
    }

    /// The open note `origin` names, read under the heap lock, for an item about to be made in `heap_id` from it.
    async fn find_origin(
        &self,
        txn: &impl ConnectionTrait,
        fields: &NewItem,
        heap_id: HeapId,
        origin: NewOrigin,
    ) -> Result<Item, ServiceError> {
        if fields.kind == Kind::Reminder {
            return Err(ValidationError::FieldInvalid { field: Field::Origin }.into());
        }
        let item_id = origin.item_id;
        let found = store::items::find_item_locking(txn, item_id)
            .await?
            .filter(|found| found.heap_id == heap_id)
            .ok_or(ServiceError::OriginNotInHeap { item_id })?;
        if found.kind != Kind::Note {
            return Err(ServiceError::OriginNotANote { item_id });
        }
        if found.is_closed() {
            return Err(ServiceError::OriginClosed { item_id });
        }
        Ok(found)
    }

    pub async fn find_item(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let item = store::items::find_item(&self.db, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap(&self.db, principal, item.heap_id, Permission::ViewHeap).await?;
        let steps = store::steps::list_steps(&self.db, item.id).await?;
        let questions = store::questions::list_questions(&self.db, item.id).await?;
        let comments = store::comments::list_comments(&self.db, item.id).await?;
        let open_turn = store::turns::find_open_turn(&self.db, item.id).await?;
        build_item_view(
            &self.db,
            &access.heap,
            &access.membership,
            item,
            ItemChildren {
                steps,
                questions,
                comments,
                open_turn,
                locking: false,
            },
            self.now(),
        )
        .await
    }

    /// Applies a partial edit if the item is still at version `expected`. New tags must belong to the item's
    /// heap, or it fails with [`ServiceError::TagNotInHeap`]. A new blocker must be a non-closed item of the same
    /// heap that doesn't already wait for this one ([`ServiceError::BlockerNotInHeap`],
    /// [`ServiceError::BlockerClosed`], [`ServiceError::BlockingCycle`]). A value set must be an option in use of a
    /// parameter in use of the heap; a value cleared may be of any of its parameters.
    ///
    /// Closing an item as done frees the items of its heap it blocks: they go to open, each with its own entry. Closing
    /// it as won't do leaves them blocked, and they wait for the person ([`Item::blocker_gone`]).
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
        let access = authorize_heap_write(&txn, principal, current.heap_id, Permission::EditItems)
            .await
            .map_err(|error| in_run(principal, error))?;
        if principal.run.is_some() {
            check_run_patch(&patch)?;
            self.check_run_write(&txn, principal, item_id, self.now()).await?;
        }
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
            let children = ItemChildren::read_locking(&txn, current.id).await?;
            let view = build_item_view(&txn, &access.heap, &access.membership, current, children, now).await?;
            // A write in a run that changes nothing still tells the run it was heard.
            commit_if_in_run(txn, principal).await?;
            return Ok(view);
        };
        // Starting or closing an item takes it back from the agent first, and closing withdraws its open questions;
        // each has its own entry, written before the item's so the history reads in the order things happened.
        let mut turns_taken_back = 0_usize;
        let mut questions_withdrawn = 0_usize;
        if principal.run.is_some() {
            // An agent that starts the item it works on keeps its turn (design, Queue). Closing isn't allowed in a
            // run, so there is nothing else to take back.
        } else if edited.state.kind() == StateKind::InProgress && current.state.kind() != StateKind::InProgress {
            turns_taken_back = self
                .take_back_open_turn(&txn, &access.heap, principal, current.id, TakeBackWhy::Started, now)
                .await?
                .into();
        } else if edited.is_closed() && !current.is_closed() {
            (turns_taken_back, questions_withdrawn) = self
                .end_work_for_close(&txn, &access.heap, principal, &current, TurnOnClose::TakeBack, now)
                .await?;
        }
        let (saved, freed) = self
            .save_edited_item(&txn, &access.heap, principal, &current, &edited, now)
            .await?;
        // A person setting the stage to a turn's goal or past it is the agent's work done by hand: the turn ends as
        // reached, after the item's entry so the history reads in order. Starting or closing took the turn back above.
        if principal.run.is_none() && saved.stage != current.stage {
            let open = store::turns::find_open_turn_locking(&txn, saved.id)
                .await?
                .filter(|turn| turn.goal_met_by(saved.stage));
            if let Some(turn) = open {
                self.end_met_turn(
                    &txn,
                    &access.heap,
                    principal,
                    &turn,
                    &saved,
                    current.stage,
                    GoalMetHow::StageSet,
                    now,
                )
                .await?;
            }
        }
        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, &access.heap, &access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(heap_id = %view.item.heap_id, item_id = %view.item.id, user_id = %principal.user_id, version = view.item.version.get(), turns_taken_back, questions_withdrawn, freed, "item updated");
        Ok(view)
    }

    /// What closing `current` ends: its open turn, which `on_turn` says how, and its open questions, withdrawn. Says how
    /// many turns it ended and how many questions it withdrew. The caller holds the heap lock and goes on to
    /// [`Self::save_edited_item`], so the history reads in the order things happened.
    pub(crate) async fn end_work_for_close(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        current: &Item,
        on_turn: TurnOnClose,
        now: Timestamp,
    ) -> Result<(usize, usize), ServiceError> {
        let turns_ended = match on_turn {
            TurnOnClose::TakeBack => self
                .take_back_open_turn(txn, heap, principal, current.id, TakeBackWhy::Closed, now)
                .await?
                .into(),
            TurnOnClose::Reached => self.reach_open_turn(txn, heap, principal, current, now).await?.into(),
        };
        let withdrawn = self.withdraw_open_questions(txn, heap, principal, current, now).await?;
        Ok((turns_ended, withdrawn))
    }

    /// Writes `edited`, the new form of `current`, with its `item_updated` entry, if `current` is still the stored
    /// version. An item closed as done frees the items it blocks first. Says the saved item and how many
    /// items it freed. The caller holds the heap lock.
    pub(crate) async fn save_edited_item(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        current: &Item,
        edited: &Item,
        now: Timestamp,
    ) -> Result<(Item, usize), ServiceError> {
        // The caller checked that `current` is at the version the request expected.
        let expected = current.version;
        let freed = if edited.state.kind() == StateKind::Done && !current.is_closed() {
            self.free_blocked_items(txn, heap, principal, current, now).await?
        } else {
            0
        };
        let saved =
            store::items::update_item(txn, edited, expected, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::ItemUpdated,
            &saved,
            diff_items(current, &saved),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok((saved, freed))
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
        self.item_audit_entry_for(heap, principal, action, item.id, diff, now)
    }

    /// [`Self::item_audit_entry`] for an item known by its ID.
    pub(crate) fn item_audit_entry_for(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        item_id: ItemId,
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
            entity_id: *item_id.as_uuid(),
            diff,
            run_id: principal.run,
            created_at: now,
        }
    }

    /// Frees every item of `heap` that `blocker`, which is being closed as done, blocks: each goes to open, gets its next
    /// version, and an `item_updated` entry naming the blocker as the reason. Says how many it freed. The caller holds
    /// the heap lock.
    async fn free_blocked_items(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        blocker: &Item,
        now: Timestamp,
    ) -> Result<usize, ServiceError> {
        let blocked = store::items::list_items_blocked_by_locking(txn, heap.id, blocker.id).await?;
        for before in &blocked {
            let after = store::items::update_item(txn, &before.freed(), before.version, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected: before.version,
                    current: before.version.next(),
                })?;
            let entry = self.item_audit_entry(
                heap,
                principal,
                AuditAction::ItemUpdated,
                &after,
                diff_freed(before, &after, blocker),
                now,
            );
            store::audit::insert_audit_entry(txn, &entry).await?;
        }
        Ok(blocked.len())
    }

    /// Withdraws every open question of `item`, which is being closed: each gets its next version and a
    /// `question_withdrawn` entry. Answered questions stay as they are. Says how many it withdrew. The caller holds
    /// the heap lock.
    pub(crate) async fn withdraw_open_questions(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        item: &Item,
        now: Timestamp,
    ) -> Result<usize, ServiceError> {
        let questions = store::questions::list_questions_locking(txn, item.id).await?;
        let mut withdrawn_count = 0;
        for question in questions.iter().filter(|q| q.state() == QuestionState::Open) {
            let withdrawn = question.withdraw(principal.user_id, now);
            store::questions::update_question(txn, &withdrawn, question.version)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected: question.version,
                    current: question.version.next(),
                })?;
            let entry = self.item_audit_entry(
                heap,
                principal,
                AuditAction::QuestionWithdrawn,
                item,
                describe_withdrawal(question, TakeBackWhy::Closed),
                now,
            );
            store::audit::insert_audit_entry(txn, &entry).await?;
            withdrawn_count += 1;
        }
        Ok(withdrawn_count)
    }
}

/// How closing an item ends its open turn.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TurnOnClose {
    /// The person closed it: the agent's turn is taken back.
    TakeBack,
    /// The person agreed to a done proposal: the turn reached its goal.
    Reached,
}

/// What an item owns in rows of their own, which a single-item response carries whole.
pub(crate) struct ItemChildren {
    pub steps: Vec<Step>,
    pub questions: Vec<Question>,
    pub comments: Vec<Comment>,
    /// The item's open turn, if it has one.
    pub open_turn: Option<Turn>,
    /// Whether these were read with locking reads after the heap lock, so the turn's runs are read the same way.
    /// A plain read, such as `GET` of the item, takes no lock.
    pub locking: bool,
}

impl ItemChildren {
    /// The steps, questions, comments and open turn of `item_id` as they are now, with locking reads (see
    /// [`store::steps::list_steps_locking`]). A write calls it after the heap lock.
    pub(crate) async fn read_locking(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Self, StoreError> {
        Ok(Self {
            steps: store::steps::list_steps_locking(db, item_id).await?,
            questions: store::questions::list_questions_locking(db, item_id).await?,
            comments: store::comments::list_comments_locking(db, item_id).await?,
            open_turn: store::turns::find_open_turn_locking(db, item_id).await?,
            locking: true,
        })
    }
}

/// The item `item` was made from, when it has one and `viewer` may view the heap it is in. An origin the viewer can't
/// see reads as none, as one that is gone does, so a heap's existence isn't shown to people outside it.
async fn find_origin(
    db: &impl ConnectionTrait,
    viewer: UserId,
    item: &Item,
) -> Result<Option<ItemOrigin>, ServiceError> {
    let Some(origin_id) = item.origin_id else {
        return Ok(None);
    };
    let Some(origin) = store::items::find_item(db, origin_id).await? else {
        return Ok(None);
    };
    let Some(heap) = store::heaps::find_heap(db, origin.heap_id).await? else {
        return Ok(None);
    };
    let may_view = store::teams::find_membership(db, heap.team_id, viewer)
        .await?
        .is_some_and(|membership| Permission::ViewHeap.is_granted_to(membership.role));
    if !may_view {
        return Ok(None);
    }
    let team = store::teams::find_team(db, heap.team_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(Some(ItemOrigin {
        item: origin,
        heap,
        team,
    }))
}

/// The view of `item`, with its steps, questions and comments, as `role` sees it in `heap` at `now`. It reads the heap's tags
/// and parameters, the blocker and the users, so it can run inside the transaction that wrote the item.
pub(crate) async fn build_item_view(
    db: &impl ConnectionTrait,
    heap: &Heap,
    membership: &Membership,
    item: Item,
    children: ItemChildren,
    now: Timestamp,
) -> Result<ItemView, ServiceError> {
    let role = membership.role;
    let ItemChildren {
        steps,
        questions,
        comments,
        open_turn,
        locking,
    } = children;
    // The turn reads with a lapsed run counted as failed, so everything below agrees on whether it is stuck.
    let turn = match open_turn {
        Some(turn) => turn_views(db, vec![turn], now, locking).await?.remove(&item.id),
        None => None,
    };
    let stuck = turn.as_ref().is_some_and(TurnView::is_stuck);
    let mut allowed_actions = allowed_item_actions(&item, heap, role, turn.is_some(), stuck, now);
    // Changes can be asked only of an item with no turn open, which is the only one worth the extra read. Lists don't
    // offer it: they would need the latest turn of every row.
    if turn.is_none() && !item.is_closed() && item.kind != Kind::Reminder {
        let latest = store::turns::find_latest_turn(db, item.id, locking).await?;
        if can_redo_turn(&item, heap, role, latest.as_ref()) {
            allowed_actions.push(ItemAction::RedoTurn);
        }
    }
    let stage_choices = allowed_stage_choices(&item, heap, role);
    let step_actions = allowed_step_actions(&item, heap, role);
    let tags = load_heap_tags(db, item.heap_id, &item.tags).await?;
    let parameters = store::parameters::list_parameters(db, item.heap_id).await?;
    let blocked_by = find_blocker(db, &item).await?;
    let blocker_gone = match &blocked_by {
        Some(blocker) => {
            let blockers = HashMap::from([(blocker.id, blocker.clone())]);
            gone_blockers(db, [&item], &blockers).await?.remove(&item.id)
        }
        None => None,
    };
    let became = made_from_by_origin(&store::items::list_items_made_from(db, &[item.id], locking).await?)
        .remove(&item.id)
        .unwrap_or_default();
    let (created_by, closed_by) = find_item_users(db, &item).await?;
    let open_questions = questions.iter().filter(|q| q.state() == QuestionState::Open).count();
    let waits_for_you = waits_for_you(&item, heap, open_questions, stuck, blocker_gone.is_some());
    let with_agent = with_agent(
        &item,
        heap,
        open_questions,
        turn.is_some(),
        stuck,
        blocker_gone.is_some(),
    );
    let questions = build_question_views(db, heap, role, &item, questions).await?;
    let comments = build_comment_views(db, comments).await?;
    let origin = find_origin(db, membership.user_id, &item).await?;
    Ok(ItemView {
        origin,
        parameters: describe_values(&item, &parameters),
        item,
        tags,
        blocked_by,
        blocker_gone,
        became: Some(became),
        created_by,
        closed_by,
        allowed_actions,
        stage_choices,
        step_progress: progress(&steps),
        steps: Some(steps),
        step_actions,
        questions: Some(questions),
        comments: Some(comments),
        waits_for_you,
        turn,
        with_agent,
    })
}

/// The open `turns` with the users who opened them, by item, in one query each for the users, the runs, the runs that
/// went quiet and the failures. Turns are opened by the team's members, so the users are few.
///
/// Each view's turn is read as of `now` ([`Turn::counting_lapse`]): a turn with a lapsed run counts it as failed, so
/// it may be stuck though its stored count says otherwise, and its reason is the lapse. `lock` makes the read of the
/// runs a locking one, for a caller that has taken the heap lock.
pub(crate) async fn turn_views(
    db: &impl ConnectionTrait,
    turns: impl IntoIterator<Item = Turn>,
    now: Timestamp,
    lock: bool,
) -> Result<HashMap<ItemId, TurnView>, ServiceError> {
    let turns: Vec<Turn> = turns.into_iter().collect();
    let ids: Vec<UserId> = turns
        .iter()
        .filter_map(|turn| turn.opened_by)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let users: HashMap<UserId, User> = store::users::list_users(db, &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect();
    let turn_ids: Vec<TurnId> = turns.iter().map(|turn| turn.id).collect();
    let lapsed: HashSet<TurnId> = store::runs::list_unended_runs_for_turns(db, &turn_ids, lock)
        .await?
        .into_iter()
        .filter(|run| run.is_lapsed(now))
        .map(|run| run.turn_id)
        .collect();
    let turns: Vec<Turn> = turns
        .iter()
        .map(|turn| turn.counting_lapse(lapsed.contains(&turn.id)))
        .collect();
    // A lapsed run is the latest failure there is, and isn't stored yet.
    let stuck_ids: Vec<TurnId> = turns
        .iter()
        .filter(|turn| turn.is_stuck() && !lapsed.contains(&turn.id))
        .map(|turn| turn.id)
        .collect();
    let runs = store::runs::count_runs_for_turns(db, &turn_ids).await?;
    let mut reasons = store::runs::latest_failure_for_turns(db, &stuck_ids).await?;
    Ok(turns
        .into_iter()
        .map(|turn| {
            let opened_by = pick_user(&users, turn.opened_by);
            let stuck_reason = if lapsed.contains(&turn.id) {
                Some(LAPSE_FAILURE.to_owned())
            } else {
                reasons.remove(&turn.id)
            };
            let view = TurnView {
                opened_by,
                runs: runs.get(&turn.id).copied().unwrap_or(0),
                stuck_reason,
                turn,
            };
            (view.turn.item_id, view)
        })
        .collect())
}

/// `questions` of `item` with the users who asked and replied, and what `role` may do with each. The questions of
/// an item are few ([`domain::question::MAX_QUESTIONS`]) and so are the people who answer them.
async fn build_question_views(
    db: &impl ConnectionTrait,
    heap: &Heap,
    role: Role,
    item: &Item,
    questions: Vec<Question>,
) -> Result<Vec<QuestionView>, ServiceError> {
    let ids: Vec<UserId> = questions
        .iter()
        .flat_map(|question| {
            [
                question.asked_by,
                question.reply.as_ref().and_then(|reply| reply.replied_by),
                question
                    .withdrawal
                    .as_ref()
                    .and_then(|withdrawal| withdrawal.withdrawn_by),
            ]
        })
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let users: HashMap<UserId, User> = store::users::list_users(db, &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect();
    Ok(questions
        .into_iter()
        .map(|question| QuestionView {
            allowed_actions: allowed_question_actions(item, heap, role, question.state()),
            asked_by: pick_user(&users, question.asked_by),
            replied_by: pick_user(&users, question.reply.as_ref().and_then(|reply| reply.replied_by)),
            withdrawn_by: pick_user(
                &users,
                question
                    .withdrawal
                    .as_ref()
                    .and_then(|withdrawal| withdrawal.withdrawn_by),
            ),
            question,
        })
        .collect())
}

/// `comments` of an item with the users who wrote them. The comments of an item are bounded
/// ([`domain::comment::MAX_COMMENTS`]) and so are the people who write them.
async fn build_comment_views(
    db: &impl ConnectionTrait,
    comments: Vec<Comment>,
) -> Result<Vec<CommentView>, ServiceError> {
    let ids: Vec<UserId> = comments
        .iter()
        .map(|comment| comment.created_by)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let users: HashMap<UserId, User> = store::users::list_users(db, &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect();
    Ok(comments
        .into_iter()
        .map(|comment| CommentView {
            created_by: pick_user(&users, Some(comment.created_by)),
            comment,
        })
        .collect())
}

/// The open items of the heap (open, in progress, blocked) split into those that rank and its reminders to come at
/// `now`, and its parameters in use. This is where reminders to come leave the ranking: every list, the agents'
/// queue and Waiting for me read the heap's open items through it. The split is at the server's `now`, not at the
/// time a list ranks at, so a reminder whose date has come is ranked even in a list frozen from before it.
async fn load_open_items(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    now: Timestamp,
) -> Result<(Vec<Item>, Vec<Item>, Vec<Parameter>), StoreError> {
    let (to_come, items) = store::items::list_items_in_states(db, heap_id, &StateKind::OPEN)
        .await?
        .into_iter()
        .partition(|item| item.is_to_come(now));
    let parameters = store::parameters::list_parameters(db, heap_id).await?;
    Ok((items, to_come, parameters))
}

/// [`load_open_items`] without the reminders to come.
async fn load_ranked_items(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    now: Timestamp,
) -> Result<(Vec<Item>, Vec<Parameter>), StoreError> {
    let (items, _, parameters) = load_open_items(db, heap_id, now).await?;
    Ok((items, parameters))
}

/// A heap's open items, ranked.
pub(crate) struct OpenRanking {
    /// The items that rank, which is every open item but the reminders to come.
    pub items: Vec<Item>,
    pub parameters: Vec<Parameter>,
    pub ranking: Vec<RankedItem>,
    /// The heap's reminders to come: out of the ranking until their date, in no particular order.
    pub to_come: Vec<Item>,
}

/// Ranks every open item of the heap that isn't a reminder to come at `now`, at `as_of` and under its stored
/// weights, as its list does, and returns the items and parameters with the ranking, and the reminders left out.
/// Every ranked item is ranked, because a blocker inherits from what it blocks even when that is left out of what a
/// caller shows. The one place that says how a heap's current order is made, so no list drifts from another.
pub(crate) async fn rank_open_items(
    db: &impl ConnectionTrait,
    heap: &Heap,
    as_of: Timestamp,
    now: Timestamp,
) -> Result<OpenRanking, StoreError> {
    let (items, to_come, parameters) = load_open_items(db, heap.id, now).await?;
    let inputs: Vec<ItemForRanking<'_>> = items.iter().map(ItemForRanking::from).collect();
    let ranking = rank(CURRENT_FORMULA, &heap.policy, &parameters, &inputs, as_of);
    Ok(OpenRanking {
        items,
        parameters,
        ranking,
        to_come,
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

/// The item `item` waits for, if any, in whichever heap it is now.
async fn find_blocker(db: &impl ConnectionTrait, item: &Item) -> Result<Option<ItemSummary>, ServiceError> {
    let Some(id) = item.blocked_by() else {
        return Ok(None);
    };
    Ok(store::items::list_blockers(db, [id]).await?.pop())
}

/// How the blocker of each of `items` whose blocker is gone stopped holding it, by the blocked item's ID, with the
/// heap a moved blocker is in now. `blockers` holds their blockers by ID ([`blocker_summaries`]). The heaps blockers
/// moved to are few, as a team has few heaps, and are read once each.
pub(crate) async fn gone_blockers<'a>(
    db: &impl ConnectionTrait,
    items: impl IntoIterator<Item = &'a Item>,
    blockers: &HashMap<ItemId, ItemSummary>,
) -> Result<HashMap<ItemId, GoneBlocker>, ServiceError> {
    let gone: Vec<(&Item, &ItemSummary, BlockerGone)> = items
        .into_iter()
        .filter_map(|item| {
            let blocker = blockers.get(&item.blocked_by()?)?;
            Some((item, blocker, item.blocker_gone(blocker)?))
        })
        .collect();
    let mut heaps: HashMap<HeapId, Heap> = HashMap::new();
    for (item, blocker, _) in &gone {
        if blocker.heap_id != item.heap_id
            && !heaps.contains_key(&blocker.heap_id)
            && let Some(heap) = store::heaps::find_heap(db, blocker.heap_id).await?
        {
            heaps.insert(heap.id, heap);
        }
    }
    Ok(gone
        .into_iter()
        .map(|(item, blocker, how)| {
            let moved_to = (blocker.heap_id != item.heap_id)
                .then(|| heaps.get(&blocker.heap_id).cloned())
                .flatten();
            (item.id, GoneBlocker { how, moved_to })
        })
        .collect())
}

/// How many open items each of `heap_ids` has at `now`, in one query: those its list shows by default, before
/// filters (the `unfiltered_count` of [`ListView::default`]), so without reminders to come. Heaps with none are left
/// out.
pub(crate) async fn count_open_items(
    db: &impl ConnectionTrait,
    heap_ids: &[HeapId],
    now: Timestamp,
) -> Result<HashMap<HeapId, u32>, StoreError> {
    store::items::count_items_by_heap(db, heap_ids, &ListView::default().listed_states(), now).await
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

/// Every blocker of `items`, by ID. Blockers among `items` are taken from there; closed ones, and ones moved to another
/// heap, are loaded.
pub(crate) async fn blocker_summaries(
    db: &impl ConnectionTrait,
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
    for summary in store::items::list_blockers(db, missing).await? {
        found.insert(summary.id, summary);
    }
    Ok(found)
}

/// `made`, the items made from other items, grouped under the item each was made from, in the order given.
pub(crate) fn made_from_by_origin(made: &[Item]) -> HashMap<ItemId, Vec<ItemSummary>> {
    let mut grouped: HashMap<ItemId, Vec<ItemSummary>> = HashMap::new();
    for item in made {
        if let Some(origin) = item.origin_id {
            grouped.entry(origin).or_default().push(ItemSummary::from(item));
        }
    }
    grouped
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
pub(crate) async fn new_item_key(
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

/// Fails with [`RunForbidden::BlockerNotMade`] unless `blocker` is an item a run of `run`'s turn made from the run's
/// item (Blocks §1: an agent sets blocks only among the items it made in the same turn). A run that failed and was
/// tried again can still chain on what the first one made.
async fn ensure_made_in_turn(db: &impl ConnectionTrait, blocker: ItemId, run: &Run) -> Result<(), ServiceError> {
    let made_by = store::items::find_item_locking(db, blocker)
        .await?
        .filter(|found| found.origin_id == Some(run.item_id))
        .and_then(|found| found.created_run_id);
    let in_turn = match made_by {
        Some(maker) if maker == run.id => true,
        Some(maker) => store::runs::find_run(db, maker)
            .await?
            .is_some_and(|maker| maker.turn_id == run.turn_id),
        None => false,
    };
    if in_turn {
        Ok(())
    } else {
        Err(run_forbidden(RunForbidden::BlockerNotMade))
    }
}

/// A write only the person makes, tried in a run.
const fn forbidden() -> ServiceError {
    run_forbidden(RunForbidden::Person)
}

/// A write a run is refused, for `reason`.
const fn run_forbidden(reason: RunForbidden) -> ServiceError {
    ServiceError::Run(RunError::Forbidden(reason))
}

/// Refuses a patch made in a run unless it only sets the context, makes a note or a reminder a task, or starts the
/// item (design, Queue and Runs). Any other field, or any other state, refuses the whole patch.
fn check_run_patch(patch: &ItemPatch) -> Result<(), ServiceError> {
    match patch.state {
        None | Some(StateKind::InProgress) => {}
        Some(StateKind::Done) => return Err(RunError::Forbidden(RunForbidden::Done).into()),
        Some(_) => return Err(forbidden()),
    }
    if patch.kind.is_some_and(|kind| kind != Kind::Task) {
        return Err(forbidden());
    }
    let other_fields = patch.title.is_some()
        || patch.need.is_some()
        || patch.stage.is_some()
        || patch.effort.is_some()
        || patch.impact.is_some()
        || patch.due.is_some()
        || patch.tags.is_some()
        || !patch.parameters.is_empty()
        || patch.blocked_by.is_some();
    if other_fields {
        return Err(forbidden());
    }
    Ok(())
}

/// An archived heap read as a run's: its turn was taken back when the heap was archived, so the run is over.
pub(crate) fn in_run(principal: Principal, error: ServiceError) -> ServiceError {
    match error {
        ServiceError::HeapArchived { .. } if principal.run.is_some() => RunError::Over.into(),
        other => other,
    }
}

/// Commits `txn` when the request is made in a run, so what the run's check wrote (that it was heard) is kept by a
/// write that changed nothing. Other requests that change nothing leave their transaction to roll back.
pub(crate) async fn commit_if_in_run(
    txn: store::DatabaseTransaction,
    principal: Principal,
) -> Result<(), ServiceError> {
    if principal.run.is_some() {
        txn.commit().await.map_err(StoreError::from)?;
    }
    Ok(())
}
