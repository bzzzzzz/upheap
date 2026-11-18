//! The raw ID token a provider sign-in carried (`sessions.id_token` and `oidc_sign_ins.id_token`), kept only when the
//! operator turns provider logout on, `NULL` otherwise: the session keeps it to sign out at the provider, and the
//! sign-in keeps it until the username step opens the session.
//!
//! Existing sessions and sign-ins have `NULL`, so none needs a backfill.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Sessions {
    Table,
    IdToken,
}

#[derive(DeriveIden)]
enum OidcSignIns {
    Table,
    IdToken,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    // One `ALTER TABLE` for each column: SQLite adds one column at a time.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Sessions::Table)
                    .add_column(text_null(Sessions::IdToken))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(OidcSignIns::Table)
                    .add_column(text_null(OidcSignIns::IdToken))
                    .to_owned(),
            )
            .await
    }
}
