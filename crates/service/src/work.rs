//! The agent's queue and its runs: what to work on next, claiming it, and how a run ends.
//!
//! A run is claimed from the queue of some heaps, holds one item for as long as it is heard from, and ends by
//! reaching the turn's goal, by asking a question, by failing, by handing the turn back, by watching a pull request
//! (the turn stays open, with a watch the loop checks), or by the person taking it back. A turn whose item is already
//! at its goal is never claimed: the claim ends it as reached. There is no lock to give up: a run nobody has heard
//! from for [`CLAIM_LAPSE`] counts as ended, failed, wherever it is read, and is written as failed the next time a
//! claim, its own `finish` or a take back touches it (architecture.md, Data model, Runs).

use std::collections::{HashMap, HashSet};

use domain::audit::AuditAction;
use domain::comment::CommentBody;
use domain::handle::Handle;
use domain::heap::{Heap, HeapPriority};
use domain::item::{Item, Kind, NewItem, Stage, StateKind};
use domain::list_view::ListView;
use domain::permission::Permission;
use domain::question::QuestionState;
use domain::run::{
    LAPSE_FAILURE, MAX_QUEUE_HEAPS, PullRequestLink, Run, RunEnd, RunError, RunForbidden, RunLink, RunReached,
    RunResult, STARTED_BOOST, default_failure, reached_check, reported_goal,
};
use domain::step::StepState;
use domain::team::Team;
use domain::turn::{TakeBackWhy, Turn, Watch};
use domain::validation::Field;
use domain::{HeapId, ItemId, RunId, Timestamp, TurnId, ValidationError};
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::access::{authorize_heap, authorize_heap_write, ensure_active, find_item_locking_in};
use crate::audit::{GoalMetHow, describe_hand_back, describe_new_turn, describe_reached, describe_watch};
use crate::items::{OpenRanking, rank_open_items};
use crate::waiting::list_gone_blockers;
use crate::{Principal, ServiceError, Services};

/// An item an agent may work on, with the heap it is in and the turn it works to.
#[derive(Debug)]
pub struct WorkItem {
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
    /// The item's turn: open, or for a run that reached its goal, the turn it ended.
    pub turn: Turn,
    /// How many runs the turn has had, ended or not.
    pub runs: u32,
    /// Whether a run has already worked on this send of the item, which gets the queue's [`STARTED_BOOST`].
    pub started: bool,
}

/// An item of the queue, with why it stands where it does.
#[derive(Debug)]
pub struct QueuedItem {
    pub work: WorkItem,
    /// The item's 1-based position in its heap's list as the person sees it by default, within its group: in
    /// progress, or open.
    pub heap_position: u32,
    /// The caller's priority for the item's heap.
    pub heap_priority: HeapPriority,
    /// Whether the turn's latest run was cut off by the agent tool's usage limit, which puts the item first.
    pub limited: bool,
}

/// The whole queue in the order a claim takes it, and how many items wait for the person.
#[derive(Debug)]
pub struct QueueView {
    pub items: Vec<QueuedItem>,
    pub waiting_count: u32,
    /// Where those items are: each heap of the queue that has any, in the queue's heap order.
    pub waiting: Vec<WaitingInHeap>,
}

/// Items waiting for the person in one heap of the queue.
#[derive(Debug)]
pub struct WaitingInHeap {
    pub heap: Heap,
    pub team: Team,
    /// At least 1.
    pub count: u32,
}

/// An open watch: the turn that has it, and the item, heap and team the turn is in.
#[derive(Debug)]
pub struct WatchView {
    pub turn: Turn,
    pub watch: Watch,
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
}

/// A run with the item it holds.
#[derive(Debug)]
pub struct RunView {
    pub run: Run,
    pub work: WorkItem,
}

/// The result of a claim: a run, or `None` when the queue was empty.
#[derive(Debug)]
pub struct ClaimView {
    pub run: Option<RunView>,
    pub waiting_count: u32,
    /// As in [`QueueView`].
    pub waiting: Vec<WaitingInHeap>,
}

/// The run a write is made in and the turn it works to.
#[derive(Debug)]
pub(crate) struct RunScope {
    pub run: Run,
    pub turn: Turn,
}

/// What orders the queue's candidates, best first: whether the turn's latest run was limited, the score, the heap's
/// index, and the position.
type QueueKey = (bool, f64, usize, u32);

/// A heap the queue is asked about, with its team.
struct QueueHeap {
    heap: Heap,
    team: Team,
    /// The caller's priority for the heap, which multiplies its items' scores in the queue.
    priority: HeapPriority,
}

/// What the queue holds right now.
struct Queue {
    items: Vec<QueuedItem>,
    /// Candidates whose item is already at the turn's goal, by heap index. They are never in `items`: a claim ends
    /// them.
    met: Vec<(usize, Item, Turn)>,
    waiting_count: u32,
    waiting: Vec<WaitingInHeap>,
}

/// `refs` as `team/heap` pairs of handles, each once, in the order given. Fails with a validation error unless there
/// are 1 to [`MAX_QUEUE_HEAPS`] well-formed entries.
fn parse_heap_refs(refs: &[String]) -> Result<Vec<(Handle, Handle)>, ValidationError> {
    let invalid = || ValidationError::FieldInvalid { field: Field::Heaps };
    if refs.is_empty() || refs.len() > MAX_QUEUE_HEAPS {
        return Err(invalid());
    }
    let mut parsed: Vec<(Handle, Handle)> = Vec::with_capacity(refs.len());
    for raw in refs {
        let (team, heap) = raw.split_once('/').ok_or_else(invalid)?;
        let pair = (
            Handle::parse(team, Field::Slug).map_err(|_| invalid())?,
            Handle::parse(heap, Field::Slug).map_err(|_| invalid())?,
        );
        if !parsed.contains(&pair) {
            parsed.push(pair);
        }
    }
    Ok(parsed)
}

