//! Why a token family or personal token was revoked: `oauth_token_families.revoke_reason` and
//! `personal_tokens.revoke_reason`, set with `revoked_at`. Today the only value is `password_changed`.
//!
//! `NULL` means no reason was recorded: every row from before this migration, and every revocation that isn't a
//! password change. So no backfill is needed.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::string_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OauthTokenFamilies {
    Table,
    RevokeReason,
}

#[derive(DeriveIden)]
enum PersonalTokens {
    Table,
    RevokeReason,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // One `ALTER TABLE` for each column: SQLite adds one column at a time.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(OauthTokenFamilies::Table)
                    .add_column(string_null(OauthTokenFamilies::RevokeReason))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(PersonalTokens::Table)
                    .add_column(string_null(PersonalTokens::RevokeReason))
                    .to_owned(),
            )
            .await
    }
}
