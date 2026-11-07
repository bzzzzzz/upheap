//! Sign-in attempts counted per email address, for rate limiting. Rows are keyed by the SHA-256 of the address.
//! That only keeps addresses out of plain sight: anyone who can read the table can test a guessed address. Rows
//! are deleted soon after their window ends, so the table only ever shows addresses tried in the last few minutes.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len, integer};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum SignInAttempts {
    Table,
    EmailHash,
    Attempts,
    WindowEndsAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SignInAttempts::Table)
                    .col(char_len(SignInAttempts::EmailHash, 64).primary_key())
                    .col(integer(SignInAttempts::Attempts))
                    .col(big_integer(SignInAttempts::WindowEndsAt))
                    .to_owned(),
            )
            .await?;
        // For removing the rows of windows that have ended.
        manager
            .create_index(
                Index::create()
                    .name("idx_sign_in_attempts_window_ends_at")
                    .table(SignInAttempts::Table)
                    .col(SignInAttempts::WindowEndsAt)
                    .to_owned(),
            )
            .await
    }
}