impl Services {
    /// The queue over `heaps` (each `team/heap` by slug), in the order a claim takes it, and how many items wait for
    /// the person. Unpaged: the queue is at most the open items of the given heaps, which are few; past a few hundred
    /// it would need a cap.
    /// A turn whose item is already at its goal is skipped, not ended. Writes nothing: a lapsed run counts as ended,
    /// failed, but isn't written (decision: lapses are written lazily).
    ///
    /// Fails with a validation error unless `heaps` names 1 to 20 well-formed heaps, [`ServiceError::NotFound`] for a
    /// heap outside the caller's teams, [`ServiceError::HeapArchived`] for an archived one, and
    /// [`ServiceError::Forbidden`] where the caller can't edit items.
    pub async fn queue_for_agent(&self, principal: Principal, heaps: &[String]) -> Result<QueueView, ServiceError> {
        let heaps = self.resolve_queue_heaps(principal, heaps).await?;
        let now = self.now();
        let heap_ids: Vec<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let mut live = HashSet::new();
        let mut lapsed = HashSet::new();
        for run in store::runs::list_unended_runs_in_heaps(&self.db, &heap_ids, false).await? {
            if run.is_live(now) {
                live.insert(run.item_id);
            } else {
                lapsed.insert(run.turn_id);
            }
        }
        let queue = self.read_queue(&self.db, &heaps, &live, &lapsed, now, false).await?;
        Ok(QueueView {
            items: queue.items,
            waiting_count: queue.waiting_count,
            waiting: queue.waiting,
        })
    }

    /// Does what [`queue_for_agent`](Self::queue_for_agent) does under the locks of every given heap, taken in heap ID
    /// order, and starts a run on the first item. Before choosing, it writes every lapsed run on those heaps' items as
    /// failed, and ends every candidate turn whose item is already at its goal as reached, credited to the caller,
    /// so no run is spent on it (a turn sent all the way may open a next one, which is then a candidate). The run
    /// belongs to the caller. Fails as `queue_for_agent` does.
    pub async fn claim_run(&self, principal: Principal, heaps: &[String]) -> Result<ClaimView, ServiceError> {
        let mut heaps = self.resolve_queue_heaps(principal, heaps).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        // Locks first, so every read after them sees what committed while this waited, on MySQL too.
        let mut lock_order: Vec<usize> = (0..heaps.len()).collect();
        lock_order.sort_by_key(|&index| heaps[index].heap.id);
        for index in lock_order {
            let locked = store::heaps::lock_heap(&txn, heaps[index].heap.id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            ensure_active(&locked)?;
            heaps[index].heap = locked;
        }

        let now = self.now();
        let heap_ids: Vec<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let mut live = HashSet::new();
        for run in store::runs::list_unended_runs_in_heaps(&txn, &heap_ids, true).await? {
            if run.is_lapsed(now) {
                self.end_lapsed_run(&txn, &run, now).await?;
            } else {
                live.insert(run.item_id);
            }
        }
        // Every lapsed run was written above, so none is left to count.
        let mut queue = self.read_queue(&txn, &heaps, &live, &HashSet::new(), now, true).await?;
        // Each pass ends at least one turn, and a follow-on goes past the stage, so it is never met itself.
        let person = Principal { run: None, ..principal };
        while !queue.met.is_empty() {
            let mut opened = false;
            for (heap_index, item, turn) in &queue.met {
                let next = self
                    .end_met_turn(
                        &txn,
                        &heaps[*heap_index].heap,
                        person,
                        turn,
                        item,
                        item.stage,
                        GoalMetHow::AlreadyMet,
                        now,
                    )
                    .await?;
                opened |= next.is_some();
            }
            if !opened {
                break;
            }
            queue = self.read_queue(&txn, &heaps, &live, &HashSet::new(), now, true).await?;
        }
        let Some(QueuedItem { mut work, .. }) = queue.items.into_iter().next() else {
            // Lapsed runs written above stay written.
            txn.commit().await.map_err(StoreError::from)?;
            return Ok(ClaimView {
                run: None,
                waiting_count: queue.waiting_count,
                waiting: queue.waiting,
            });
        };
        let run = Run::start(
            RunId::from_uuid(self.new_id(now)),
            &work.item,
            work.turn.id,
            principal.user_id,
            now,
        );
        store::runs::insert_run(&txn, &run).await?;
        txn.commit().await.map_err(StoreError::from)?;
        work.runs += 1;
        work.started = true;

        tracing::info!(
            heap_id = %work.heap.id,
            item_id = %work.item.id,
            run_id = %run.id,
            user_id = %principal.user_id,
            "run started"
        );
        Ok(ClaimView {
            run: Some(RunView { run, work }),
            waiting_count: queue.waiting_count,
            waiting: queue.waiting,
        })
    }

    /// Says the run `run_id` is still going: it was heard from now. Fails with [`RunError::Over`] when the run has
    /// ended or lapsed, or its turn is no longer open, and with [`ServiceError::NotFound`] unless it is the caller's.
    pub async fn heartbeat(&self, principal: Principal, run_id: RunId) -> Result<(), ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (run, _) = self.lock_run(&txn, principal, run_id).await?;
        let now = self.now();
        self.ensure_live(&txn, &run, now).await?;
        store::runs::touch_run(&txn, run.id, now).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(())
    }

