//! A note becoming the items made from it, or a step of a task (architecture.md, Data model and Cycle detection and
//! locking).
//!
//! The items are made with `create_item` from an origin. This closes the note as became. A person's close takes back
//! the note's turn like any close. In a run it is also the run's report that the turn is reached: the check that every
//! open task made from the note is ready to be defined, the tasks' stage, the note's close, the end of the run and the
//! turn, and the turns those tasks get when the note was sent all the way are one transaction, so a run that dies
//! between them can't strand a closed note with an open turn. A note made a step of a task closes the same way, and
//! in a run it reaches the turn with the step added.

use domain::audit::AuditAction;
use domain::heap::Heap;
use domain::item::{Item, ItemError, Kind, Stage, StateKind};
use domain::permission::Permission;
use domain::question::QuestionState;
use domain::run::{NotReached, RunEnd, RunResult, made_heaps_check, split_reached_check};
use domain::step::{NewStep, Step, StepState, check_steps_editable, place_step};
use domain::team::Team;
use domain::text::Context;
use domain::turn::{TakeBackWhy, TurnSource};
use domain::{HeapId, ItemId, StepId, Timestamp, TurnId, ValidationError};
use store::{StoreError, TransactionTrait};

use crate::access::authorize_heap_write;
use crate::audit::{GoalMetHow, describe_new_step, describe_new_turn, describe_reached, diff_items};
use crate::items::{ItemChildren, ItemView, build_item_view, commit_if_in_run, in_run};
use crate::work::RunScope;
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Closes the note `item_id` as became, because other items were made from it. Declarative: a note that already
    /// became succeeds, unchanged, without an entry.
    ///
    /// Fails with [`domain::item::ItemError::BecameNeedsNote`] unless the item is a note,
    /// [`domain::item::ItemError::Closed`] when it is done or won't do, and [`ServiceError::NothingMade`] when
    /// nothing was made from it, in any state.
    ///
    /// A person's close takes back the note's open turn and withdraws its open questions, as any close does. In a run
    /// the note must be the run's item, and it is refused with [`ServiceError::NotReached`] (and the run only heard
    /// from) unless it has no open question and every open task made from it has a context and a step that isn't
    /// dropped (those still in the note's heap; one moved away is the person's). Otherwise each of those tasks becomes
    /// defined, the note closes as became, and the run and the turn end as reached; no next turn opens on the note.
    /// Each of those tasks with an open turn that defined meets has it ended as reached, as a person's stage edit
    /// would ([`Services::end_met_turn`], how `became`). When the note's turn was sent all the way, each of those
    /// tasks that isn't started and has no open turn gets one of its own ([`domain::turn::Turn::passed_on`]),
    /// including one whose ended turn was the heap's prepare turn, which the person didn't send; one whose ended
    /// turn the person sent on stays as they left it, with no turn or with that turn's own follow-on.
    pub async fn became_item(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems)
            .await
            .map_err(|error| in_run(principal, error))?;
        let now = self.now();
        let scope = self.check_run_write(&txn, principal, item_id, now).await?;
        let heap = &access.heap;
        let note = store::items::find_item_locking(&txn, item_id)
            .await?
            .filter(|note| note.heap_id == heap.id)
            .ok_or(ServiceError::NotFound)?;

        let Some(became) = note.became(principal.user_id, now)? else {
            let children = ItemChildren::read_locking(&txn, note.id).await?;
            let view = build_item_view(&txn, heap, &access.membership, note, children, now).await?;
            commit_if_in_run(txn, principal).await?;
            return Ok(view);
        };
        let made = store::items::list_items_made_from(&txn, &[note.id], true).await?;
        if made.is_empty() {
            return Err(ServiceError::NothingMade { item_id });
        }

        // Heaps made from the note count as made: a run's became checks them and defines the tasks in them.
        let made_heaps = if scope.is_some() {
            store::heaps::list_heaps_made_from(&txn, &[note.id]).await?
        } else {
            Vec::new()
        };
        let mut defining = vec![heap.id];
        defining.extend(made_heaps.iter().map(|made| made.id));
        let mut turns_taken_back = 0_usize;
        let mut questions_withdrawn = 0_usize;
        let mut defined: Vec<&Item> = Vec::new();
        let mut turns_opened = 0_usize;
        let mut turns_ended = 0_usize;
        let mut sent_on: Vec<ItemId> = Vec::new();
        if scope.is_some() {
            let open_questions = store::questions::list_questions_locking(&txn, note.id)
                .await?
                .iter()
                .filter(|question| question.state() == domain::question::QuestionState::Open)
                .count();
            let mut checked = Vec::new();
            for task in made.iter().filter(|item| to_define(item, &defining)) {
                let steps = store::steps::list_steps_locking(&txn, task.id).await?;
                let live = steps.iter().filter(|step| step.state != StepState::Dropped).count();
                checked.push((task.clone(), live));
            }
            let in_made_heaps: Vec<(Heap, Vec<Item>)> = made_heaps
                .iter()
                .map(|made_heap| {
                    let items = made.iter().filter(|item| item.heap_id == made_heap.id).cloned();
                    (made_heap.clone(), items.collect())
                })
                .collect();
            if let Err(missed) =
                split_reached_check(open_questions, &checked).and_then(|()| made_heaps_check(&in_made_heaps))
            {
                // The run is plainly still going, so it was heard from; the check already touched it.
                commit_if_in_run(txn, principal).await?;
                return Err(missed.into());
            }
            defined.extend(
                made.iter()
                    .filter(|item| to_define(item, &defining) && item.stage != Stage::Defined),
            );
            for task in &defined {
                let edited = Item {
                    stage: Stage::Defined,
                    ..(*task).clone()
                };
                let saved = store::items::update_item(&txn, &edited, task.version, now)
                    .await?
                    .ok_or(ServiceError::VersionConflict {
                        expected: task.version,
                        current: task.version.next(),
                    })?;
                let entry = self.item_audit_entry(
                    heap,
                    principal,
                    AuditAction::ItemUpdated,
                    &saved,
                    diff_items(task, &saved),
                    now,
                );
                store::audit::insert_audit_entry(&txn, &entry).await?;
                let met = store::turns::find_open_turn_locking(&txn, saved.id)
                    .await?
                    .filter(|turn| turn.goal_met_by(saved.stage));
                if let Some(turn) = met {
                    if turn.source == TurnSource::GoAhead {
                        sent_on.push(saved.id);
                    }
                    self.end_met_turn(
                        &txn,
                        heap,
                        principal,
                        &turn,
                        &saved,
                        task.stage,
                        GoalMetHow::Became,
                        now,
                    )
                    .await?;
                    turns_ended += 1;
                }
            }
        } else {
            turns_taken_back = self
                .take_back_open_turn(&txn, heap, principal, note.id, TakeBackWhy::Closed, now)
                .await?
                .into();
            questions_withdrawn = self.withdraw_open_questions(&txn, heap, principal, &note, now).await?;
        }

        let saved = store::items::update_item(&txn, &became, note.version, now)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected: note.version,
                current: note.version.next(),
            })?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::ItemUpdated,
            &saved,
            diff_items(&note, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;

        if let Some(scope) = scope {
            let team = match made_heaps.first() {
                Some(made_heap) => Some(
                    store::teams::find_team(&txn, made_heap.team_id)
                        .await?
                        .ok_or(ServiceError::NotFound)?,
                ),
                None => None,
            };
            let result = became_result(&made, &made_heaps, team.as_ref())?;
            let ended_run = scope.run.ended_as(RunEnd::Reached, now, Some(result), Vec::new(), None);
            store::runs::end_run(&txn, &ended_run).await?;
            self.reach_turn(&txn, heap, principal, &scope, &saved, now).await?;

            // All the way §2: sent all the way, the note's turn goes on to the tasks it became, each defined by now.
            // One the person has started or sent on already stays as the person left it, even when became just ended
            // that turn.
            for task in made.iter().filter(|item| {
                open_task_in(item, heap.id) && item.state.kind() != StateKind::InProgress && !sent_on.contains(&item.id)
            }) {
                if store::turns::find_open_turn_locking(&txn, task.id).await?.is_some() {
                    continue;
                }
                let task = Item {
                    stage: Stage::Defined,
                    ..task.clone()
                };
                let Some(turn) = scope.turn.passed_on(TurnId::from_uuid(self.new_id(now)), &task, now) else {
                    // A turn of one state passes nothing on, to any task.
                    break;
                };
                store::turns::insert_turn(&txn, &turn).await?;
                let entry = self.item_audit_entry(
                    heap,
                    principal,
                    AuditAction::TurnOpened,
                    &task,
                    describe_new_turn(&turn),
                    now,
                );
                store::audit::insert_audit_entry(&txn, &entry).await?;
                turns_opened += 1;
            }
        }

        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, heap, &access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %heap.id,
            item_id = %item_id,
            user_id = %principal.user_id,
            made = made.len(),
            tasks_defined = defined.len(),
            turns_taken_back,
            turns_ended,
            turns_opened,
            questions_withdrawn,
            in_run = principal.run.is_some(),
            "note became"
        );
        Ok(view)
    }

    /// Makes the note `item_id` a step of the open task `task_id` in its heap: the step is at the end of the task, with
    /// the note's title and description as its title and context, and it keeps the note as its origin. The note
    /// closes as became. Not declarative: a note that already became is refused, as a repeat would add a second step.
    ///
    /// Fails with [`domain::item::ItemError::BecameNeedsNote`] unless the item is a note,
    /// [`domain::item::ItemError::Closed`] when the note or the task is closed, [`ServiceError::StepTaskNotInHeap`]
    /// when the task isn't an item of the note's heap, and [`domain::step::StepError`] when the task is a note or has
    /// too many steps.
    ///
    /// A person's close takes back the note's open turn and withdraws its open questions, as any close does. In a run
    /// the note must be the run's item, and it is refused with [`ServiceError::NotReached`] (and the run only heard
    /// from) while it has an open question. Otherwise the run and the turn end as reached; no turn opens on the task,
    /// and its stage, state and turn stay as they are.
    pub async fn note_to_step(
        &self,
        principal: Principal,
        item_id: ItemId,
        task_id: ItemId,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems)
            .await
            .map_err(|error| in_run(principal, error))?;
        let now = self.now();
        let scope = self.check_run_write(&txn, principal, item_id, now).await?;
        let heap = &access.heap;
        let note = store::items::find_item_locking(&txn, item_id)
            .await?
            .filter(|note| note.heap_id == heap.id)
            .ok_or(ServiceError::NotFound)?;
        let became = note
            .became(principal.user_id, now)?
            .ok_or(ItemError::Closed { item_id })?;
        let task = store::items::find_item_locking(&txn, task_id)
            .await?
            .filter(|task| task.heap_id == heap.id)
            .ok_or(ServiceError::StepTaskNotInHeap { item_id: task_id })?;
        check_steps_editable(&task)?;
        let steps = store::steps::list_steps_locking(&txn, task.id).await?;
        let placement = place_step(&task, &steps, None)?;

        let mut turns_taken_back = 0_usize;
        let mut questions_withdrawn = 0_usize;
        if scope.is_some() {
            let open_questions = store::questions::list_questions_locking(&txn, note.id)
                .await?
                .iter()
                .any(|question| question.state() == QuestionState::Open);
            if open_questions {
                // The run is plainly still going, so it was heard from; the check already touched it.
                commit_if_in_run(txn, principal).await?;
                return Err(NotReached::OpenQuestion.into());
            }
        } else {
            turns_taken_back = self
                .take_back_open_turn(&txn, heap, principal, note.id, TakeBackWhy::Closed, now)
                .await?
                .into();
            questions_withdrawn = self.withdraw_open_questions(&txn, heap, principal, &note, now).await?;
        }

        let fields = NewStep {
            title: note.title.clone(),
            context: note
                .description
                .as_ref()
                .map(|description| Context::parse(description.as_str()))
                .transpose()?
                .flatten(),
            after: None,
        };
        let step = Step {
            origin_id: Some(note.id),
            ..Step::new(StepId::from_uuid(self.new_id(now)), task.id, placement, fields, now)
        };
        store::steps::insert_step(&txn, &step).await?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::StepAdded,
            &task,
            describe_new_step(&step),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;

        let saved = store::items::update_item(&txn, &became, note.version, now)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected: note.version,
                current: note.version.next(),
            })?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::ItemUpdated,
            &saved,
            diff_items(&note, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;

        if let Some(scope) = scope {
            let text = format!(
                "Became step {}.{} \"{}\" of {} \"{}\"",
                task.key,
                step.number,
                step.title.as_str(),
                task.key,
                task.title.as_str()
            );
            let result = RunResult::parse(&text.chars().take(RunResult::MAX_CHARS).collect::<String>())?;
            let ended_run = scope.run.ended_as(RunEnd::Reached, now, Some(result), Vec::new(), None);
            store::runs::end_run(&txn, &ended_run).await?;
            self.reach_turn(&txn, heap, principal, &scope, &saved, now).await?;
        }

        let children = ItemChildren::read_locking(&txn, saved.id).await?;
        let view = build_item_view(&txn, heap, &access.membership, saved, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %heap.id,
            item_id = %item_id,
            task_id = %task.id,
            step = step.number.get(),
            user_id = %principal.user_id,
            turns_taken_back,
            questions_withdrawn,
            in_run = principal.run.is_some(),
            "note became a step"
        );
        Ok(view)
    }

    /// Ends the turn of `scope` as reached and records it on `note`, the run's item, which has just closed as became.
    /// No next turn opens on it.
    async fn reach_turn(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &Heap,
        principal: Principal,
        scope: &RunScope,
        note: &Item,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        store::turns::end_turn(txn, &scope.turn.reached(principal.user_id, now)).await?;
        let entry = self.item_audit_entry(
            heap,
            principal,
            AuditAction::TurnReached,
            note,
            describe_reached(scope.turn.goal, note.stage, note.stage),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
    }
}

