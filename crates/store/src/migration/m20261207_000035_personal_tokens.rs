//! Personal tokens: bearer tokens a person makes in the web app for a machine with no browser or keychain. As with
//! sessions and OAuth tokens, only the SHA-256 of the secret is stored.
//!
//! `expires_at` is empty for a token that never expires. Revoked and expired rows are kept, so a request that
//! presents one can be told why it was refused.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, string_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;
const NAME_LEN: u32 = 64;

#[derive(DeriveIden)]
enum PersonalTokens {
    Table,
    Id,
    UserId,
    Name,
    TokenHash,
    CreatedAt,
    ExpiresAt,
    LastUsedAt,
    RevokedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

const HASH_INDEX: &str = "idx_personal_tokens_hash";
const USER_INDEX: &str = "idx_personal_tokens_user";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PersonalTokens::Table)
                    .col(char_len(PersonalTokens::Id, ID_LEN).primary_key())
                    .col(char_len(PersonalTokens::UserId, ID_LEN))
                    .col(string_len(PersonalTokens::Name, NAME_LEN))
                    .col(char_len(PersonalTokens::TokenHash, HASH_LEN))
                    .col(big_integer(PersonalTokens::CreatedAt))
                    .col(big_integer_null(PersonalTokens::ExpiresAt))
                    .col(big_integer_null(PersonalTokens::LastUsedAt))
                    .col(big_integer_null(PersonalTokens::RevokedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_personal_tokens_user")
                            .from(PersonalTokens::Table, PersonalTokens::UserId)
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
                    .name(HASH_INDEX)
                    .table(PersonalTokens::Table)
                    .col(PersonalTokens::TokenHash)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(USER_INDEX)
                    .table(PersonalTokens::Table)
                    .col(PersonalTokens::UserId)
                    .to_owned(),
            )
            .await
    }
}
