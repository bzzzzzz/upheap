//! Question use cases: asking a question or making a request on an item, and replying to one.
//!
//! A question write never changes the item's version, and an item write never changes a question's, so replying
//! doesn't conflict with someone editing the item (architecture.md, Data model). Both use cases return the whole
//! item, so a client re-renders from one response.

use domain::audit::AuditAction;
use domain::permission::Permission;
use domain::question::{
    NewQuestion, Question, QuestionError, QuestionNumber, ReplyError, check_questions_editable, number_question,
};
use domain::text::Context;
use domain::{ItemId, QuestionId, Version};
use store::{StoreError, TransactionTrait};

use crate::access::authorize_heap_write;
use crate::audit::{describe_new_question, describe_reply};
use crate::items::{ItemChildren, ItemView, build_item_view};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Asks a question or makes a request on an item. The new question is the one with the highest number in the
    /// returned item.
    ///
    /// Fails with [`ItemError::Closed`](domain::item::ItemError::Closed) on a closed item, and with
    /// [`QuestionError::TooMany`] or [`QuestionError::StepNotFound`] when the question can't go there.
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
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = store::items::find_item_locking(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        check_questions_editable(&item)?;
        let questions = store::questions::list_questions_locking(&txn, item.id).await?;
        let step_numbers: Vec<_> = if fields.step.is_some() {
            store::steps::list_steps_locking(&txn, item.id)
                .await?
                .into_iter()
                .map(|step| step.number)
                .collect()
        } else {
            Vec::new()
        };
        let number = number_question(&item, &questions, &step_numbers, &fields)?;

        let now = self.now();
        let question = Question::new(
            QuestionId::from_uuid(self.new_id(now)),
            item.id,
            number,
            fields,
            principal.user_id,
            now,
        );
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
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
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
    /// [`QuestionError::Answered`] when it already has a reply, and with a validation error when the reply doesn't
    /// fit the question.
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
        let item = store::items::find_item_locking(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
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
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            question = number.get(),
            user_id = %principal.user_id,
            version = saved.version.get(),
            "question answered"
        );
        Ok(view)
    }
}
