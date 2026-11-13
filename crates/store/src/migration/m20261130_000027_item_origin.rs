//! Where an item came from: the item whose run added it (`items.origin_id`) and that run (`items.created_run_id`).
//!
//! Both are set together for an item an agent run added, and stay `NULL` for every item that exists now and for
//! every item a person adds, so no row needs a backfill. Items are archived, never deleted, so the foreign keys
//! restrict. As in the creator and run migrations, SQLite gets each foreign key on the added column itself, since it
//! can't add a constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
    OriginId,
    CreatedRunId,
}

#[derive(DeriveIden)]
enum ItemRuns {
    Table,
    Id,
}

const ID_LEN: u32 = 36;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            let db = manager.get_connection();
            db.execute_unprepared(
                "ALTER TABLE items ADD COLUMN origin_id char(36) NULL \
                 REFERENCES items (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
            )
            .await?;
            db.execute_unprepared(
                "ALTER TABLE items ADD COLUMN created_run_id char(36) NULL \
                 REFERENCES item_runs (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
            )
            .await?;
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::OriginId).char_len(ID_LEN).null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_items_origin")
                            .from_tbl(Items::Table)
                            .from_col(Items::OriginId)
                            .to_tbl(Items::Table)
                            .to_col(Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::CreatedRunId).char_len(ID_LEN).null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_items_created_run")
                            .from_tbl(Items::Table)
                            .from_col(Items::CreatedRunId)
                            .to_tbl(ItemRuns::Table)
                            .to_col(ItemRuns::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await
    }
}
