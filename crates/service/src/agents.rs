//! The Agents page: what agents finished that waits for the caller (Your decision), and the history of the runs that
//! ended as reached. Both are built from runs and turns, not the audit log.

use std::collections::{BTreeSet, HashMap};

use domain::heap::{Heap, HeapPriority};
use domain::item::{Item, Kind, StateKind};
use domain::run::{Run, RunReached};
use domain::team::Team;
use domain::turn::{Goal, Turn};
use domain::{Duration, HeapId, ItemId, Timestamp, TurnId};

pub use store::runs::ReachedCursor;

use crate::items::{OpenRanking, rank_open_items};
use crate::{Principal, ServiceError, Services};

/// Most rows [`Services::list_decisions`] returns. As for Waiting for me, one response holds them all, and this
/// bounds a pathological one. The count still says how many there are.
pub const MAX_DECISION_ROWS: usize = 500;

/// How many runs a page of the history holds.
pub const RESULTS_PAGE_SIZE: u64 = 50;

/// How far back the history goes.
pub const RESULTS_DAYS: i64 = 30;

/// What waits for the caller's decision.
#[derive(Debug)]
pub struct DecisionsView {
    /// How many rows wait, whether or not they all fit in `rows`.
    pub count: u32,
    /// The first [`MAX_DECISION_ROWS`] of them, most important first.
    pub rows: Vec<DecisionRow>,
}

/// An item an agent brought to a stable state, which now waits for the person.
#[derive(Debug)]
pub struct DecisionRow {
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
    /// When the turn that reached it ended.
    pub arrived_at: Timestamp,
    /// What a note that became other items became, in words: "made 3 tasks". `None` on any other row.
    pub became_label: Option<String>,
}

/// One run that ended as reached, with the item it was on.
#[derive(Debug)]
pub struct ResultRow {
    pub run: Run,
    /// The goal of the run's turn.
    pub goal: Goal,
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
}

/// One page of the history.
#[derive(Debug)]
pub struct ResultsPage {
    /// Newest first.
    pub rows: Vec<ResultRow>,
    /// Where the next page starts, or `None` on the last page.
    pub next: Option<ReachedCursor>,
}

/// A decision, before it is shown: where it ranks and the item it is on.
struct Candidate {
    heap_index: usize,
    score: f64,
    /// The caller's priority for the heap the score is in: the row sorts by the score times this.
    priority: HeapPriority,
    position: u32,
    item: Item,
    arrived_at: Timestamp,
    became_label: Option<String>,
}

impl Candidate {
    fn sort_key(&self) -> f64 {
        self.score * self.priority.as_factor()
    }
}

impl Services {
    /// The items an agent brought to a stable state that still wait for the caller, in the active heaps of their
    /// teams: the latest turn of the item ended as reached by a run that ended as reached, the item is not closed or
    /// in progress, and it is still at the stage the run reached. A note that became items or heaps is one row, until
    /// one of what it made is started, closed or sent on. Ordered as Waiting for me orders its rows: the item's
    /// score in its own heap times the caller's priority for that heap, highest first, then heap, then position. A
    /// became row takes the best of its made items that are ranked, or score 0.
    ///
    /// The queries are over sets of heaps, turns and items, and only heaps with a row are ranked. Any member may
    /// ask; nobody sees a heap outside their teams.
    pub async fn list_decisions(&self, principal: Principal) -> Result<DecisionsView, ServiceError> {
        self.list_decisions_capped(principal, MAX_DECISION_ROWS).await
    }

