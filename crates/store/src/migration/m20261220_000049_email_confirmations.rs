//! Confirming the address of a new account: `users.unconfirmed_since`, and the links that confirm it.
//!
//! `unconfirmed_since` is when the account was made while the server had mail set up, and `NULL` means the address is
//! confirmed. So every existing account counts as confirmed with no backfill, and so do accounts made while mail is
//! off and accounts made through a provider or SCIM, which never set it.
//!
//! `email_confirmation_links` is separate from `email_change_links` and `password_reset_links`: one table per
//! purpose, as those two have, since each has its own lifetime and its own meaning for using a link. Using and
//! cancelling set `used_at` and `cancelled_at`. As for the others, only a hash of the token is stored. Rows are
//! authentication plumbing and are not deleted yet: the table grows by one row per mail.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;

#[derive(DeriveIden)]
enum EmailConfirmationLinks {
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
    UnconfirmedSince,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(big_integer_null(Users::UnconfirmedSince))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(EmailConfirmationLinks::Table)
                    .col(char_len(EmailConfirmationLinks::TokenHash, HASH_LEN).primary_key())
                    .col(char_len(EmailConfirmationLinks::UserId, ID_LEN))
                    .col(big_integer(EmailConfirmationLinks::CreatedAt))
                    .col(big_integer(EmailConfirmationLinks::ExpiresAt))
                    // Set when the address is confirmed with the link.
                    .col(big_integer_null(EmailConfirmationLinks::UsedAt))
                    // Set when a newer link is mailed to the user.
                    .col(big_integer_null(EmailConfirmationLinks::CancelledAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_email_confirmation_links_user")
                            .from(EmailConfirmationLinks::Table, EmailConfirmationLinks::UserId)
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
                    .name("ix_email_confirmation_links_user")
                    .table(EmailConfirmationLinks::Table)
                    .col(EmailConfirmationLinks::UserId)
                    .to_owned(),
            )
            .await
    }
}
