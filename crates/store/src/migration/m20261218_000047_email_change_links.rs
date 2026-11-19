//! Email change links. A user asks to change their address; a link is mailed to the new one, works once, for a day,
//! and a newer ask cancels it. As with password reset links, only the SHA-256 of the token is stored, so a leaked
//! database can't be used to change an address. The day is derived from `created_at` in `domain` and stored as
//! `expires_at` so a change of the lifetime doesn't change links already made.
//!
//! The table is separate from `password_reset_links` because a link carries the address it changes to, and has its
//! own lifetime. Using and cancelling set `used_at` and `cancelled_at`. Rows are authentication plumbing and are not
//! deleted yet: the table grows by one row per ask.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, string_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;
/// As `users.email`: an address is at most 254 characters.
const EMAIL_LEN: u32 = 254;

#[derive(DeriveIden)]
enum EmailChangeLinks {
    Table,
    TokenHash,
    UserId,
    NewEmail,
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
                    .table(EmailChangeLinks::Table)
                    .col(char_len(EmailChangeLinks::TokenHash, HASH_LEN).primary_key())
                    .col(char_len(EmailChangeLinks::UserId, ID_LEN))
                    .col(string_len(EmailChangeLinks::NewEmail, EMAIL_LEN))
                    .col(big_integer(EmailChangeLinks::CreatedAt))
                    .col(big_integer(EmailChangeLinks::ExpiresAt))
                    // Set when the address is changed with the link.
                    .col(big_integer_null(EmailChangeLinks::UsedAt))
                    // Set when a newer link is asked for by the user.
                    .col(big_integer_null(EmailChangeLinks::CancelledAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_email_change_links_user")
                            .from(EmailChangeLinks::Table, EmailChangeLinks::UserId)
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
                    .name("ix_email_change_links_user")
                    .table(EmailChangeLinks::Table)
                    .col(EmailChangeLinks::UserId)
                    .to_owned(),
            )
            .await
    }
}
