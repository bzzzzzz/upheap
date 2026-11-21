//! One ranked list across all heaps: the items of every active heap the caller may view, merged into the groups of
//! one list view. The mechanism is in architecture.md, Ranking, Across heaps.

use std::collections::HashSet;

use domain::heap::{Heap, HeapPriority};
use domain::item::StateKind;
use domain::list_view::ListView;
use domain::team::Team;
use domain::{HeapId, ParameterId, TeamId, Timestamp};

use crate::items::{RankedEntry, RankedList, ensure_tags_in_heap};
use crate::{Principal, ServiceError, Services};

/// Most rows [`Services::list_ranked_across_heaps`] returns, in all groups together. A person has tens to a few
/// hundred open items across their heaps, so one response holds them all and the list needs no paging; this bounds a
/// pathological one. The count still says how many there are. When the size no longer holds, the list gains a cursor
/// over the merged order.
pub const MAX_ACROSS_HEAPS_ROWS: usize = 500;

/// A row of the merged list: an item as its own heap's list shows it, with the heap and its place in the merge.
#[derive(Debug)]
pub struct AcrossHeapsEntry {
    /// 1-based position within its group across all heaps. `entry.position` stays the position within its heap.
    pub position: u32,
    pub entry: RankedEntry,
    pub heap: Heap,
    pub team: Team,
    /// The caller's priority for the item's heap.
    pub priority: HeapPriority,
}

/// The ranked items of all the caller's heaps, grouped as one [`ListView`] groups them.
#[derive(Debug)]
pub struct AcrossHeapsList {
    /// The time every heap was ranked at. Clients send it back to keep the order stable.
    pub as_of: Timestamp,
    /// How many rows the view lists in all groups, whether or not they all fit under the cap.
    pub count: u32,
    /// Items in pinned states, shown above the rest.
    pub pinned: Vec<AcrossHeapsEntry>,
    /// Items in shown states, including blocked items mixed in.
    pub ranked: Vec<AcrossHeapsEntry>,
    /// Shown blocked items, when they're grouped.
    pub blocked: Vec<AcrossHeapsEntry>,
    /// Items the filters keep but whose state is hidden, summed over heaps.
    pub hidden_count: u32,
    /// Items in progress, summed over the heaps listed, whatever the view and its filters leave out.
    pub in_progress_count: u32,
}

/// An entry on its way to the merged list: the place of its heap in the store's heap order, and the heap.
struct Candidate {
    heap_index: usize,
    priority: HeapPriority,
    entry: RankedEntry,
}

impl Candidate {
    /// What the merge orders by: the item's score in its own heap times the caller's priority for that heap.
    fn sort_key(&self) -> f64 {
        self.entry.score * self.priority.as_factor()
    }
}

impl Services {
    /// The non-closed items of the active heaps of the caller's teams where their role may view the heap, ranked at
    /// one time and shown as `view` says. Each heap is ranked and grouped as its own list does
    /// ([`list_ranked`](Self::list_ranked)); then each group is merged across heaps, ordered by the item's score in
    /// its own heap times the caller's priority for that heap, highest first, then by heap (by name, then ID), then by the
    /// item's position in its heap's group. So every heap's own relative order is kept. At most
    /// [`MAX_ACROSS_HEAPS_ROWS`] rows are returned, taken pinned first, then ranked, then blocked.
    ///
    /// With `team`, only that team's heaps are ranked and merged, and the cap applies to them alone. The caller must
    /// be in the team: resolve it with [`resolve_team`](Self::resolve_team) first.
    ///
    /// `requested_as_of` is honoured as [`list_ranked`](Self::list_ranked) honours it. `view` must sort by priority:
    /// the list across heaps has no heap to resolve a sort by parameter against. It filters by tags and options
    /// only with a `team`, which the caller passes; they must be the team's tags and the options of its parameters
    /// ([`ServiceError::TagNotInHeap`], [`ServiceError::OptionNotInHeap`]). Options of archived team parameters
    /// don't filter. A heap that doesn't use every other parameter the options name is left out unranked, as none of
    /// its items can hold a value for it. Filtering picks rows after each heap is ranked, so the merged order is the
    /// unfiltered one with the other rows taken out. Ranking costs one pass over a heap's non-closed items, so heaps
    /// with none are skipped. Read-only; any member may ask, and nobody sees a heap outside their teams.
    pub async fn list_ranked_across_heaps(
        &self,
        principal: Principal,
        requested_as_of: Option<Timestamp>,
        view: &ListView,
        team: Option<TeamId>,
    ) -> Result<AcrossHeapsList, ServiceError> {
        self.list_ranked_across_heaps_capped(principal, requested_as_of, view, team, MAX_ACROSS_HEAPS_ROWS)
            .await
    }

