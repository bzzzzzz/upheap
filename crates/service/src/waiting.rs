//! Waiting for me: every open question on an open item, and every item whose agent turn is stuck, across the caller's
//! heaps, most important first.

use std::collections::{BTreeSet, HashMap, HashSet};

use domain::heap::Heap;
use domain::item::{Item, Title};
use domain::permission::{ItemAction, Permission, allowed_item_actions, allowed_question_actions};
use domain::step::StepNumber;
use domain::team::{Role, Team};
use domain::turn::{Goal, Turn, with_agent};
use domain::user::User;
use domain::{HeapId, ItemId, TurnId, UserId};

use crate::items::{OpenRanking, QuestionView, TurnView, rank_open_items, turn_views};
use crate::{Principal, ServiceError, Services};

/// Most rows [`Services::list_waiting`] returns. A person waits on tens of questions, not thousands, so one
/// response holds them all, and this bounds a pathological one. The count still says how many there are.
pub const MAX_WAITING_ROWS: usize = 500;

/// What waits for the caller.
#[derive(Debug)]
pub struct WaitingView {
    /// How many rows wait (questions and stuck turns), whether or not they all fit in `rows`.
    pub count: u32,
    /// The first [`MAX_WAITING_ROWS`] of them, most important first.
    pub rows: Vec<WaitingRow>,
}

/// One thing that waits for the person, with the item and heap it is on.
#[derive(Debug)]
pub struct WaitingRow {
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
    /// The title of the step the question names, when it names one that exists. `None` on a stuck row.
    pub step_title: Option<Title>,
    pub subject: WaitingSubject,
    /// The item's open turn, if an agent was sent it and it hasn't been taken back. The item waits for the person,
    /// so the agent doesn't have it ([`WaitingRow::with_agent`]).
    pub turn: Option<TurnView>,
    /// Whether an agent has the item: never, as a row's item waits for the person.
    pub with_agent: bool,
}

/// What a row is about.
#[derive(Debug)]
pub enum WaitingSubject {
    /// An open question or request.
    Question(Box<QuestionView>),
    /// A turn whose runs keep failing.
    Stuck(StuckView),
}

/// A stuck turn, as a row shows it.
#[derive(Debug)]
pub struct StuckView {
    /// How many runs in a row failed.
    pub failed_runs: u32,
    /// Why the latest failed run failed.
    pub reason: Option<String>,
    pub goal: Goal,
    /// What the caller may do about it: retry and take back, when they may edit items.
    pub allowed_actions: Vec<ItemAction>,
}

/// An active heap the caller may view, with its team and the caller's role in it.
pub(crate) struct ViewableHeap {
    pub heap: Heap,
    pub team: Team,
    pub role: Role,
}

/// What a candidate is about, before it is shown.
enum CandidateSubject {
    Question(domain::question::Question),
    Stuck(Turn),
}

/// Something that waits, before it is shown: where it ranks and the item it is on.
struct Candidate {
    heap_index: usize,
    score: f64,
    position: u32,
    item: Item,
    subject: CandidateSubject,
}

impl Candidate {
    /// Where the candidate sorts among those of one item: a stuck turn first, then questions by number.
    fn order_in_item(&self) -> u32 {
        match &self.subject {
            CandidateSubject::Stuck(_) => 0,
            CandidateSubject::Question(question) => question.number.get(),
        }
    }
}

impl Services {
    /// The active heaps of the caller's teams where their role may view the heap, with the team and role, in the
    /// store's heap order (by name, then ID): the tie-break between heaps of the lists that merge several.
    pub(crate) async fn list_viewable_heaps(&self, principal: Principal) -> Result<Vec<ViewableHeap>, ServiceError> {
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let team_ids: Vec<_> = teams.iter().map(|(team, _)| team.id).collect();
        let team_by_id: HashMap<_, (&Team, Role)> = teams.iter().map(|(team, role)| (team.id, (team, *role))).collect();
        Ok(store::heaps::list_heaps_for_teams(&self.db, &team_ids)
            .await?
            .into_iter()
            .filter(|heap| !heap.is_archived())
            .filter_map(|heap| {
                let (team, role) = *team_by_id.get(&heap.team_id)?;
                Permission::ViewHeap.is_granted_to(role).then(|| ViewableHeap {
                    team: team.clone(),
                    role,
                    heap,
                })
            })
            .collect())
    }

