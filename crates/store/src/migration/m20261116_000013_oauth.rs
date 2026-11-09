//! Authorization codes and tokens for non-web clients (OAuth 2.0 with PKCE). As with sessions, only the SHA-256 of
//! each code and token is stored, so a leaked database can't be used to sign in.
//!
//! Every token belongs to a family: the access and refresh tokens of one sign-in and all the pairs refreshed from it.
//! Revoking a token, or presenting a refresh token that was already used, revokes the whole family, which
//! `idx_oauth_tokens_family` finds. A code records the family it was exchanged for, so a code presented twice revokes
//! what the first exchange issued while the code is within its lifetime (RFC 6749 §4.1.2).
//!
//! `client_id` names a client of the registry in code (`domain::oauth`), so it has no foreign key.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, char_len_null, string_len, text};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;

#[derive(DeriveIden)]
enum OauthAuthCodes {
    Table,
    CodeHash,
    UserId,
    ClientId,
    RedirectUri,
    CodeChallenge,
    CreatedAt,
    ExpiresAt,
    UsedAt,
    FamilyId,
}

#[derive(DeriveIden)]
enum OauthTokens {
    Table,
    TokenHash,
    FamilyId,
    Kind,
    UserId,
    ClientId,
    CreatedAt,
    ExpiresAt,
    UsedAt,
    RevokedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

fn user_fk(name: &str, from: (impl IntoIden, impl IntoIden)) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(from.0, from.1)
        .to(Users::Table, Users::Id)
        .on_delete(ForeignKeyAction::Restrict)
        .on_update(ForeignKeyAction::Restrict)
        .to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OauthAuthCodes::Table)
                    .col(char_len(OauthAuthCodes::CodeHash, HASH_LEN).primary_key())
                    .col(char_len(OauthAuthCodes::UserId, ID_LEN))
                    .col(string_len(OauthAuthCodes::ClientId, 64))
                    .col(text(OauthAuthCodes::RedirectUri))
                    .col(string_len(OauthAuthCodes::CodeChallenge, 128))
                    .col(big_integer(OauthAuthCodes::CreatedAt))
                    .col(big_integer(OauthAuthCodes::ExpiresAt))
                    // Set, with the family, when the code is exchanged; a code is good once.
                    .col(big_integer_null(OauthAuthCodes::UsedAt))
                    .col(char_len_null(OauthAuthCodes::FamilyId, ID_LEN))
                    .foreign_key(&mut user_fk(
                        "fk_oauth_auth_codes_user",
                        (OauthAuthCodes::Table, OauthAuthCodes::UserId),
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(OauthTokens::Table)
                    .col(char_len(OauthTokens::TokenHash, HASH_LEN).primary_key())
                    .col(char_len(OauthTokens::FamilyId, ID_LEN))
                    // `access` or `refresh`.
                    .col(string_len(OauthTokens::Kind, 16))
                    .col(char_len(OauthTokens::UserId, ID_LEN))
                    .col(string_len(OauthTokens::ClientId, 64))
                    .col(big_integer(OauthTokens::CreatedAt))
                    .col(big_integer(OauthTokens::ExpiresAt))
                    // Refresh tokens only: set when it was exchanged for the next pair.
                    .col(big_integer_null(OauthTokens::UsedAt))
                    .col(big_integer_null(OauthTokens::RevokedAt))
                    .foreign_key(&mut user_fk(
                        "fk_oauth_tokens_user",
                        (OauthTokens::Table, OauthTokens::UserId),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_oauth_tokens_family")
                    .table(OauthTokens::Table)
                    .col(OauthTokens::FamilyId)
                    .to_owned(),
            )
            .await
    }
}
