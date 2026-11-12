//! An item's comments. Each is its own row, written once and never changed, so adding one never touches the item's
//! version. Comments are never deleted.

use domain::comment::{Comment, CommentBody, CommentNumber};
use domain::{CommentId, ItemId, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db};
use crate::entities::item_comments;

const TABLE: &str = "item_comments";

/// Inserts a new comment.
pub async fn insert_comment(db: &impl ConnectionTrait, comment: &Comment) -> Result<(), StoreError> {
    item_comments::Entity::insert(to_row(comment)?)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// The comments of `item_id` in number order.
pub async fn list_comments(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Comment>, StoreError> {
    read_comments(db, item_id, false).await
}

/// The comments of `item_id` in number order, read with a locking read (`FOR SHARE` on MySQL and Postgres), which
/// sees the latest committed rows rather than the transaction's snapshot. Adding a comment calls it after
/// [`crate::heaps::lock_heap`], so on MySQL (REPEATABLE READ) it doesn't work from the snapshot fixed by an earlier
/// plain read: two racing comments then can't pick the same number, and the cap of
/// [`domain::comment::MAX_COMMENTS`] holds. SQLite has no row locks and needs none (see `lock_heap`).
pub async fn list_comments_locking(db: &impl ConnectionTrait, item_id: ItemId) -> Result<Vec<Comment>, StoreError> {
    read_comments(db, item_id, true).await
}

async fn read_comments(db: &impl ConnectionTrait, item_id: ItemId, lock: bool) -> Result<Vec<Comment>, StoreError> {
    let query = item_comments::Entity::find()
        .filter(item_comments::Column::ItemId.eq(id_to_db(item_id.as_uuid())))
        .order_by_asc(item_comments::Column::Number);
    let rows = crate::items::shared_if(query, lock).all(db).await?;
    rows.into_iter().map(to_comment).collect()
}

fn to_row(comment: &Comment) -> Result<item_comments::ActiveModel, StoreError> {
    let id = id_to_db(comment.id.as_uuid());
    let number = i32::try_from(comment.number.get()).map_err(|_| StoreError::corrupt(TABLE, &id, "number overflow"))?;
    Ok(item_comments::ActiveModel {
        id: Set(id),
        item_id: Set(id_to_db(comment.item_id.as_uuid())),
        number: Set(number),
        body: Set(comment.body.as_str().to_owned()),
        created_by: Set(id_to_db(comment.created_by.as_uuid())),
        created_at: Set(comment.created_at.as_millis()),
    })
}

fn to_comment(row: item_comments::Model) -> Result<Comment, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let number = u32::try_from(row.number)
        .ok()
        .and_then(CommentNumber::new)
        .ok_or_else(|| read.corrupt(format!("number {} is not positive", row.number)))?;
    Ok(Comment {
        id: CommentId::from_uuid(read.uuid(&row.id)?),
        item_id: ItemId::from_uuid(read.uuid(&row.item_id)?),
        number,
        body: CommentBody::from_stored(row.body),
        created_by: UserId::from_uuid(read.uuid(&row.created_by)?),
        created_at: read.timestamp(row.created_at)?,
    })
}
