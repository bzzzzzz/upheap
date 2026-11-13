//! A note becoming the items made from it (architecture.md, Data model and Cycle detection and locking).
//!
//! The items are made with `create_item` from an origin. This closes the note as became. A person's close takes back
//! the note's turn like any close. In a run it is also the run's report that the turn is reached: the check that every
//! open task made from the note is ready to be defined, the tasks' stage, the note's close and the end of the run and
//! the turn are one transaction, so a run that dies between them can't strand a closed note with an open turn.

use domain::audit::AuditAction;
use domain::item::{Item, Kind, Stage};
use domain::permission::Permission;
use domain::run::{RunEnd, RunResult, split_reached_check};
use domain::step::StepState;
use domain::turn::TakeBackWhy;
use domain::{HeapId, ItemId, ValidationError};
use store::{StoreError, TransactionTrait};

use crate::access::authorize_heap_write;
use crate::audit::{describe_reached, diff_items};
use crate::items::{ItemChildren, ItemView, build_item_view, commit_if_in_run, in_run};
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
    /// defined, the note closes as became, and the run and the turn end
    /// as reached; no next turn opens.
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

        let mut turns_taken_back = 0_usize;
        let mut questions_withdrawn = 0_usize;
        let mut defined: Vec<&Item> = Vec::new();
        if scope.is_some() {
            let open_questions = store::questions::list_questions_locking(&txn, note.id)
                .await?
                .iter()
                .filter(|question| question.state() == domain::question::QuestionState::Open)
                .count();
            let mut checked = Vec::new();
            for task in made.iter().filter(|item| to_define(item, heap.id)) {
                let steps = store::steps::list_steps_locking(&txn, task.id).await?;
                let live = steps.iter().filter(|step| step.state != StepState::Dropped).count();
                checked.push((task.clone(), live));
            }
            if let Err(missed) = split_reached_check(open_questions, &checked) {
                // The run is plainly still going, so it was heard from; the check already touched it.
                commit_if_in_run(txn, principal).await?;
                return Err(missed.into());
            }
            defined.extend(
                made.iter()
                    .filter(|item| to_define(item, heap.id) && item.stage != Stage::Defined),
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
            let result = became_result(&made)?;
            let ended_run = scope.run.ended_as(RunEnd::Reached, now, Some(result), Vec::new(), None);
            store::runs::end_run(&txn, &ended_run).await?;
            store::turns::end_turn(&txn, &scope.turn.reached(principal.user_id, now)).await?;
            let entry = self.item_audit_entry(
                heap,
                principal,
                AuditAction::TurnReached,
                &saved,
                describe_reached(scope.turn.goal, saved.stage, saved.stage),
                now,
            );
            store::audit::insert_audit_entry(&txn, &entry).await?;
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
            questions_withdrawn,
            in_run = principal.run.is_some(),
            "note became"
        );
        Ok(view)
    }
}

/// Whether a run's became checks `item` and makes it defined: an open task in the note's heap, `heap_id`, which is
/// the heap this holds the lock of. A task made from the note and moved to another heap since is the person's.
fn to_define(item: &Item, heap_id: HeapId) -> bool {
    item.kind == Kind::Task && !item.is_closed() && item.heap_id == heap_id
}

/// What a run says it reached when its note became: how many items, and each by key and title, cut to the result's
/// longest length.
fn became_result(made: &[Item]) -> Result<RunResult, ValidationError> {
    let items: Vec<String> = made
        .iter()
        .map(|item| format!("{} \"{}\"", item.key, item.title.as_str()))
        .collect();
    let noun = if made.len() == 1 { "item" } else { "items" };
    let text = format!("Became {} {noun}: {}", made.len(), items.join(", "));
    RunResult::parse(&text.chars().take(RunResult::MAX_CHARS).collect::<String>())
}
