//! Password reset links. The operator makes one for a user with a server command; it works once, for an hour, and a
//! newer link or a password change cancels it. As with sessions, only the SHA-256 of the token is stored, so a leaked
//! database can't be used to take an account over, and the link is shown once, when made. The hour is derived from
//! `created_at` in `domain` and stored as `expires_at` so a change of the lifetime doesn't change links already made.
//!
//! Using and cancelling set `used_at` and `cancelled_at`; a link is never both working and either. Rows are
//! authentication plumbing and are not deleted yet: the table grows by one row per link the operator makes.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;

#[derive(DeriveIden)]
enum PasswordResetLinks {
    Table,
    TokenHash,
    UserId,
    CreatedAt,
    ExpiresAt,
    UsedAt,
    CancelledAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PasswordResetLinks::Table)
                    .col(char_len(PasswordResetLinks::TokenHash, HASH_LEN).primary_key())
                    .col(char_len(PasswordResetLinks::UserId, ID_LEN))
                    .col(big_integer(PasswordResetLinks::CreatedAt))
                    .col(big_integer(PasswordResetLinks::ExpiresAt))
                    // Set when a password is set with the link.
                    .col(big_integer_null(PasswordResetLinks::UsedAt))
                    // Set when a newer link is made for the user, or they change their password.
                    .col(big_integer_null(PasswordResetLinks::CancelledAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_password_reset_links_user")
                            .from(PasswordResetLinks::Table, PasswordResetLinks::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_password_reset_links_user")
                    .table(PasswordResetLinks::Table)
                    .col(PasswordResetLinks::UserId)
                    .to_owned(),
            )
            .await
    }
}
