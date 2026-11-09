//! The name a client gave the machine it signed in on (`oauth_token_families.device_name`), so the user can tell
//! their sign-ins apart. Nullable: families made before this have none, and a client may send none.
//!
//! Also indexes what listing a user's sign-ins reads: the families of a user, and the tokens of a family. Neither
//! foreign key has one, and both tables only grow.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEVICE_NAME_LEN: u32 = 64;

#[derive(DeriveIden)]
enum OauthTokenFamilies {
    Table,
    DeviceName,
    UserId,
}

#[derive(DeriveIden)]
enum OauthTokens {
    Table,
    FamilyId,
}

const FAMILY_INDEX: &str = "idx_oauth_tokens_family";
const USER_INDEX: &str = "idx_oauth_token_families_user";

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
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(FAMILY_INDEX)
                    .table(OauthTokens::Table)
                    .col(OauthTokens::FamilyId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name(USER_INDEX)
                    .table(OauthTokenFamilies::Table)
                    .col(OauthTokenFamilies::UserId)
                    .to_owned(),
            )
            .await
    }
}
