//! The name a client gave the machine it signed in on (`oauth_token_families.device_name`), so the user can tell
//! their sign-ins apart. Nullable: families made before this have none, and a client may send none.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEVICE_NAME_LEN: u32 = 64;

#[derive(DeriveIden)]
enum OauthTokenFamilies {
    Table,
    DeviceName,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(OauthTokenFamilies::Table)
                    .add_column(
                        ColumnDef::new(OauthTokenFamilies::DeviceName)
                            .string_len(DEVICE_NAME_LEN)
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }
}
