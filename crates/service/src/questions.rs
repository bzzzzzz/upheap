//! Question use cases: asking a question or making a request on an item, and replying to one.
//!
//! A question write never changes the item's version, and an item write never changes a question's, so replying
//! doesn't conflict with someone editing the item (architecture.md, Data model). Both use cases return the whole
//! item, so a client re-renders from one response.

use domain::audit::AuditAction;
use domain::item::Item;
use domain::permission::Permission;
use domain::question::{
    AGREE_PICK, NewQuestion, Question, QuestionError, QuestionKind, QuestionNumber, ReplyError,
    check_questions_editable, check_task_ask, next_round, number_question,
};
use domain::text::Context;
use domain::{ItemId, QuestionId, Version};
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap_write, find_item_locking_in};
use crate::audit::{describe_new_question, describe_reply};
use crate::items::{ItemChildren, ItemView, TurnOnClose, build_item_view, in_run};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Asks a question or makes a request on an item. The new question is the one with the highest number in the
    /// returned item.
    ///
    /// Fails with [`ItemError::Closed`](domain::item::ItemError::Closed) on a closed item, and with
    /// [`QuestionError::TooMany`] or [`QuestionError::StepNotFound`] when the question can't go there.
    ///
    /// A review or a done proposal is asked only in a run whose turn goes to done, on a task: otherwise
    /// [`QuestionError::NotOnTask`] or [`QuestionError::NeedsDoTurn`]. A review also fails with
    /// [`QuestionError::ReviewOpen`], and a done proposal with [`QuestionError::NotReadyToClose`] listing what is left.
    /// A review's round is one more than the answered reviews of its piece.
    pub async fn ask_question(
        &self,
        principal: Principal,
        item_id: ItemId,
        fields: NewQuestion,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems)
            .await
            .map_err(|error| in_run(principal, error))?;
        let run = self.check_run_write(&txn, principal, item_id, self.now()).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        check_questions_editable(&item)?;
        let questions = store::questions::list_questions_locking(&txn, item.id).await?;
        let steps = if fields.step.is_some() || fields.kind == QuestionKind::DoneProposal {
            store::steps::list_steps_locking(&txn, item.id).await?
        } else {
            Vec::new()
        };
        let step_numbers: Vec<_> = steps.iter().map(|step| step.number).collect();
        // The goal of the turn the run works to; the run check above made sure that turn is open.
        let goal = match run {
            Some(_) => store::turns::find_open_turn_locking(&txn, item.id)
                .await?
                .map(|turn| turn.goal),
            None => None,
        };
        check_task_ask(&item, goal, &steps, &questions, &fields)?;
        let number = number_question(&item, &questions, &step_numbers, &fields)?;
        let round = fields
            .review
            .as_ref()
            .map(|target| next_round(&questions, &target.piece));

        let now = self.now();
        let question = Question::new(
            QuestionId::from_uuid(self.new_id(now)),
            item.id,
            number,
            fields,
            principal.user_id,
            now,
        )
        .asked_in(principal.run);
        let question = match round {
            Some(round) => question.in_round(round),
            None => question,
        };
        store::questions::insert_question(&txn, &question).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::QuestionAsked,
            &item,
            describe_new_question(&question),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, &access.membership, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            question = question.number.get(),
            kind = question.kind.code(),
            user_id = %principal.user_id,
            "question asked"
        );
        Ok(view)
    }

    /// Replies to question `number` of an item if the question is still at version `expected`, picking `picked`
    /// (indices into its choices) and saying `comment`. Fails with [`ServiceError::NotFound`] when the item has
    /// no such question, with [`ItemError::Closed`](domain::item::ItemError::Closed) on a closed item, with
    /// [`QuestionError::Answered`] when it already has a reply, with [`QuestionError::Withdrawn`] when closing the
    /// item withdrew it, and with a validation error when the reply doesn't
    /// fit the question.
    ///
    /// Agreeing to a done proposal (the first choice, Close as done) closes the task as done in the same transaction:
    /// the item's open turn ends as reached by the person, any other open question is withdrawn, and the items the
    /// task blocks are freed, as when the person closes it by hand.
    pub async fn reply_to_question(
        &self,
        principal: Principal,
        item_id: ItemId,
        number: QuestionNumber,
        picked: &[u32],
        comment: Option<Context>,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        let questions = store::questions::list_questions_locking(&txn, item.id).await?;
        let current = questions
            .iter()
            .find(|question| question.number == number)
            .ok_or(ServiceError::NotFound)?;
        check_questions_editable(&item)?;
        // Before the version: someone else's reply moved it, and "already answered" is the more useful thing to say.
        if current.reply.is_some() {
            return Err(QuestionError::Answered { number }.into());
        }
        if current.withdrawal.is_some() {
            return Err(QuestionError::Withdrawn { number }.into());
        }
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        let now = self.now();
        let answered = current
            .reply(picked, comment, principal.user_id, now)
            .map_err(|error| match error {
                ReplyError::Question(error) => ServiceError::Question(error),
                ReplyError::Invalid(error) => ServiceError::Invalid(error),
            })?;
        let saved = store::questions::update_question(&txn, &answered, expected)
            .await?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::QuestionAnswered,
            &item,
            describe_reply(&saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let agreed_to_close = saved.kind == QuestionKind::DoneProposal
            && saved.reply.as_ref().is_some_and(|reply| reply.picked == [AGREE_PICK]);
        let item = if agreed_to_close {
            self.close_on_proposal(&txn, &access.heap, principal, item, now).await?
        } else {
            item
        };
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, &access.membership, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            closed = agreed_to_close,
            question = number.get(),
            user_id = %principal.user_id,
            version = saved.version.get(),
            "question answered"
        );
        Ok(view)
    }

    /// Closes `item` as done on the person's agreement to a done proposal, by the same transition as their own Done:
    /// the open turn ends as reached, other open questions are withdrawn, the items it blocks are freed, and the item
    /// is written with its entry. Returns the closed item. The caller holds the heap lock.
    async fn close_on_proposal(
        &self,
        txn: &store::DatabaseTransaction,
        heap: &domain::heap::Heap,
        principal: Principal,
        item: Item,
        now: domain::Timestamp,
    ) -> Result<Item, ServiceError> {
        let patch = domain::item::ItemPatch {
            state: Some(domain::item::StateKind::Done),
            ..domain::item::ItemPatch::default()
        };
        let edited = item
            .apply_patch(&patch, principal.user_id, now)?
            .ok_or_else(|| ServiceError::Item(domain::item::ItemError::Closed { item_id: item.id }))?;
        let (turns, withdrawn) = self
            .end_work_for_close(txn, heap, principal, &item, TurnOnClose::Reached, now)
            .await?;
        let (saved, freed) = self.save_edited_item(txn, heap, principal, &item, &edited, now).await?;
        tracing::info!(item_id = %item.id, turns, withdrawn, freed, "item closed on a done proposal");
        Ok(saved)
    }
}