    /// Says the run `run_id` reached its turn's goal, or the later `stage` it reports (see [`reported_goal`]). If the
    /// item is at that goal as it is now, the run ends as reached, the turn ends as reached and the item's stage
    /// becomes that goal; for a turn sent all the way, the next turn opens in the same transaction. Otherwise it fails
    /// with [`ServiceError::NotReached`], naming the first condition missed, and the run stays live. A `stage` below
    /// the turn's goal, or any in a turn to done, fails with a validation error and writes nothing.
    ///
    /// Fails with [`RunError::Over`] when the run has ended or lapsed or its turn was taken back.
    pub async fn reached(
        &self,
        principal: Principal,
        run_id: RunId,
        result: RunResult,
        links: Vec<RunLink>,
        stage: Option<Stage>,
    ) -> Result<RunView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (run, heap) = self.lock_run(&txn, principal, run_id).await?;
        // A run that already ended as reached, as `item became` ends it, has nothing more to do.
        if run.ended.as_ref().is_some_and(|ended| ended.end == RunEnd::Reached) {
            let view = self.run_view(&txn, run).await?;
            return Ok(view);
        }
        let now = self.now();
        let turn = self.ensure_live(&txn, &run, now).await?;
        let goal = reported_goal(turn.goal, stage)?;
        let item = store::items::find_item_locking(&txn, run.item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let open_questions = store::questions::list_questions_locking(&txn, item.id)
            .await?
            .iter()
            .filter(|question| question.state() == domain::question::QuestionState::Open)
            .count();
        let live_steps = store::steps::list_steps_locking(&txn, item.id)
            .await?
            .iter()
            .filter(|step| step.state != StepState::Dropped)
            .count();
        if let Err(missed) = reached_check(&item, goal, open_questions, live_steps) {
            // The run is plainly still going, so it was heard from.
            store::runs::touch_run(&txn, run.id, now).await?;
            txn.commit().await.map_err(StoreError::from)?;
            return Err(missed.into());
        }

        // A goal that is not a stage never passes the check, so there is a stage to move to. The person may have
        // taken the item further than the goal while the agent worked, and reaching never moves it back.
        let after = goal.stage().map_or(item.stage, |goal| goal.max(item.stage));
        let mut ended_run = run.ended_as(RunEnd::Reached, now, Some(result), links, None);
        if let Some(reached) = RunReached::from_stage(after) {
            ended_run = ended_run.reaching(reached);
        }
        store::runs::end_run(&txn, &ended_run).await?;
        let ended_turn = turn.reached(principal.user_id, now);
        store::turns::end_turn(&txn, &ended_turn).await?;
        let before = item.stage;
        let item = if after == before {
            item
        } else {
            let edited = Item {
                stage: after,
                ..item.clone()
            };
            store::items::update_item(&txn, &edited, item.version, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected: item.version,
                    current: item.version.next(),
                })?
        };
        let in_run = Principal {
            run: Some(run.id),
            ..principal
        };
        let entry = self.item_audit_entry(
            &heap,
            in_run,
            AuditAction::TurnReached,
            &item,
            describe_reached(turn.goal, before, after),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        // A turn sent all the way goes on: the next one opens under the same lock, so the item is never the
        // person's in between.
        let next = turn.follow_on(TurnId::from_uuid(self.new_id(now)), &item, now);
        if let Some(next) = &next {
            store::turns::insert_turn(&txn, next).await?;
            let entry = self.item_audit_entry(
                &heap,
                in_run,
                AuditAction::TurnOpened,
                &item,
                describe_new_turn(next),
                now,
            );
            store::audit::insert_audit_entry(&txn, &entry).await?;
        }
        let view = self.run_view(&txn, ended_run).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %heap.id,
            item_id = %item.id,
            run_id = %run_id,
            goal = turn.goal.code(),
            next_goal = next.map_or("none", |next| next.goal.code()),
            user_id = %principal.user_id,
            "turn reached"
        );
        Ok(view)
    }

    /// Says the run `run_id` found nothing to do for its turn's goal, with `reason`: the run ends as handed back, with
    /// the reason as its result, and the turn ends as handed back, so the item is the person's again. Not a failure.
    /// The item's stage doesn't change and no next turn opens. Other runs of the turn that haven't ended end as taken
    /// back, and open questions asked in the turn's runs are withdrawn.
    ///
    /// Fails with [`RunError::Over`] when the run has ended or lapsed or its turn was taken back.
    pub async fn hand_back(
        &self,
        principal: Principal,
        run_id: RunId,
        reason: RunResult,
    ) -> Result<RunView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (run, heap) = self.lock_run(&txn, principal, run_id).await?;
        let now = self.now();
        let turn = self.ensure_live(&txn, &run, now).await?;

        let ended_run = run.ended_as(RunEnd::HandedBack, now, Some(reason.clone()), Vec::new(), None);
        store::runs::end_run(&txn, &ended_run).await?;
        store::turns::end_turn(&txn, &turn.handed_back(principal.user_id, now)).await?;
        let in_run = Principal {
            run: Some(run.id),
            ..principal
        };
        let entry = self.item_audit_entry_for(
            &heap,
            in_run,
            AuditAction::TurnHandedBack,
            run.item_id,
            describe_hand_back(&turn, reason.as_str()),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        self.release_turn_work(&txn, &heap, in_run, &turn, TakeBackWhy::HandedBack, now)
            .await?;
        let view = self.run_view(&txn, ended_run).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %heap.id,
            item_id = %run.item_id,
            run_id = %run_id,
            goal = turn.goal.code(),
            user_id = %principal.user_id,
            "turn handed back"
        );
        Ok(view)
    }

    /// Says the run `run_id` waits on its own pull request `url`: the run ends as watching, with the link as its one
    /// link, and the turn stays open with a watch since now, so the item leaves the queue until the watch fires. Not a
    /// failure: the turn's failed runs are set back to 0. No question or run of the turn is touched.
    ///
    /// Fails with [`RunError::Over`] when the run has ended or lapsed or its turn was taken back, and with
    /// [`RunError::WatchWhileAsking`] when the item has an open question, which makes it the person's.
    pub async fn watch(
        &self,
        principal: Principal,
        run_id: RunId,
        url: PullRequestLink,
    ) -> Result<RunView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (run, heap) = self.lock_run(&txn, principal, run_id).await?;
        let now = self.now();
        let turn = self.ensure_live(&txn, &run, now).await?;
        let asking = store::questions::list_questions_locking(&txn, run.item_id)
            .await?
            .iter()
            .any(|question| question.state() == QuestionState::Open);
        if asking {
            return Err(RunError::WatchWhileAsking.into());
        }

        let ended_run = run.ended_as(RunEnd::Watching, now, None, vec![url.to_run_link()], None);
        store::runs::end_run(&txn, &ended_run).await?;
        store::turns::set_failed_runs(&txn, turn.id, 0).await?;
        let watch = Watch { url, since: now };
        store::turns::set_turn_watch(&txn, turn.id, Some(&watch)).await?;
        let in_run = Principal {
            run: Some(run.id),
            ..principal
        };
        let entry = self.item_audit_entry_for(
            &heap,
            in_run,
            AuditAction::TurnWatching,
            run.item_id,
            describe_watch(&turn, watch.url.as_str()),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let view = self.run_view(&txn, ended_run).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %heap.id,
            item_id = %run.item_id,
            run_id = %run_id,
            turn_id = %turn.id,
            user_id = %principal.user_id,
            "turn watching a pull request"
        );
        Ok(view)
    }

    /// Every open watch on an open item of `heaps` (each `team/heap` by slug, as for the queue, with the heaps made
    /// from their items). Writes nothing. Fails as [`queue_for_agent`](Self::queue_for_agent) does.
    pub async fn list_watches(&self, principal: Principal, heaps: &[String]) -> Result<Vec<WatchView>, ServiceError> {
        let heaps = self.resolve_queue_heaps(principal, heaps).await?;
        let heap_ids: Vec<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let watching: Vec<(HeapId, Turn, Watch)> = store::turns::list_open_turns_in_heaps(&self.db, &heap_ids, false)
            .await?
            .into_iter()
            .filter_map(|(heap_id, turn)| turn.watch.clone().map(|watch| (heap_id, turn, watch)))
            .collect();
        let item_ids: Vec<ItemId> = watching.iter().map(|(_, turn, _)| turn.item_id).collect();
        let items: HashMap<ItemId, Item> = store::items::list_items_by_ids(&self.db, &item_ids)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        Ok(watching
            .into_iter()
            .filter_map(|(heap_id, turn, watch)| {
                let item = items.get(&turn.item_id).filter(|item| !item.is_closed())?;
                let queue = heaps.iter().find(|queue| queue.heap.id == heap_id)?;
                Some(WatchView {
                    watch,
                    item: item.clone(),
                    heap: queue.heap.clone(),
                    team: queue.team.clone(),
                    turn,
                })
            })
            .collect())
    }

    /// Fires the watch of the turn `turn_id`: clears it and adds a comment on the item, by the caller, saying what
    /// came, so the item is in the queue again. `what` is the lines the loop found.
    ///
    /// Fails with [`ServiceError::NotFound`] for a turn that doesn't exist or isn't in a heap the caller is in, and
    /// with [`ServiceError::WatchOver`] when the turn has ended or has no watch, writing nothing.
    pub async fn fire_watch(&self, principal: Principal, turn_id: TurnId, what: &str) -> Result<(), ServiceError> {
        let what = CommentBody::parse_as(what, Field::What)?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let turn = store::turns::find_turn(&txn, turn_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let found = store::items::find_item(&txn, turn.item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        let item = find_item_locking_in(&txn, found.id, access.heap.id, found.version).await?;
        let watch = store::turns::find_open_turn_locking(&txn, item.id)
            .await?
            .filter(|open| open.id == turn_id)
            .and_then(|open| open.watch)
            .ok_or(ServiceError::WatchOver)?;

        store::turns::set_turn_watch(&txn, turn_id, None).await?;
        let now = self.now();
        let body = CommentBody::parse(&format!(
            "Back from watching {}:\n\n{}",
            watch.url.as_str(),
            what.as_str()
        ))?;
        self.write_comment(&txn, &access.heap, principal, &item, body, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item.id,
            turn_id = %turn_id,
            user_id = %principal.user_id,
            "watch fired"
        );
        Ok(())
    }

    /// Ends the run `run_id` if it hasn't ended, and records what it cost. What a loop calls when the agent tool
    /// exits.
    ///
    /// - A run that hasn't ended ends as asked when questions were asked in it, then as limited when `limited` says
    ///   the agent tool hit the account's usage limit (the turn's failed runs are left as they are, and no failure
    ///   text is kept), otherwise as failed, with `failure` (already cut by [`cut_failure`]) or the default text. A
    ///   lapsed run ends as failed with the lapse text, whatever was sent.
    /// - A run that has ended keeps its end; only the cost is recorded.
    ///
    /// The cost, in millionths of a US dollar, is set once: a second one fails with [`RunError::CostRecorded`]. The
    /// tool's start and exit times (already checked by [`domain::run::check_tool_times`]) are each set once too, but a
    /// second one is ignored without an error. The tool and model (already cut by [`domain::run::cut_tool_text`])
    /// replace what was recorded, each only when given.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    pub async fn finish_run(
        &self,
        principal: Principal,
        run_id: RunId,
        cost_micro_usd: Option<i64>,
        tool_started_at: Option<Timestamp>,
        tool_ended_at: Option<Timestamp>,
        tool: Option<String>,
        model: Option<String>,
        failure: Option<String>,
        limited: bool,
    ) -> Result<RunView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (mut run, _) = self.lock_run(&txn, principal, run_id).await?;
        if cost_micro_usd.is_some() && run.cost_micro_usd.is_some() {
            return Err(RunError::CostRecorded.into());
        }
        let now = self.now();
        if run.ended.is_none() {
            let turn = store::turns::find_open_turn_locking(&txn, run.item_id)
                .await?
                .filter(|turn| turn.id == run.turn_id);
            run = match turn {
                // The turn ended without ending the run: not a thing that happens, but nothing is lost by closing it.
                None => run.ended_as(RunEnd::TakenBack, now, None, Vec::new(), None),
                Some(turn) if run.is_lapsed(now) => {
                    self.record_failed_run(&txn, &turn).await?;
                    run.ended_as(RunEnd::Failed, now, None, Vec::new(), Some(LAPSE_FAILURE.to_owned()))
                }
                Some(turn) if store::questions::run_asked_locking(&txn, run.id).await? => {
                    store::turns::set_failed_runs(&txn, turn.id, 0).await?;
                    run.ended_as(RunEnd::Asked, now, None, Vec::new(), None)
                }
                Some(_) if limited => run.ended_as(RunEnd::Limited, now, None, Vec::new(), None),
                Some(turn) => {
                    self.record_failed_run(&txn, &turn).await?;
                    let failure = failure.unwrap_or_else(|| default_failure(turn.goal));
                    run.ended_as(RunEnd::Failed, now, None, Vec::new(), Some(failure))
                }
            };
            store::runs::end_run(&txn, &run).await?;
        }
        if let Some(cost) = cost_micro_usd {
            store::runs::set_run_cost(&txn, run.id, cost).await?;
            run.cost_micro_usd = Some(cost);
        }
        store::runs::set_run_tool_times(&txn, run.id, tool_started_at, tool_ended_at).await?;
        run.tool_started_at = run.tool_started_at.or(tool_started_at);
        run.tool_ended_at = run.tool_ended_at.or(tool_ended_at);
        store::runs::set_run_tool_and_model(&txn, run.id, tool.as_deref(), model.as_deref()).await?;
        let view = self.run_view(&txn, run).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            run_id = %run_id,
            end = view.run.ended.as_ref().map_or("none", |ended| ended.end.code()),
            user_id = %principal.user_id,
            "run finished"
        );
        Ok(view)
    }

    /// Checks that a write made in the run `principal` carries may go on, and tells the run it was heard from. The
    /// caller holds the lock of the heap of `item_id`, the item written. Does nothing outside a run.
    ///
    /// Fails with [`ServiceError::NotFound`] for a run that doesn't exist or is another user's, with
    /// [`RunError::Over`] for one that has ended or lapsed or whose turn isn't open, and with
    /// [`RunError::WrongItem`] when `item_id` isn't the run's item.
    pub(crate) async fn check_run_write(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        item_id: ItemId,
        now: Timestamp,
    ) -> Result<Option<RunScope>, ServiceError> {
        self.check_run_item(txn, principal, item_id, None, now).await
    }

    /// [`Self::check_run_write`] for adding a step to `item`: it may also be an item this run added
    /// (`created_run_id`), or one split from the run's item by anyone, so a run can finish a split a failed run or the
    /// person started: `became` in a run needs a step on each. That is the only write a run has on them besides what it
    /// has on its own item.
    pub(crate) async fn check_run_step_add(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        item: &Item,
        now: Timestamp,
    ) -> Result<Option<RunScope>, ServiceError> {
        self.check_run_item(txn, principal, item.id, Some(item), now).await
    }

    async fn check_run_item(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        item_id: ItemId,
        made: Option<&Item>,
        now: Timestamp,
    ) -> Result<Option<RunScope>, ServiceError> {
        let Some(run_id) = principal.run else {
            return Ok(None);
        };
        let run = store::runs::find_run_locking(txn, run_id)
            .await?
            .filter(|run| run.user_id == principal.user_id)
            .ok_or(ServiceError::NotFound)?;
        let turn = self.ensure_live(txn, &run, now).await?;
        let allowed = run.item_id == item_id
            || match made {
                Some(item) => {
                    item.created_run_id == Some(run.id) || (item.made_in_split && item.origin_id == Some(run.item_id))
                }
                None => false,
            };
        if !allowed {
            let held = store::items::find_item(txn, run.item_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            return Err(RunError::WrongItem { key: held.key }.into());
        }
        store::runs::touch_run(txn, run.id, now).await?;
        Ok(Some(RunScope { run, turn }))
    }

    /// Checks that the run `principal` carries may add `fields` to the heap `target`, and tells the run it was heard
    /// from. Does nothing outside a run.
    ///
    /// Locks the heap of the run's item and `target` in heap ID order, as `claim_run` and `move_item` do (the same
    /// heap once), then reads the run under its lock (see [`Self::lock_run_and`]). The caller checks access to
    /// `target` after.
    ///
    /// Fails with [`ServiceError::NotFound`] for a run that doesn't exist or is another user's, with
    /// [`ServiceError::VersionConflict`] when the run's item moved to another heap while this waited for the lock (the
    /// run goes on: trying again works), with [`RunError::Over`] for one that has ended or lapsed or whose turn isn't
    /// open, and with
    /// [`RunForbidden::AddReminder`] or [`RunForbidden::AddNeedsContext`] for an item a run doesn't add.
    pub(crate) async fn check_run_add(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        target: HeapId,
        fields: &NewItem,
        now: Timestamp,
    ) -> Result<Option<Run>, ServiceError> {
        let Some(run_id) = principal.run else {
            return Ok(None);
        };
        let (run, _) = self.lock_run_and(txn, principal, run_id, Some(target)).await?;
        self.ensure_live(txn, &run, now).await?;
        if fields.kind == Kind::Reminder {
            return Err(RunError::Forbidden(RunForbidden::AddReminder).into());
        }
        if fields.description.is_none() {
            return Err(RunError::Forbidden(RunForbidden::AddNeedsContext).into());
        }
        store::runs::touch_run(txn, run.id, now).await?;
        Ok(Some(run))
    }

    /// Checks that the run `principal` carries may make a heap from `origin`, and tells the run it was heard from.
    /// Does nothing outside a run. A run makes a heap only from its own item, with a context (`has_context`).
    ///
    /// Fails with [`RunForbidden::Person`] without an origin, [`RunForbidden::AddNeedsContext`] without a context,
    /// [`ServiceError::NotFound`] for a run that doesn't exist or is another user's, [`RunError::WrongItem`] when the
    /// origin isn't the run's item, and [`RunError::Over`] for a run that is over.
    pub(crate) async fn check_run_make_heap(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        origin: Option<ItemId>,
        has_context: bool,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let Some(run_id) = principal.run else {
            return Ok(());
        };
        let Some(origin) = origin else {
            return Err(RunError::Forbidden(RunForbidden::Person).into());
        };
        if !has_context {
            return Err(RunError::Forbidden(RunForbidden::AddNeedsContext).into());
        }
        let (run, _) = self.lock_run_and(txn, principal, run_id, None).await?;
        self.ensure_live(txn, &run, now).await?;
        if run.item_id != origin {
            let held = store::items::find_item(txn, run.item_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            return Err(RunError::WrongItem { key: held.key }.into());
        }
        store::runs::touch_run(txn, run.id, now).await?;
        Ok(())
    }

    /// The run `run_id` as the caller's, read under the lock of its item's heap, with that heap. Fails with
    /// [`ServiceError::NotFound`] unless the caller made the run and may still edit in its heap.
    async fn lock_run(
        &self,
        txn: &store::DatabaseTransaction,
        principal: Principal,
        run_id: RunId,
    ) -> Result<(Run, Heap), ServiceError> {
        self.lock_run_and(txn, principal, run_id, None).await
    }

    /// [`Self::lock_run`], also locking the heap `target` when there is one: the two heaps lock in heap ID order (once
    /// when they are the same), so a write in a heap other than the run's item's can't deadlock with one that holds
    /// the run's heap first. Returns the heap of the run's item.
    pub(crate) async fn lock_run_and(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        run_id: RunId,
        target: Option<HeapId>,
    ) -> Result<(Run, Heap), ServiceError> {
        let found = store::runs::find_run(txn, run_id)
            .await?
            .filter(|run| run.user_id == principal.user_id)
            .ok_or(ServiceError::NotFound)?;
        let item = store::items::find_item(txn, found.item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        authorize_heap(txn, principal, item.heap_id, Permission::EditItems).await?;
        let mut ids = vec![item.heap_id];
        ids.extend(target);
        ids.sort();
        ids.dedup();
        let mut held = None;
        for heap_id in ids {
            let locked = store::heaps::lock_heap(txn, heap_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            if heap_id == item.heap_id {
                held = Some(locked);
            }
        }
        let heap = held.ok_or(ServiceError::NotFound)?;
        // The item may have moved to another heap while this waited for the lock.
        find_item_locking_in(txn, item.id, heap.id, item.version).await?;
        let run = store::runs::find_run_locking(txn, run_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        Ok((run, heap))
    }

    /// The open turn `run` belongs to, if the run is live: it hasn't ended, was heard from within the lapse, and its
    /// turn is still open. Otherwise [`RunError::Over`]. A run in an archived heap is over, as archiving took its turn
    /// back.
    async fn ensure_live(&self, txn: &impl ConnectionTrait, run: &Run, now: Timestamp) -> Result<Turn, ServiceError> {
        if !run.is_live(now) {
            return Err(RunError::Over.into());
        }
        store::turns::find_open_turn_locking(txn, run.item_id)
            .await?
            .filter(|turn| turn.id == run.turn_id)
            .ok_or_else(|| RunError::Over.into())
    }

    /// Writes the lapsed `run` as failed, and counts it against its turn.
    pub(crate) async fn end_lapsed_run(
        &self,
        txn: &impl ConnectionTrait,
        run: &Run,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let ended = run.ended_as(RunEnd::Failed, now, None, Vec::new(), Some(LAPSE_FAILURE.to_owned()));
        if !store::runs::end_run(txn, &ended).await? {
            return Ok(());
        }
        if let Some(turn) = store::turns::find_open_turn_locking(txn, run.item_id)
            .await?
            .filter(|turn| turn.id == run.turn_id)
        {
            self.record_failed_run(txn, &turn).await?;
        }
        Ok(())
    }

    /// Counts one more failed run in a row against `turn`.
    async fn record_failed_run(&self, txn: &impl ConnectionTrait, turn: &Turn) -> Result<(), ServiceError> {
        store::turns::set_failed_runs(txn, turn.id, turn.failed_runs.saturating_add(1)).await?;
        Ok(())
    }

    /// The heaps of a queue request, found for the caller in the order given, and the heaps made from their items.
    /// Checked outside any transaction: the caller locks them afterwards if it writes.
    async fn resolve_queue_heaps(&self, principal: Principal, refs: &[String]) -> Result<Vec<QueueHeap>, ServiceError> {
        let mut heaps = Vec::new();
        for (team, slug) in parse_heap_refs(refs)? {
            let found = self.resolve_heap(principal, &team, &slug).await?;
            let access = authorize_heap(&self.db, principal, found.heap_id, Permission::EditItems).await?;
            ensure_active(&access.heap)?;
            let team = store::teams::find_team(&self.db, found.team_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            heaps.push(QueueHeap {
                heap: access.heap,
                team,
                priority: HeapPriority::DEFAULT,
            });
        }
        // A heap made from an item in one of these counts as given too, and so on down: a made heap's items are the
        // work of the note it came from. Access is by team role, so no check per heap. An archived heap has no open
        // turn to give and would fail a claim, so it adds nothing.
        // A made heap is always in its note's team, so the team of a heap in the frontier is the made heap's.
        let teams: HashMap<_, _> = heaps.iter().map(|queue| (queue.team.id, queue.team.clone())).collect();
        let mut seen: HashSet<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let mut frontier: Vec<HeapId> = seen.iter().copied().collect();
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for made in store::heaps::list_heaps_made_from_items_in(&self.db, &frontier).await? {
                if made.is_archived() || !seen.insert(made.id) {
                    continue;
                }
                let team = teams.get(&made.team_id).cloned().ok_or(ServiceError::NotFound)?;
                next.push(made.id);
                heaps.push(QueueHeap {
                    heap: made,
                    team,
                    priority: HeapPriority::DEFAULT,
                });
            }
            frontier = next;
        }
        let heap_ids: Vec<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let priorities = store::heap_priorities::list_heap_priorities(&self.db, principal.user_id, &heap_ids).await?;
        for queue in &mut heaps {
            if let Some(priority) = priorities.get(&queue.heap.id) {
                queue.priority = *priority;
            }
        }
        // Ties between heaps go by name, then ID, as Waiting for me orders them.
        heaps.sort_by(|a, b| (a.heap.name.as_str(), a.heap.id).cmp(&(b.heap.name.as_str(), b.heap.id)));
        Ok(heaps)
    }

    /// What the queue holds in `heaps`, which are sorted as ties between heaps are broken. `live` holds the items that
    /// have a live run, and `lapsed` the turns that have a lapsed run not written yet, which counts as failed
    /// ([`Turn::counting_lapse`]). Ranks each heap that has something in the queue once. The candidates come best
    /// first: those whose turn's latest run ended as limited (the agent tool's usage limit), so the work a limit cut
    /// off is the one claimed once it resets; then the highest score
    /// times the caller's priority for its heap, times [`STARTED_BOOST`] when a run has already worked on its send
    /// (its turn, or an earlier stage of the same all-the-way send or the note it was split from), then by heap, then
    /// by position.
    async fn read_queue(
        &self,
        db: &impl ConnectionTrait,
        heaps: &[QueueHeap],
        live: &HashSet<ItemId>,
        lapsed: &HashSet<TurnId>,
        now: Timestamp,
        lock: bool,
    ) -> Result<Queue, ServiceError> {
        let heap_ids: Vec<HeapId> = heaps.iter().map(|queue| queue.heap.id).collect();
        let turns: Vec<(HeapId, Turn)> = store::turns::list_open_turns_in_heaps(db, &heap_ids, lock)
            .await?
            .into_iter()
            .map(|(heap_id, turn)| (heap_id, turn.counting_lapse(lapsed.contains(&turn.id))))
            .collect();
        let questions = store::questions::list_open_questions_on_open_items(db, &heap_ids).await?;
        let with_question: HashSet<ItemId> = questions.iter().map(|(_, question)| question.item_id).collect();
        let gone = list_gone_blockers(db, &heap_ids).await?;
        let waiting_heap: HashMap<ItemId, HeapId> = questions
            .iter()
            .map(|(heap_id, question)| (question.item_id, *heap_id))
            .chain(
                turns
                    .iter()
                    .filter(|(_, turn)| turn.is_stuck())
                    .map(|(heap_id, turn)| (turn.item_id, *heap_id)),
            )
            .chain(gone.iter().map(|(item, ..)| (item.id, item.heap_id)))
            .collect();
        let waiting_count = u32::try_from(waiting_heap.len()).unwrap_or(u32::MAX);
        let waiting = heaps
            .iter()
            .filter_map(|queue| {
                let count = waiting_heap.values().filter(|id| **id == queue.heap.id).count();
                (count > 0).then(|| WaitingInHeap {
                    heap: queue.heap.clone(),
                    team: queue.team.clone(),
                    count: u32::try_from(count).unwrap_or(u32::MAX),
                })
            })
            .collect();

        // Items an agent could take: an open turn that isn't stuck or watching, no open question and no live run.
        let mut by_heap: HashMap<HeapId, HashMap<ItemId, Turn>> = HashMap::new();
        for (heap_id, turn) in turns {
            if !turn.is_stuck()
                && turn.watch.is_none()
                && !with_question.contains(&turn.item_id)
                && !live.contains(&turn.item_id)
            {
                by_heap.entry(heap_id).or_default().insert(turn.item_id, turn);
            }
        }

        // The sends that have had a run on any turn: a candidate in one is started work, and gets the boost.
        let candidate_sends: Vec<TurnId> = by_heap
            .values()
            .flat_map(|turns| turns.values().map(|turn| turn.send_id))
            .collect();
        let started_sends = store::runs::sends_with_runs(db, &candidate_sends).await?;

        // The turns whose latest run hit the agent tool's usage limit come before every other candidate.
        let candidate_turns: Vec<TurnId> = by_heap
            .values()
            .flat_map(|turns| turns.values().map(|turn| turn.id))
            .collect();
        let limited_turns = store::runs::turns_last_limited(db, &candidate_turns).await?;

        // Each candidate with its sort key. Its runs are counted below, in one query for all of them.
        let mut met = Vec::new();
        let mut candidates: Vec<(QueueKey, QueuedItem)> = Vec::new();
        let list = ListView::default();
        for (heap_index, queue) in heaps.iter().enumerate() {
            let Some(in_heap) = by_heap.remove(&queue.heap.id) else {
                continue;
            };
            let priority = queue.priority;
            let OpenRanking { items, ranking, .. } = rank_open_items(db, &queue.heap, now, now).await?;
            let by_id: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
            // The position the person sees is within the item's group of the heap's default list (in progress
            // pinned, open, blocked hidden): how many items of each group came so far.
            let mut listed = [0_u32; 3];
            for ranked in &ranking {
                let Some(item) = by_id.get(&ranked.item_id) else {
                    continue;
                };
                let list_position = list.group_of(item.state.kind()).map(|group| {
                    let count = &mut listed[group as usize];
                    *count += 1;
                    *count
                });
                let Some(turn) = in_heap.get(&ranked.item_id) else {
                    continue;
                };
                if item.state.kind() == StateKind::Blocked {
                    continue;
                }
                if turn.goal_met_by(item.stage) {
                    met.push((heap_index, (*item).clone(), turn.clone()));
                    continue;
                }
                let started = started_sends.contains(&turn.send_id);
                let boost = if started { STARTED_BOOST } else { 1.0 };
                let limited = limited_turns.contains(&turn.id);
                let key = (
                    limited,
                    ranked.score * priority.as_factor() * boost,
                    heap_index,
                    ranked.position,
                );
                candidates.push((
                    key,
                    QueuedItem {
                        // An item that isn't blocked is always in the default list.
                        heap_position: list_position.unwrap_or(ranked.position),
                        heap_priority: priority,
                        limited,
                        work: WorkItem {
                            item: (*item).clone(),
                            heap: queue.heap.clone(),
                            team: queue.team.clone(),
                            turn: turn.clone(),
                            runs: 0,
                            started,
                        },
                    },
                ));
            }
        }

        candidates.sort_by(|(a, _), (b, _)| {
            a.0.cmp(&b.0)
                .reverse()
                .then_with(|| a.1.total_cmp(&b.1).reverse())
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.3.cmp(&b.3))
        });
        let turn_ids: Vec<TurnId> = candidates.iter().map(|(_, queued)| queued.work.turn.id).collect();
        let run_counts = store::runs::count_runs_for_turns(db, &turn_ids).await?;
        let items = candidates
            .into_iter()
            .map(|(_, mut queued)| {
                queued.work.runs = run_counts.get(&queued.work.turn.id).copied().unwrap_or(0);
                queued
            })
            .collect();
        Ok(Queue {
            items,
            met,
            waiting_count,
            waiting,
        })
    }

    /// `run` with the item it holds, its heap, team and turn.
    async fn run_view(&self, db: &impl ConnectionTrait, run: Run) -> Result<RunView, ServiceError> {
        let item = store::items::find_item(db, run.item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let heap = store::heaps::find_heap(db, item.heap_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let team = store::teams::find_team(db, heap.team_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let turn = store::turns::find_turn(db, run.turn_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let runs = store::runs::count_runs_for_turns(db, &[turn.id])
            .await?
            .get(&turn.id)
            .copied()
            .unwrap_or(0);
        Ok(RunView {
            run,
            work: WorkItem {
                item,
                heap,
                team,
                turn,
                runs,
                // The run viewed is on this turn, so its send has had one.
                started: true,
            },
        })
    }
}

/// Whether `item` was split from `run`'s item by a run of the same turn: `run` itself, or one before it that failed
/// or lapsed. A run may chain blocks on such items (Blocks §1).
pub(crate) async fn split_in_turn(db: &impl ConnectionTrait, item: &Item, run: &Run) -> Result<bool, ServiceError> {
    if !item.made_in_split || item.origin_id != Some(run.item_id) {
        return Ok(false);
    }
    Ok(match item.created_run_id {
        Some(maker) if maker == run.id => true,
        Some(maker) => store::runs::find_run(db, maker)
            .await?
            .is_some_and(|maker| maker.turn_id == run.turn_id),
        None => false,
    })
}
