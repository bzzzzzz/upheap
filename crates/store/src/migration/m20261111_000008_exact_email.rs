//! Email addresses compare exactly on MySQL too. Addresses are stored lowercased, and SQLite compares bytes, but
//! MySQL's default collation also ignores accents and treats some invisible characters as equal, so `adá@…` or
//! `ada` followed by a zero-width space found Ada's account there. That let each such spelling get its own
//! sign-in attempts, and let two accounts differ only in an accent on SQLite but not on MySQL.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::MySql {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE users MODIFY email varchar(254) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                )
                .await?;
        }
        Ok(())
    }
}
