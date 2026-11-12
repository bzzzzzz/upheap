//! Comments on an item: an append-only thread under its context. A comment is its own row and has no `version`: it is
//! never changed, and adding one never bumps the item's.
//!
//! `number` is given when the comment is added and never changes or repeats; the unique index on `(item_id, number)`
//! backs the service's numbering under the heap lock. `body` may be 20,000 characters, up to 80,000 bytes in UTF-8,
//! which MySQL's `TEXT` can't hold, so it is `MEDIUMTEXT` there (see `m20261122_000019_long_text_on_mysql`). Rows
//! are never deleted.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len, integer, text};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemComments {
    Table,
    Id,
    ItemId,
    Number,
    Body,
    CreatedBy,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

/// The column with its nullability, which `MODIFY` must state again.
const MYSQL_LONG_TEXT: &str = "ALTER TABLE item_comments MODIFY body MEDIUMTEXT NOT NULL";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ItemComments::Table)
                    .col(char_len(ItemComments::Id, ID_LEN).primary_key())
                    .col(char_len(ItemComments::ItemId, ID_LEN))
                    .col(integer(ItemComments::Number))
                    .col(text(ItemComments::Body))
                    .col(char_len(ItemComments::CreatedBy, ID_LEN))
                    .col(big_integer(ItemComments::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_comments_item")
                            .from(ItemComments::Table, ItemComments::ItemId)
                            .to(Items::Table, Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_comments_created_by")
                            .from(ItemComments::Table, ItemComments::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_item_comments_number")
                    .table(ItemComments::Table)
                    .col(ItemComments::ItemId)
                    .col(ItemComments::Number)
                    .unique()
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            manager.get_connection().execute_unprepared(MYSQL_LONG_TEXT).await?;
        }
        Ok(())
    }
}
