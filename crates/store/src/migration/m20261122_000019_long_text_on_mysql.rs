//! The long texts fit on MySQL. Its `TEXT` holds 65,535 bytes, but an item's context (`items.description`), a step's
//! and a heap's may each be 20,000 characters, up to 80,000 bytes in UTF-8, and an audit entry's diff keeps the text
//! before and after an edit. `MEDIUMTEXT` holds 16 MB. SQLite and Postgres have no such limit on `text`.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Each column with its nullability, which `MODIFY` must state again.
const COLUMNS: [&str; 4] = [
    "ALTER TABLE items MODIFY description MEDIUMTEXT NULL",
    "ALTER TABLE item_steps MODIFY context MEDIUMTEXT NULL",
    "ALTER TABLE heaps MODIFY context MEDIUMTEXT NULL",
    "ALTER TABLE audit_log MODIFY diff MEDIUMTEXT NOT NULL",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::MySql {
            for statement in COLUMNS {
                manager.get_connection().execute_unprepared(statement).await?;
            }
        }
        Ok(())
    }
}