    /// [`list_ranked_across_heaps`](Self::list_ranked_across_heaps) with at most `max_rows` rows in place of
    /// [`MAX_ACROSS_HEAPS_ROWS`], so the cap can be tested without a row for each of its rows.
    pub async fn list_ranked_across_heaps_capped(
        &self,
        principal: Principal,
        requested_as_of: Option<Timestamp>,
        view: &ListView,
        team: Option<TeamId>,
        max_rows: usize,
    ) -> Result<AcrossHeapsList, ServiceError> {
        let now = self.now();
        let as_of = self.effective_as_of(requested_as_of, now);
        let (view, using) = self.team_filter(team, view).await?;
        let view = &view;
        let mut viewable = self.list_viewable_heaps(principal).await?;
        if let Some(team) = team {
            viewable.retain(|viewable| viewable.team.id == team);
        }
        let heap_ids: Vec<_> = viewable.iter().map(|viewable| viewable.heap.id).collect();
        let with_items = store::items::count_items_by_heap(&self.db, &heap_ids, &StateKind::OPEN, now).await?;

        let priorities = store::heap_priorities::list_heap_priorities(&self.db, principal.user_id, &heap_ids).await?;
        let (mut pinned, mut ranked, mut blocked) = (Vec::new(), Vec::new(), Vec::new());
        let (mut hidden_count, mut in_progress_count) = (0_u32, 0_u32);
        for (heap_index, viewable) in viewable.iter().enumerate() {
            if !with_items.contains_key(&viewable.heap.id)
                || using.as_ref().is_some_and(|using| !using.contains(&viewable.heap.id))
            {
                continue;
            }
            let RankedList {
                pinned: heap_pinned,
                ranked: heap_ranked,
                blocked: heap_blocked,
                hidden_count: heap_hidden,
                in_progress_count: heap_in_progress,
                ..
            } = self
                .rank_heap_list(viewable.heap.clone(), viewable.role, as_of, now, view)
                .await?;
            let priority = priorities
                .get(&viewable.heap.id)
                .copied()
                .unwrap_or(HeapPriority::DEFAULT);
            hidden_count = hidden_count.saturating_add(heap_hidden);
            in_progress_count = in_progress_count.saturating_add(heap_in_progress);
            for (group, entries) in [
                (&mut pinned, heap_pinned),
                (&mut ranked, heap_ranked),
                (&mut blocked, heap_blocked),
            ] {
                group.extend(entries.into_iter().map(|entry| Candidate {
                    heap_index,
                    priority,
                    entry,
                }));
            }
        }

        let count = [&pinned, &ranked, &blocked]
            .iter()
            .map(|group| group.len())
            .sum::<usize>();
        let mut room = max_rows;
        let mut merge = |mut group: Vec<Candidate>| {
            group.sort_by(|a, b| {
                b.sort_key()
                    .total_cmp(&a.sort_key())
                    .then_with(|| a.heap_index.cmp(&b.heap_index))
                    .then_with(|| a.entry.position.cmp(&b.entry.position))
            });
            group.truncate(room);
            room -= group.len();
            group
                .into_iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let owner = &viewable[candidate.heap_index];
                    AcrossHeapsEntry {
                        position: u32::try_from(index + 1).unwrap_or(u32::MAX),
                        entry: candidate.entry,
                        heap: owner.heap.clone(),
                        team: owner.team.clone(),
                        priority: candidate.priority,
                    }
                })
                .collect::<Vec<_>>()
        };
        let (pinned, ranked, blocked) = (merge(pinned), merge(ranked), merge(blocked));

        tracing::debug!(
            user_id = %principal.user_id,
            heaps = viewable.len(),
            count,
            as_of = as_of.as_millis(),
            "listed ranked items across heaps"
        );
        Ok(AcrossHeapsList {
            as_of,
            count: u32::try_from(count).unwrap_or(u32::MAX),
            pinned,
            ranked,
            blocked,
            hidden_count,
            in_progress_count,
        })
    }
}

impl Services {
    /// `view` checked against `team`'s tags and parameters, and trimmed for ranking: the options of archived team
    /// parameters, which don't filter, are dropped. With options of non-archived parameters, also the heaps of the
    /// team that use every one of those parameters now; the others can't hold a value for one, so they have no rows.
    /// Reads nothing for a view without tags and options, or without a team.
    async fn team_filter(
        &self,
        team: Option<TeamId>,
        view: &ListView,
    ) -> Result<(ListView, Option<HashSet<HeapId>>), ServiceError> {
        let Some(team) = team else {
            return Ok((view.clone(), None));
        };
        if !view.filter.tags.is_empty() {
            let team_tags = store::tags::list_team_tags(&self.db, team).await?;
            ensure_tags_in_heap(&team_tags, &view.filter.tags)?;
        }
        if view.filter.options.is_empty() {
            return Ok((view.clone(), None));
        }
        let parameters = store::parameters::list_team_parameters(&self.db, team).await?;
        let mut named: Vec<ParameterId> = Vec::new();
        let mut trimmed = view.clone();
        trimmed.filter.options.clear();
        for &option_id in &view.filter.options {
            let parameter = parameters
                .iter()
                .find(|parameter| parameter.find_option(option_id).is_some())
                .ok_or(ServiceError::OptionNotInHeap { option_id })?;
            if !parameter.is_archived() {
                trimmed.filter.options.insert(option_id);
                if !named.contains(&parameter.id) {
                    named.push(parameter.id);
                }
            }
        }
        if named.is_empty() {
            return Ok((trimmed, None));
        }
        let uses = store::parameters::list_heap_uses(&self.db, &named).await?;
        let using = store::heaps::list_heaps_for_teams(&self.db, &[team])
            .await?
            .into_iter()
            .map(|heap| heap.id)
            .filter(|heap_id| {
                named.iter().all(|parameter_id| {
                    uses.iter().any(|heap_use| {
                        heap_use.heap_id == *heap_id
                            && heap_use.parameter_id == *parameter_id
                            && heap_use.stopped_at.is_none()
                    })
                })
            })
            .collect();
        Ok((trimmed, Some(using)))
    }
}
