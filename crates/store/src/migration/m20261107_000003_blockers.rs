//! The item a blocked item waits for.
//!
//! `blocked_by_item_id` is set only while the item is blocked. It points to an item of the same heap when it is set,
//! which the service checks, as it checks that no blocking cycle forms; a blocker moved to another heap later keeps
//! the link (decisions.md §35). The index serves lookups of what an item blocks.
//!
//! SQLite can't add a foreign key to an existing table with `ALTER TABLE … ADD CONSTRAINT`, but it accepts one on
//! a column added with `ADD COLUMN`, so the column is added with its reference there. MySQL gets the usual
//! named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
    BlockedByItemId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE items ADD COLUMN blocked_by_item_id char(36) NULL \
                     REFERENCES items (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(Items::Table)
                        .add_column(ColumnDef::new(Items::BlockedByItemId).char_len(36).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_items_blocked_by")
                                .from_tbl(Items::Table)
                                .from_col(Items::BlockedByItemId)
                                .to_tbl(Items::Table)
                                .to_col(Items::Id)
                                .on_delete(ForeignKeyAction::Restrict)
                                .on_update(ForeignKeyAction::Restrict),
                        )
                        .to_owned(),
                )
                .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("idx_items_blocked_by")
                    .table(Items::Table)
                    .col(Items::BlockedByItemId)
                    .to_owned(),
            )
            .await
    }
}