    /// The open questions on open items, and the items whose agent turn is stuck, in the active heaps of the caller's
    /// teams, ordered by the score of their item in its own heap, highest first (every heap weighs the same for now),
    /// then by heap (by name), the item's position in its heap, a stuck row before the item's questions, and the
    /// question's number. Scores are those the heap's list ranks by right now.
    ///
    /// Ranking costs one pass over a heap's open items, so only heaps with something waiting are ranked. Any member
    /// may ask; nobody sees a heap outside their teams.
    pub async fn list_waiting(&self, principal: Principal) -> Result<WaitingView, ServiceError> {
        self.list_waiting_capped(principal, MAX_WAITING_ROWS).await
    }

    /// [`list_waiting`](Self::list_waiting) with at most `max_rows` rows in place of [`MAX_WAITING_ROWS`], so the cap
    /// can be tested without a question for each of its rows.
    pub async fn list_waiting_capped(
        &self,
        principal: Principal,
        max_rows: usize,
    ) -> Result<WaitingView, ServiceError> {
        let viewable = self.list_viewable_heaps(principal).await?;
        let heaps: Vec<Heap> = viewable.iter().map(|viewable| viewable.heap.clone()).collect();
        let team_by_id: HashMap<_, (&Team, Role)> = viewable
            .iter()
            .map(|viewable| (viewable.heap.team_id, (&viewable.team, viewable.role)))
            .collect();
        let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
        let mut waiting = store::questions::list_open_questions_on_open_items(&self.db, &heap_ids).await?;
        let as_of = self.now();
        // A turn with a lapsed run counts it as failed, so it can be stuck before the lapse is written.
        let lapsed: HashSet<TurnId> = store::runs::list_unended_runs_in_heaps(&self.db, &heap_ids, false)
            .await?
            .into_iter()
            .filter(|run| run.is_lapsed(as_of))
            .map(|run| run.turn_id)
            .collect();
        let mut stuck: Vec<(HeapId, Turn)> = store::turns::list_open_turns_in_heaps(&self.db, &heap_ids, false)
            .await?
            .into_iter()
            .map(|(heap_id, turn)| (heap_id, turn.counting_lapse(lapsed.contains(&turn.id))))
            .filter(|(_, turn)| turn.is_stuck())
            .collect();
        if waiting.is_empty() && stuck.is_empty() {
            return Ok(WaitingView {
                count: 0,
                rows: Vec::new(),
            });
        }

        let mut candidates = Vec::with_capacity(waiting.len());
        for (heap_index, heap) in heaps.iter().enumerate() {
            let (mine, rest): (Vec<_>, Vec<_>) = waiting.into_iter().partition(|(id, _)| *id == heap.id);
            waiting = rest;
            let (mine_stuck, rest): (Vec<_>, Vec<_>) = stuck.into_iter().partition(|(id, _)| *id == heap.id);
            stuck = rest;
            if mine.is_empty() && mine_stuck.is_empty() {
                continue;
            }
            let OpenRanking { items, ranking, .. } = rank_open_items(&self.db, heap, as_of).await?;
            let ranks: HashMap<ItemId, (f64, u32)> = ranking
                .iter()
                .map(|ranked| (ranked.item_id, (ranked.score, ranked.position)))
                .collect();
            let by_id: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
            for (_, question) in mine {
                // An item closed since the questions were read is gone from the ranking, and doesn't wait.
                let (Some(item), Some(&(score, position))) =
                    (by_id.get(&question.item_id), ranks.get(&question.item_id))
                else {
                    continue;
                };
                candidates.push(Candidate {
                    heap_index,
                    score,
                    position,
                    item: (*item).clone(),
                    subject: CandidateSubject::Question(question),
                });
            }
            for (_, turn) in mine_stuck {
                let (Some(item), Some(&(score, position))) = (by_id.get(&turn.item_id), ranks.get(&turn.item_id))
                else {
                    continue;
                };
                candidates.push(Candidate {
                    heap_index,
                    score,
                    position,
                    item: (*item).clone(),
                    subject: CandidateSubject::Stuck(turn),
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.heap_index.cmp(&b.heap_index))
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.order_in_item().cmp(&b.order_in_item()))
        });
        let count = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        candidates.truncate(max_rows);

        let user_ids: Vec<UserId> = candidates
            .iter()
            .filter_map(|candidate| match &candidate.subject {
                CandidateSubject::Question(question) => question.asked_by,
                CandidateSubject::Stuck(_) => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let users: HashMap<UserId, User> = store::users::list_users(&self.db, &user_ids)
            .await?
            .into_iter()
            .map(|user| (user.id, user))
            .collect();
        let step_item_ids: Vec<ItemId> = candidates
            .iter()
            .filter(|candidate| matches!(&candidate.subject, CandidateSubject::Question(q) if q.step.is_some()))
            .map(|candidate| candidate.item.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let step_titles: HashMap<(ItemId, StepNumber), Title> = if step_item_ids.is_empty() {
            HashMap::new()
        } else {
            store::steps::list_step_titles(&self.db, &step_item_ids).await?
        };

        let item_ids: Vec<ItemId> = candidates
            .iter()
            .map(|candidate| candidate.item.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let open_turns = turn_views(
            &self.db,
            store::turns::list_open_turns(&self.db, &item_ids).await?.into_values(),
            as_of,
            false,
        )
        .await?;

        let rows = candidates
            .into_iter()
            .filter_map(|candidate| {
                let heap = &heaps[candidate.heap_index];
                let (team, role) = *team_by_id.get(&heap.team_id)?;
                let turn = open_turns.get(&candidate.item.id).cloned();
                let turn_is_stuck = turn.as_ref().is_some_and(TurnView::is_stuck);
                let (subject, step_title, open_questions) = match candidate.subject {
                    CandidateSubject::Question(question) => {
                        let step_title = question
                            .step
                            .and_then(|step| step_titles.get(&(candidate.item.id, step)).cloned());
                        let view = QuestionView {
                            allowed_actions: allowed_question_actions(&candidate.item, heap, role, question.state()),
                            asked_by: question.asked_by.and_then(|id| users.get(&id).cloned()),
                            replied_by: None,
                            // An open question isn't withdrawn.
                            withdrawn_by: None,
                            question,
                        };
                        (WaitingSubject::Question(Box::new(view)), step_title, 1)
                    }
                    CandidateSubject::Stuck(stuck_turn) => {
                        let allowed_actions = allowed_item_actions(&candidate.item, heap, role, true, true, as_of)
                            .into_iter()
                            .filter(|action| matches!(action, ItemAction::Retry | ItemAction::TakeBack))
                            .collect();
                        let view = StuckView {
                            failed_runs: stuck_turn.failed_runs,
                            reason: turn.as_ref().and_then(|turn| turn.stuck_reason.clone()),
                            goal: stuck_turn.goal,
                            allowed_actions,
                        };
                        (WaitingSubject::Stuck(view), None, 0)
                    }
                };
                Some(WaitingRow {
                    // The row's item waits for the person: it has an open question or a stuck turn.
                    with_agent: with_agent(&candidate.item, heap, open_questions, turn.is_some(), turn_is_stuck),
                    turn,
                    heap: heap.clone(),
                    team: team.clone(),
                    step_title,
                    subject,
                    item: candidate.item,
                })
            })
            .collect();

        tracing::debug!(user_id = %principal.user_id, count, "listed waiting");
        Ok(WaitingView { count, rows })
    }
}
