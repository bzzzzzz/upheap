//! Comment use cases: adding a comment to an item.
//!
//! A comment write never changes the item's version, and an item write never changes a comment, so adding one
//! doesn't conflict with someone editing the item (architecture.md, Data model). It answers with the whole item, so
//! a client re-renders from one response.

use domain::audit::AuditAction;
use domain::comment::{Comment, CommentBody, number_comment};
use domain::permission::Permission;
use domain::{CommentId, ItemId};
use store::{StoreError, TransactionTrait};

use crate::access::{authorize_heap_write, find_item_locking_in};
use crate::audit::describe_new_comment;
use crate::items::{ItemChildren, ItemView, build_item_view, in_run};
use crate::{Principal, ServiceError, Services};

impl Services {
    /// Adds a comment to an item, open or closed. The new comment is the one with the highest number in the
    /// returned item. The item's version and `updated_at` don't change.
    ///
    /// Fails with [`CommentError::TooMany`](domain::comment::CommentError::TooMany) when the item has
    /// [`MAX_COMMENTS`](domain::comment::MAX_COMMENTS) already.
    pub async fn add_comment(
        &self,
        principal: Principal,
        item_id: ItemId,
        body: CommentBody,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let found = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, found.heap_id, Permission::EditItems)
            .await
            .map_err(|error| in_run(principal, error))?;
        self.check_run_write_made(&txn, principal, item_id, self.now()).await?;
        // Read again under the heap lock, with locking reads: on MySQL a plain read would see the snapshot fixed by
        // the first read above, not what committed while this waited for the lock.
        let item = find_item_locking_in(&txn, item_id, access.heap.id, found.version).await?;
        let comments = store::comments::list_comments_locking(&txn, item.id).await?;
        let number = number_comment(&comments)?;

        let now = self.now();
        let comment = Comment::new(
            CommentId::from_uuid(self.new_id(now)),
            item.id,
            number,
            body,
            principal.user_id,
            now,
        );
        store::comments::insert_comment(&txn, &comment).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::CommentAdded,
            &item,
            describe_new_comment(&comment),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let children = ItemChildren::read_locking(&txn, item.id).await?;
        let view = build_item_view(&txn, &access.heap, access.membership.role, item, children, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            heap_id = %access.heap.id,
            item_id = %item_id,
            comment = comment.number.get(),
            user_id = %principal.user_id,
            "comment added"
        );
        Ok(view)
    }
}
