//! The operator role and what an operator reads and does about accounts: `users.operator_since` (when the server
//! command made the person an operator), `users.deactivated_at` (when an operator turned a local account off) and
//! `users.last_sign_in_at` (when a session was last opened for the person).
//!
//! All three are `NULL` for existing rows, so nobody is an operator and nobody is turned off. `last_sign_in_at` is
//! backfilled from the newest session each user still has; a user with none stays empty until they sign in.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::big_integer_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Users {
    Table,
    OperatorSince,
    DeactivatedAt,
    LastSignInAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One statement per column: SQLite adds one column at a time.
        for column in [
            big_integer_null(Users::OperatorSince),
            big_integer_null(Users::DeactivatedAt),
            big_integer_null(Users::LastSignInAt),
        ] {
            manager
                .alter_table(Table::alter().table(Users::Table).add_column(column).to_owned())
                .await?;
        }
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE users SET last_sign_in_at = \
                 (SELECT MAX(created_at) FROM sessions WHERE sessions.user_id = users.id)",
            )
            .await?;
        Ok(())
    }
}