    /// [`list_decisions`](Self::list_decisions) with at most `max_rows` rows in place of [`MAX_DECISION_ROWS`], so
    /// the cap can be tested without a row for each of its places.
    pub async fn list_decisions_capped(
        &self,
        principal: Principal,
        max_rows: usize,
    ) -> Result<DecisionsView, ServiceError> {
        let empty = || DecisionsView {
            count: 0,
            rows: Vec::new(),
        };
        let viewable = self.list_viewable_heaps(principal).await?;
        let heap_ids: Vec<HeapId> = viewable.iter().map(|viewable| viewable.heap.id).collect();
        let turns = store::turns::list_latest_reached_turns_in_heaps(&self.db, &heap_ids).await?;
        if turns.is_empty() {
            return Ok(empty());
        }
        let turn_ids: Vec<TurnId> = turns.iter().map(|turn| turn.id).collect();
        // A turn reached by one run: the person's own reach, or a claim that found the item there, has none.
        let reached_by: HashMap<TurnId, Run> = store::runs::list_reached_runs_for_turns(&self.db, &turn_ids)
            .await?
            .into_iter()
            .map(|run| (run.turn_id, run))
            .collect();
        let item_ids: Vec<ItemId> = turns.iter().map(|turn| turn.item_id).collect();
        let items: HashMap<ItemId, Item> = store::items::list_items_by_ids(&self.db, &item_ids)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();

        let mut plain: Vec<(Item, Timestamp)> = Vec::new();
        let mut notes: Vec<(Item, Timestamp, Turn)> = Vec::new();
        for turn in turns {
            let (Some(run), Some(item)) = (reached_by.get(&turn.id), items.get(&turn.item_id)) else {
                continue;
            };
            let Some(arrived_at) = turn.ended.map(|ended| ended.at) else {
                continue;
            };
            let reached = run.ended.as_ref().and_then(|ended| ended.reached);
            match item.state.kind() {
                StateKind::Became => {
                    if reached == Some(RunReached::Became) {
                        notes.push((item.clone(), arrived_at, turn));
                    }
                }
                StateKind::Open | StateKind::Blocked => {
                    let stage = match reached {
                        Some(reached) => reached.stage(),
                        None => turn.goal.stage(),
                    };
                    if stage == Some(item.stage) {
                        plain.push((item.clone(), arrived_at));
                    }
                }
                StateKind::InProgress | StateKind::Done | StateKind::WontDo => {}
            }
        }
        let became = self.became_rows(notes).await?;
        if plain.is_empty() && became.is_empty() {
            return Ok(empty());
        }

        let priorities = store::heap_priorities::list_heap_priorities(&self.db, principal.user_id, &heap_ids).await?;
        let priority_of = |heap_id: HeapId| priorities.get(&heap_id).copied().unwrap_or(HeapPriority::DEFAULT);
        let as_of = self.now();
        let heap_has_row: BTreeSet<HeapId> = plain
            .iter()
            .map(|(item, _)| item.heap_id)
            .chain(became.iter().map(|row| row.note.heap_id))
            .collect();
        let mut ranks: HashMap<ItemId, (f64, u32)> = HashMap::new();
        for heap in viewable
            .iter()
            .map(|viewable| &viewable.heap)
            .filter(|heap| heap_has_row.contains(&heap.id))
        {
            let OpenRanking { ranking, .. } = rank_open_items(&self.db, heap, as_of, as_of).await?;
            ranks.extend(
                ranking
                    .iter()
                    .map(|ranked| (ranked.item_id, (ranked.score, ranked.position))),
            );
        }
        let heap_index = |heap_id: HeapId| viewable.iter().position(|viewable| viewable.heap.id == heap_id);

        let mut candidates = Vec::new();
        for (item, arrived_at) in plain {
            let (Some(index), Some(&(score, position))) = (heap_index(item.heap_id), ranks.get(&item.id)) else {
                continue;
            };
            candidates.push(Candidate {
                heap_index: index,
                score,
                priority: priority_of(item.heap_id),
                position,
                item,
                arrived_at,
                became_label: None,
            });
        }
        for row in became {
            let Some(index) = heap_index(row.note.heap_id) else {
                continue;
            };
            // The row stands for the note; it ranks where the best of what was made from it ranks.
            let best = row
                .made
                .iter()
                .filter_map(|made| {
                    let &(score, position) = ranks.get(&made.id)?;
                    Some((score, position, priority_of(made.heap_id)))
                })
                .max_by(|a, b| (a.0 * a.2.as_factor()).total_cmp(&(b.0 * b.2.as_factor())));
            let (score, position, priority) = best.unwrap_or((0.0, 0, priority_of(row.note.heap_id)));
            candidates.push(Candidate {
                heap_index: index,
                score,
                priority,
                position,
                item: row.note,
                arrived_at: row.arrived_at,
                became_label: Some(row.label),
            });
        }
        candidates.sort_by(|a, b| {
            b.sort_key()
                .total_cmp(&a.sort_key())
                .then_with(|| a.heap_index.cmp(&b.heap_index))
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.item.created_at.cmp(&b.item.created_at))
                .then_with(|| a.item.id.cmp(&b.item.id))
        });
        let count = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        candidates.truncate(max_rows);

