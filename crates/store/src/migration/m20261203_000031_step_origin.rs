//! Where a step came from: the note that became it (`item_steps.origin_id`).
//!
//! `NULL` for every step that exists now and for every step a person or a run adds directly, so no row needs a
//! backfill. Items are archived, never deleted, so the foreign key restricts. As in the item origin migration, SQLite
//! gets the foreign key on the added column itself, since it can't add a constraint to an existing table, and MySQL
//! and Postgres get a named constraint. The index serves the lookup of the steps a note became (architecture.md,
//! Data model).

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum ItemSteps {
    Table,
    OriginId,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

const ID_LEN: u32 = 36;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE item_steps ADD COLUMN origin_id char(36) NULL \
                     REFERENCES items (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(ItemSteps::Table)
                        .add_column(ColumnDef::new(ItemSteps::OriginId).char_len(ID_LEN).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_item_steps_origin")
                                .from_tbl(ItemSteps::Table)
                                .from_col(ItemSteps::OriginId)
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
                    .name("idx_item_steps_origin")
                    .table(ItemSteps::Table)
                    .col(ItemSteps::OriginId)
                    .to_owned(),
            )
            .await
    }
}