/// Whether a run's became checks `item` and makes it defined: an open task in the note's heap or in a heap made from
/// the note, `heap_ids`. A task made from the note and moved to another heap since is the person's.
fn to_define(item: &Item, heap_ids: &[HeapId]) -> bool {
    item.kind == Kind::Task && !item.is_closed() && heap_ids.contains(&item.heap_id)
}

/// Whether a note's turn, sent all the way, passes on to `item`: an open task in the note's own heap, `heap_id`. The
/// tasks of a heap made from the note get no turn.
fn open_task_in(item: &Item, heap_id: HeapId) -> bool {
    item.kind == Kind::Task && !item.is_closed() && item.heap_id == heap_id
}

/// What a run says it reached when its note became: each heap made from it, with how many items are in it, and then
/// how many of the other items and each by key and title, cut to the result's longest length. `team` is the team
/// of the heaps.
fn became_result(made: &[Item], made_heaps: &[Heap], team: Option<&Team>) -> Result<RunResult, ValidationError> {
    let noun = |count: usize| if count == 1 { "item" } else { "items" };
    let mut parts: Vec<String> = Vec::new();
    for made_heap in made_heaps {
        let count = made.iter().filter(|item| item.heap_id == made_heap.id).count();
        let slug = team.map_or(String::new(), |team| format!("{}/", team.slug));
        parts.push(format!(
            "heap {slug}{} \"{}\" ({count} {})",
            made_heap.slug,
            made_heap.name.as_str(),
            noun(count)
        ));
    }
    let own: Vec<&Item> = made
        .iter()
        .filter(|item| made_heaps.iter().all(|made_heap| made_heap.id != item.heap_id))
        .collect();
    let items: Vec<String> = own
        .iter()
        .map(|item| format!("{} \"{}\"", item.key, item.title.as_str()))
        .collect();
    let mut text = String::from("Became");
    if !parts.is_empty() {
        text.push_str(&format!(" {}", parts.join(", ")));
    }
    if !own.is_empty() {
        if !parts.is_empty() {
            text.push_str(" and");
        }
        text.push_str(&format!(" {} {}: {}", own.len(), noun(own.len()), items.join(", ")));
    }
    RunResult::parse(&text.chars().take(RunResult::MAX_CHARS).collect::<String>())
}
