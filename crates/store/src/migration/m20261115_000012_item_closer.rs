//! Who closed each closed item (`items.closed_by`), so the closed list can say "Done · today by Sam".
//!
//! Items already closed are filled in from the audit log. Closing writes the item's `closed_at` and its
//! `item_updated` entry's `created_at` from the same instant, so the closing entry is the one at that timestamp, found
//! without reading any JSON diff. An item with no matching entry keeps `NULL` and shows no closer. The column stays
//! nullable on every backend for those items, and because SQLite can't make a column `NOT NULL` without rebuilding its
//! table. The store writes it every time an item is closed and clears it on reopening.
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

/// Fills `closed_by` from the actor of each closed item's update entry at its close time. `MIN` picks one actor
/// without `LIMIT`, which MySQL refuses in some subqueries.
const BACKFILL: &str = "UPDATE items SET closed_by = ( \
     SELECT MIN(audit_log.actor_user_id) FROM audit_log \
     WHERE audit_log.entity_type = 'item' AND audit_log.entity_id = items.id \
     AND audit_log.action = 'item_updated' AND audit_log.created_at = items.closed_at) \
     WHERE closed_at IS NOT NULL AND closed_by IS NULL";

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
        } else {
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
                .await?;
        }
        manager.get_connection().execute_unprepared(BACKFILL).await?;
        Ok(())
    }
}
