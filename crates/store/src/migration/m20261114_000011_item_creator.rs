//! Who added each item (`items.created_by`), so an item can say "Added by Sam 4 days ago".
//!
//! Existing items are filled in from their `item_created` audit entry, written in the same transaction as every
//! item since the first migration. The column stays nullable on every backend: an item whose entry can't be found
//! keeps `NULL` rather than failing the migration, and SQLite can't make a column `NOT NULL` without rebuilding its
//! table. The store writes it for every new item.
//!
//! As with `blocked_by_item_id`, SQLite gets the foreign key on the added column itself, since it can't add a
//! constraint to an existing table, and MySQL and Postgres get a named constraint.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Items {
    Table,
    CreatedBy,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

/// Fills `created_by` from the actor of each item's creation entry. `MIN` picks one actor without `LIMIT`, which
/// MySQL refuses in some subqueries; an item has only one creation entry.
const BACKFILL: &str = "UPDATE items SET created_by = ( \
     SELECT MIN(audit_log.actor_user_id) FROM audit_log \
     WHERE audit_log.entity_type = 'item' AND audit_log.entity_id = items.id \
     AND audit_log.action = 'item_created') \
     WHERE created_by IS NULL";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE items ADD COLUMN created_by char(36) NULL \
                     REFERENCES users (id) ON DELETE RESTRICT ON UPDATE RESTRICT",
                )
                .await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(Items::Table)
                        .add_column(ColumnDef::new(Items::CreatedBy).char_len(36).null())
                        .add_foreign_key(
                            TableForeignKey::new()
                                .name("fk_items_created_by")
                                .from_tbl(Items::Table)
                                .from_col(Items::CreatedBy)
                                .to_tbl(Users::Table)
                                .to_col(Users::Id)
                                .on_delete(ForeignKeyAction::Restrict)
                                .on_update(ForeignKeyAction::Restrict),
                        )
                        .to_owned(),
                )
                .await?;
        }
        manager.get_connection().execute_unprepared(BACKFILL).await?;
        Ok(())
    }
}
