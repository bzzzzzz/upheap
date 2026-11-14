//! Signing in through an OpenID Connect provider: who has which identity there, and the sign-ins in flight.
//!
//! `user_identities` ties a user to a provider's subject. `provider` is the issuer as the operator configured it,
//! and the pair is the key, so one subject is one user. A user may have none (a local account), and a provider user
//! has no `password_credentials` row. Rows are never deleted.
//!
//! `oidc_sign_ins` holds a sign-in from its start to its end: keyed by the SHA-256 of the `state` the browser carries,
//! with the nonce and PKCE verifier the callback checks, the page to return to, and once the provider vouched for the
//! person, their subject, email and display name for the username step. Rows are authentication plumbing: deleted
//! when the sign-in ends and, past `expires_at`, a few at a time by later sign-ins. They have no foreign keys, as no
//! user exists yet.
//!
//! The subject and the hash compare exactly, so MySQL gets the binary collation, as emails and tokens do.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len, string_len, string_len_null};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const HASH_LEN: u32 = 64;
const TEXT_LEN: u32 = 255;
/// The longest `redirect` kept (`domain::oidc::REDIRECT_MAX_CHARS`).
const REDIRECT_LEN: u32 = 2048;

#[derive(DeriveIden)]
enum UserIdentities {
    Table,
    UserId,
    Provider,
    Subject,
    CreatedAt,
}

#[derive(DeriveIden)]
enum OidcSignIns {
    Table,
    StateHash,
    Nonce,
    PkceVerifier,
    Redirect,
    Subject,
    Email,
    DisplayName,
    CreatedAt,
    ExpiresAt,
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
                    .table(UserIdentities::Table)
                    .col(char_len(UserIdentities::UserId, ID_LEN))
                    .col(string_len(UserIdentities::Provider, TEXT_LEN))
                    .col(string_len(UserIdentities::Subject, TEXT_LEN))
                    .col(big_integer(UserIdentities::CreatedAt))
                    .primary_key(
                        Index::create()
                            .col(UserIdentities::Provider)
                            .col(UserIdentities::Subject),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_user_identities_user")
                            .from(UserIdentities::Table, UserIdentities::UserId)
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
                    .name("ix_user_identities_user")
                    .table(UserIdentities::Table)
                    .col(UserIdentities::UserId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(OidcSignIns::Table)
                    .col(char_len(OidcSignIns::StateHash, HASH_LEN).primary_key())
                    .col(string_len(OidcSignIns::Nonce, TEXT_LEN))
                    .col(string_len(OidcSignIns::PkceVerifier, TEXT_LEN))
                    .col(string_len_null(OidcSignIns::Redirect, REDIRECT_LEN))
                    // The next three are set at the callback, once the provider vouched for the person.
                    .col(string_len_null(OidcSignIns::Subject, TEXT_LEN))
                    .col(string_len_null(OidcSignIns::Email, TEXT_LEN))
                    .col(string_len_null(OidcSignIns::DisplayName, TEXT_LEN))
                    .col(big_integer(OidcSignIns::CreatedAt))
                    .col(big_integer(OidcSignIns::ExpiresAt))
                    .to_owned(),
            )
            .await?;
        // For removing the rows that have ended.
        manager
            .create_index(
                Index::create()
                    .name("ix_oidc_sign_ins_expires_at")
                    .table(OidcSignIns::Table)
                    .col(OidcSignIns::ExpiresAt)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            for statement in [
                "ALTER TABLE user_identities MODIFY provider varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                "ALTER TABLE user_identities MODIFY subject varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                "ALTER TABLE oidc_sign_ins MODIFY state_hash char(64) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
            ] {
                manager.get_connection().execute_unprepared(statement).await?;
            }
        }
        Ok(())
    }
}
