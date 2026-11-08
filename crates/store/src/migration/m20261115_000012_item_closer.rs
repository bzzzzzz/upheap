//! Who closed each closed item (`items.closed_by`), so the closed list can say "Done · today by Sam".
//!
//! Items closed before this migration keep `NULL` and show no closer: filling it in from the audit log would need
//! reading each entry's JSON diff, which has no portable SQL. The column stays nullable on every backend for those
//! items, and because SQLite can't make a column `NOT NULL` without rebuilding its table. The store writes it every
//! time an item is closed and clears it on reopening.
//!
//! As with `created_by`, SQLite gets the foreign key on the added column itself, since it can't add a constraint to
//! an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    ClosedBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE items ADD COLUMN closed_by char(36) NULL \
                     REFERENCES users (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Items::Table)
                    .add_column(ColumnDef::new(Items::ClosedBy).char_len(36).null())
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_items_closed_by")
                            .from_tbl(Items::Table)
                            .from_col(Items::ClosedBy)
                            .to_tbl(Users::Table)
                            .to_col(Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await
    }
}
