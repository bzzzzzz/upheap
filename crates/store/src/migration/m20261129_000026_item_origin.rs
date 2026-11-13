//! The item an item was made from (`items.origin_item_id`).
//!
//! It is set when the item is made from another, in a split, and never changed. It points to an item of the same
//! heap at that time, which the service checks; either item moving to another heap later keeps the link. The index
//! serves lookups of what was made from an item. Every item stored before this migration has `NULL`. The new state
//! code `became` needs nothing here: the state column is plain text.
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
    OriginItemId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE items ADD COLUMN origin_item_id char(36) NULL \
                     REFERENCES items (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(Items::Table)
                        .add_column(ColumnDef::new(Items::OriginItemId).char_len(36).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_items_origin")
                                .from_tbl(Items::Table)
                                .from_col(Items::OriginItemId)
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
                    .name("idx_items_origin")
                    .table(Items::Table)
                    .col(Items::OriginItemId)
                    .to_owned(),
            )
            .await
    }
}