        let rows = candidates
            .into_iter()
            .map(|candidate| {
                let viewable = &viewable[candidate.heap_index];
                DecisionRow {
                    heap: viewable.heap.clone(),
                    team: viewable.team.clone(),
                    arrived_at: candidate.arrived_at,
                    became_label: candidate.became_label,
                    item: candidate.item,
                }
            })
            .collect();
        tracing::debug!(user_id = %principal.user_id, count, "listed decisions");
        Ok(DecisionsView { count, rows })
    }

    /// The notes of `notes` (each closed as became by a run, with the turn that ended) that became items or heaps
    /// and still wait: nothing made from them is in progress or closed, or has a turn opened since the note's turn
    /// ended.
    async fn became_rows(&self, notes: Vec<(Item, Timestamp, Turn)>) -> Result<Vec<BecameRow>, ServiceError> {
        if notes.is_empty() {
            return Ok(Vec::new());
        }
        let note_ids: Vec<ItemId> = notes.iter().map(|(note, ..)| note.id).collect();
        let made_items = store::items::list_items_made_from(&self.db, &note_ids, false).await?;
        let made_heaps = store::heaps::list_heaps_made_from(&self.db, &note_ids).await?;
        let made_ids: Vec<ItemId> = made_items.iter().map(|item| item.id).collect();
        let latest_turns = store::turns::list_latest_turns(&self.db, &made_ids).await?;
        let mut rows = Vec::new();
        for (note, arrived_at, _) in notes {
            let made: Vec<Item> = made_items
                .iter()
                .filter(|item| item.origin_id == Some(note.id))
                .cloned()
                .collect();
            let heaps: Vec<&Heap> = made_heaps
                .iter()
                .filter(|heap| heap.origin_id == Some(note.id))
                .collect();
            // A note that became a step made neither.
            if made.is_empty() && heaps.is_empty() {
                continue;
            }
            let sent_on = made.iter().any(|item| {
                item.state.kind() == StateKind::InProgress
                    || item.is_closed()
                    || latest_turns
                        .get(&item.id)
                        .is_some_and(|turn| turn.opened_at > arrived_at)
            });
            if sent_on {
                continue;
            }
            let label = became_label(&made, &heaps);
            rows.push(BecameRow {
                note,
                arrived_at,
                made,
                label,
            });
        }
        Ok(rows)
    }

    /// The runs that ended as reached in the last [`RESULTS_DAYS`] days, on the items of the caller's active heaps,
    /// newest first, [`RESULTS_PAGE_SIZE`] a page, starting after `after`. Any member may ask; nobody sees a heap
    /// outside their teams.
    pub async fn list_results(
        &self,
        principal: Principal,
        after: Option<ReachedCursor>,
    ) -> Result<ResultsPage, ServiceError> {
        self.list_results_paged(principal, after, RESULTS_PAGE_SIZE).await
    }

    /// [`list_results`](Self::list_results) with `page_size` runs a page in place of [`RESULTS_PAGE_SIZE`], so paging
    /// can be tested without a page of runs.
    pub async fn list_results_paged(
        &self,
        principal: Principal,
        after: Option<ReachedCursor>,
        page_size: u64,
    ) -> Result<ResultsPage, ServiceError> {
        let viewable = self.list_viewable_heaps(principal).await?;
        let heap_ids: Vec<HeapId> = viewable.iter().map(|viewable| viewable.heap.id).collect();
        let since = self
            .now()
            .checked_sub(Duration::from_days(RESULTS_DAYS))
            .ok_or(domain::ValidationError::TimestampOutOfRange)?;
        // One extra run tells whether another page follows.
        let mut found =
            store::runs::list_reached_runs_in_heaps(&self.db, &heap_ids, since, after, page_size + 1).await?;
        let has_more = found.len() as u64 > page_size;
        found.truncate(usize::try_from(page_size).unwrap_or(usize::MAX));
        let next = has_more
            .then(|| {
                found.last().and_then(|(run, _)| {
                    run.ended.as_ref().map(|ended| ReachedCursor {
                        ended_at: ended.at,
                        id: run.id,
                    })
                })
            })
            .flatten();
        let item_ids: Vec<ItemId> = found.iter().map(|(run, _)| run.item_id).collect();
        let items: HashMap<ItemId, Item> = store::items::list_items_by_ids(&self.db, &item_ids)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();
        let rows = found
            .into_iter()
            .filter_map(|(run, goal)| {
                let item = items.get(&run.item_id)?.clone();
                let viewable = viewable.iter().find(|viewable| viewable.heap.id == item.heap_id)?;
                Some(ResultRow {
                    run,
                    goal,
                    item,
                    heap: viewable.heap.clone(),
                    team: viewable.team.clone(),
                })
            })
            .collect();
        Ok(ResultsPage { rows, next })
    }
}

/// A note that became items or heaps and still waits, before it is ranked.
struct BecameRow {
    note: Item,
    arrived_at: Timestamp,
    made: Vec<Item>,
    label: String,
}

/// What a note became, in words: the heaps made from it and the items made in its own heap or elsewhere.
fn became_label(made: &[Item], made_heaps: &[&Heap]) -> String {
    let own: Vec<&Item> = made
        .iter()
        .filter(|item| made_heaps.iter().all(|heap| heap.id != item.heap_id))
        .collect();
    let mut parts: Vec<String> = made_heaps
        .iter()
        .map(|heap| format!("the heap \u{201c}{}\u{201d}", heap.name.as_str()))
        .collect();
    if !own.is_empty() {
        let noun = if own.iter().all(|item| item.kind == Kind::Task) {
            "task"
        } else {
            "item"
        };
        parts.push(match own.len() {
            1 => format!("a {noun}"),
            count => format!("{count} {noun}s"),
        });
    }
    format!("made {}", parts.join(" and "))
}
