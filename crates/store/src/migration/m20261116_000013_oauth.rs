//! Authorization codes and tokens for non-web clients (OAuth 2.0 with PKCE). As with sessions, only the SHA-256 of
//! each code and token is stored, so a leaked database can't be used to sign in.
//!
//! A token family is one sign-in by a client: the access and refresh tokens a code was exchanged for and every pair
//! refreshed from them. It has a row of its own, and revoking is setting its `revoked_at`: every token of the family
//! is checked against it. One row also gives refreshing and revoking a single row to lock, so a revocation that
//! races a refresh can't miss the pair the refresh is issuing. A code records the family it was exchanged for, so a
//! code presented twice revokes what the first exchange issued while the code is within its lifetime
//! (RFC 6749 §4.1.2).
//!
//! `client_id` names a client of the registry in code (`domain::oauth`), so it has no foreign key.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, char_len_null, string_len, text};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;

#[derive(DeriveIden)]
enum OauthTokenFamilies {
    Table,
    Id,
    UserId,
    ClientId,
    CreatedAt,
    RevokedAt,
}

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
    CreatedAt,
    ExpiresAt,
    UsedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

fn restrict(
    name: &str,
    from: (impl IntoIden, impl IntoIden),
    to: (impl IntoIden, impl IntoIden),
) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(from.0, from.1)
        .to(to.0, to.1)
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
                    .table(OauthTokenFamilies::Table)
                    .col(char_len(OauthTokenFamilies::Id, ID_LEN).primary_key())
                    .col(char_len(OauthTokenFamilies::UserId, ID_LEN))
                    .col(string_len(OauthTokenFamilies::ClientId, 64))
                    .col(big_integer(OauthTokenFamilies::CreatedAt))
                    .col(big_integer_null(OauthTokenFamilies::RevokedAt))
                    .foreign_key(&mut restrict(
                        "fk_oauth_token_families_user",
                        (OauthTokenFamilies::Table, OauthTokenFamilies::UserId),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;

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
                    .foreign_key(&mut restrict(
                        "fk_oauth_auth_codes_user",
                        (OauthAuthCodes::Table, OauthAuthCodes::UserId),
                        (Users::Table, Users::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_oauth_auth_codes_family",
                        (OauthAuthCodes::Table, OauthAuthCodes::FamilyId),
                        (OauthTokenFamilies::Table, OauthTokenFamilies::Id),
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
                    .col(big_integer(OauthTokens::CreatedAt))
                    .col(big_integer(OauthTokens::ExpiresAt))
                    // Refresh tokens only: set when it was exchanged for the next pair.
                    .col(big_integer_null(OauthTokens::UsedAt))
                    // The user and client are the family's.
                    .foreign_key(&mut restrict(
                        "fk_oauth_tokens_family",
                        (OauthTokens::Table, OauthTokens::FamilyId),
                        (OauthTokenFamilies::Table, OauthTokenFamilies::Id),
                    ))
                    .to_owned(),
            )
            .await
    }
}
