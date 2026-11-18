//! The provider groups a first sign-in carried (`oidc_sign_ins.groups`): a JSON array of strings kept until the
//! username step, `NULL` when the groups were unknown.
//!
//! Sign-ins already in flight have `NULL`, so none needs a backfill.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum OidcSignIns {
    Table,
    Groups,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(OidcSignIns::Table)
                    .add_column(text_null(OidcSignIns::Groups))
                    .to_owned(),
            )
            .await
    }
}
