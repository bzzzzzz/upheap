//! The handles an item had before a move (`item_former_handles`), so an old link still names the item.
//!
//! One row per move: the heap and key the item left, and when. The index on `(heap_id, key)` is not unique, since
//! several items can have held a handle in turn, and the lookup takes the latest. Rows are never deleted.
//!
//! Moves made before this are filled in from the audit log. An `item_moved` entry holds the heap the item left, and
//! its key only when the key changed. Each item's moves are walked from the latest to the earliest, starting from
//! its current key: the key it left is the entry's `key.before`, or else the key it had after the move, which is the
//! one the walk is at. An entry whose diff can't be read fails the migration. Moves are few, since the feature is
//! new, so every `item_moved` entry is read at once.

use std::collections::BTreeMap;

use domain::handle::ItemKey;
use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len};
use sea_orm_migration::sea_orm::ConnectionTrait;
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemFormerHandles {
    Table,
    Id,
    ItemId,
    HeapId,
    Key,
    MovedAt,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
    Key,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Id,
    EntityType,
    EntityId,
    Action,
    Diff,
    CreatedAt,
}

/// A move read from an `item_moved` entry.
#[derive(Debug, PartialEq, Eq)]
struct Move {
    heap_before: String,
    key_before: Option<String>,
    at: i64,
}

/// A handle an item held before a move.
#[derive(Debug, PartialEq, Eq)]
struct Former {
    heap_id: String,
    key: String,
    moved_at: i64,
}

/// The handles an item left, from its moves in time order and the key it has now.
fn walk(current_key: &str, moves: &[Move]) -> Vec<Former> {
    let mut key = current_key.to_owned();
    let mut formers = Vec::with_capacity(moves.len());
    for made in moves.iter().rev() {
        if let Some(before) = &made.key_before {
            key.clone_from(before);
        }
        formers.push(Former {
            heap_id: made.heap_before.clone(),
            key: key.clone(),
            moved_at: made.at,
        });
    }
    formers
}

fn read_move(entry_id: &str, diff: &str, at: i64) -> Result<Move, DbErr> {
    let unreadable = |why: &str| DbErr::Migration(format!("audit entry {entry_id} (item_moved): {why}"));
    let diff: serde_json::Value = serde_json::from_str(diff).map_err(|e| unreadable(&e.to_string()))?;
    let heap_before = diff["heap"]["before"]
        .as_str()
        .ok_or_else(|| unreadable("no heap.before"))?
        .to_owned();
    let key_before = match &diff["key"]["before"] {
        serde_json::Value::Null => None,
        serde_json::Value::String(key) => Some(key.clone()),
        _ => return Err(unreadable("key.before is not text")),
    };
    Ok(Move {
        heap_before,
        key_before,
        at,
    })
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let key_len = <u32 as std::convert::TryFrom<usize>>::try_from(ItemKey::LEN).unwrap_or(u32::MAX);
        manager
            .create_table(
                Table::create()
                    .table(ItemFormerHandles::Table)
                    .col(char_len(ItemFormerHandles::Id, ID_LEN).primary_key())
                    .col(char_len(ItemFormerHandles::ItemId, ID_LEN))
                    .col(char_len(ItemFormerHandles::HeapId, ID_LEN))
                    .col(char_len(ItemFormerHandles::Key, key_len))
                    .col(big_integer(ItemFormerHandles::MovedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_former_handles_item")
                            .from(ItemFormerHandles::Table, ItemFormerHandles::ItemId)
                            .to(Items::Table, Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_former_handles_heap")
                            .from(ItemFormerHandles::Table, ItemFormerHandles::HeapId)
                            .to(Heaps::Table, Heaps::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_item_former_handles_handle")
                    .table(ItemFormerHandles::Table)
                    .col(ItemFormerHandles::HeapId)
                    .col(ItemFormerHandles::Key)
                    .to_owned(),
            )
            .await?;
        backfill(manager.get_connection()).await
    }
}

async fn backfill(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    let entries = db
        .query_all(
            &Query::select()
                .columns([AuditLog::Id, AuditLog::EntityId, AuditLog::Diff, AuditLog::CreatedAt])
                .from(AuditLog::Table)
                .and_where(Expr::col(AuditLog::EntityType).eq("item"))
                .and_where(Expr::col(AuditLog::Action).eq("item_moved"))
                .order_by(AuditLog::CreatedAt, Order::Asc)
                .order_by(AuditLog::Id, Order::Asc)
                .to_owned(),
        )
        .await?;
    let mut moves: BTreeMap<String, Vec<Move>> = BTreeMap::new();
    for row in entries {
        let id: String = row.try_get("", "id")?;
        let item_id: String = row.try_get("", "entity_id")?;
        let diff: String = row.try_get("", "diff")?;
        let at: i64 = row.try_get("", "created_at")?;
        moves.entry(item_id).or_default().push(read_move(&id, &diff, at)?);
    }
    for (item_id, moves) in moves {
        let row = db
            .query_one(
                &Query::select()
                    .column(Items::Key)
                    .from(Items::Table)
                    .and_where(Expr::col(Items::Id).eq(item_id.as_str()))
                    .to_owned(),
            )
            .await?
            .ok_or_else(|| DbErr::Migration(format!("an item_moved entry names the missing item {item_id}")))?;
        let current_key: String = row.try_get("", "key")?;
        for former in walk(&current_key, &moves) {
            db.execute(
                &Query::insert()
                    .into_table(ItemFormerHandles::Table)
                    .columns([
                        ItemFormerHandles::Id,
                        ItemFormerHandles::ItemId,
                        ItemFormerHandles::HeapId,
                        ItemFormerHandles::Key,
                        ItemFormerHandles::MovedAt,
                    ])
                    .values_panic([
                        new_id(former.moved_at)?.into(),
                        item_id.clone().into(),
                        former.heap_id.into(),
                        former.key.into(),
                        former.moved_at.into(),
                    ])
                    .to_owned(),
            )
            .await?;
        }
    }
    Ok(())
}

/// A new UUIDv7 stamped with `millis`, as the store's IDs are.
fn new_id(millis: i64) -> Result<String, DbErr> {
    let millis = <u64 as std::convert::TryFrom<i64>>::try_from(millis)
        .map_err(|_| DbErr::Migration(format!("a move was made before 1970 ({millis})")))?;
    let nanos = <u32 as std::convert::TryFrom<u64>>::try_from((millis % 1000) * 1_000_000)
        .map_err(|_| DbErr::Migration("under a second of nanoseconds fits in u32".to_owned()))?;
    let ts = uuid::Timestamp::from_unix(uuid::NoContext, millis / 1000, nanos);
    Ok(Uuid::new_v7(ts).hyphenated().to_string())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn made(heap: &str, key: Option<&str>, at: i64) -> Move {
        Move {
            heap_before: heap.to_owned(),
            key_before: key.map(str::to_owned),
            at,
        }
    }

    #[gtest]
    fn the_walk_goes_from_the_current_key_back_through_each_move() {
        // A (k3x9a) -> B keeping the key -> C taking m4n5p.
        let moves = [made("a", None, 10), made("b", Some("k3x9a"), 20)];

        let formers = walk("m4n5p", &moves);

        expect_that!(
            formers,
            elements_are![
                eq(&Former {
                    heap_id: "b".into(),
                    key: "k3x9a".into(),
                    moved_at: 20
                }),
                eq(&Former {
                    heap_id: "a".into(),
                    key: "k3x9a".into(),
                    moved_at: 10
                }),
            ]
        );
    }
}
