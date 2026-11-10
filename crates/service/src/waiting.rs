//! Waiting for me: every open question on an open item across the caller's heaps, most important first.

use std::collections::{BTreeSet, HashMap};

use domain::heap::Heap;
use domain::item::{Item, StateKind, Title};
use domain::permission::allowed_question_actions;
use domain::ranking::{CURRENT_FORMULA, ItemForRanking, rank};
use domain::step::StepNumber;
use domain::team::{Role, Team};
use domain::user::User;
use domain::{HeapId, ItemId, UserId};

use crate::items::QuestionView;
use crate::{Principal, ServiceError, Services};

/// Most rows [`Services::list_waiting`] returns. A person waits on tens of questions, not thousands, so one
/// response holds them all, and this bounds a pathological one. The count still says how many there are.
pub const MAX_WAITING_ROWS: usize = 500;

/// What waits for the caller.
#[derive(Debug)]
pub struct WaitingView {
    /// How many questions wait, whether or not they all fit in `rows`.
    pub count: u32,
    /// The first [`MAX_WAITING_ROWS`] of them, most important first.
    pub rows: Vec<WaitingRow>,
}

/// One open question, with the item and heap it is on.
#[derive(Debug)]
pub struct WaitingRow {
    pub item: Item,
    pub heap: Heap,
    pub team: Team,
    /// The title of the step the question names, when it names one that exists.
    pub step_title: Option<Title>,
    pub question: QuestionView,
}

/// A question that waits, before it is shown: where it ranks and the item it is on.
struct Candidate {
    heap_index: usize,
    score: f64,
    position: u32,
    item: Item,
    question: domain::question::Question,
}

impl Services {
    /// The open questions on open items in the active heaps of the caller's teams, ordered by the score of their
    /// item in its own heap, highest first (every heap weighs the same for now), then by heap name, the item's
    /// position in its heap and the question's number. Scores are those the heap's list ranks by right now.
    ///
    /// Ranking costs one pass over a heap's open items, so only heaps with a waiting question are ranked. Any member
    /// may ask; nobody sees a heap outside their teams.
    pub async fn list_waiting(&self, principal: Principal) -> Result<WaitingView, ServiceError> {
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let team_ids: Vec<_> = teams.iter().map(|(team, _)| team.id).collect();
        let heaps: Vec<Heap> = store::heaps::list_heaps_for_teams(&self.db, &team_ids)
            .await?
            .into_iter()
            .filter(|heap| !heap.is_archived())
            .collect();
        let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
        let mut waiting = store::questions::list_open_questions_on_open_items(&self.db, &heap_ids).await?;
        if waiting.is_empty() {
            return Ok(WaitingView {
                count: 0,
                rows: Vec::new(),
            });
        }

        let as_of = self.now();
        let states = [StateKind::Open, StateKind::InProgress, StateKind::Blocked];
        let mut candidates = Vec::with_capacity(waiting.len());
        for (heap_index, heap) in heaps.iter().enumerate() {
            let (mine, rest): (Vec<_>, Vec<_>) = waiting.into_iter().partition(|(id, _)| *id == heap.id);
            waiting = rest;
            if mine.is_empty() {
                continue;
            }
            // Every open item is ranked, because a blocker inherits from what it blocks.
            let items = store::items::list_items_in_states(&self.db, heap.id, &states).await?;
            let parameters = store::parameters::list_parameters(&self.db, heap.id).await?;
            let inputs: Vec<ItemForRanking<'_>> = items.iter().map(ItemForRanking::from).collect();
            let ranking = rank(CURRENT_FORMULA, &heap.policy, &parameters, &inputs, as_of);
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
                    question,
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| heaps[a.heap_index].name.as_str().cmp(heaps[b.heap_index].name.as_str()))
                .then_with(|| heaps[a.heap_index].id.cmp(&heaps[b.heap_index].id))
                .then_with(|| a.position.cmp(&b.position))
                .then_with(|| a.question.number.cmp(&b.question.number))
        });
        let count = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        candidates.truncate(MAX_WAITING_ROWS);

        let user_ids: Vec<UserId> = candidates
            .iter()
            .filter_map(|candidate| candidate.question.asked_by)
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
            .filter(|candidate| candidate.question.step.is_some())
            .map(|candidate| candidate.item.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let step_titles: HashMap<(ItemId, StepNumber), Title> = if step_item_ids.is_empty() {
            HashMap::new()
        } else {
            store::steps::list_step_titles(&self.db, &step_item_ids).await?
        };
        let teams: HashMap<_, (&Team, Role)> = teams.iter().map(|(team, role)| (team.id, (team, *role))).collect();

        let rows = candidates
            .into_iter()
            .filter_map(|candidate| {
                let heap = &heaps[candidate.heap_index];
                let (team, role) = *teams.get(&heap.team_id)?;
                let question = candidate.question;
                let step_title = question
                    .step
                    .and_then(|step| step_titles.get(&(candidate.item.id, step)).cloned());
                Some(WaitingRow {
                    heap: heap.clone(),
                    team: team.clone(),
                    step_title,
                    question: QuestionView {
                        allowed_actions: allowed_question_actions(&candidate.item, heap, role, question.state()),
                        asked_by: question.asked_by.and_then(|id| users.get(&id).cloned()),
                        replied_by: None,
                        question,
                    },
                    item: candidate.item,
                })
            })
            .collect();

        tracing::debug!(user_id = %principal.user_id, count, "listed waiting questions");
        Ok(WaitingView { count, rows })
    }
}
