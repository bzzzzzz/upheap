//! What the identity provider pushes over SCIM: its users, its groups and who is in them.
//!
//! The rows mirror the provider's data and are not the person's, so a `DELETE` from the provider deletes them, and
//! the members of a group or the memberships of a user first. `user_name` and `display_name` are unique and, with
//! `external_id`, compare exactly, so MySQL gets the binary collation, as subjects and handles do. `external_id` is
//! indexed for the lookups and isn't unique. `body` is the resource as the provider sent it, less the attributes the
//! columns hold: a JSON object.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, boolean, char_len, string_len, string_len_null, text};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const TEXT_LEN: u32 = 255;

#[derive(DeriveIden)]
enum ScimUsers {
    Table,
    Id,
    UserName,
    ExternalId,
    Active,
    Body,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ScimGroups {
    Table,
    Id,
    DisplayName,
    ExternalId,
    Body,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ScimGroupMembers {
    Table,
    GroupId,
    UserId,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ScimUsers::Table)
                    .col(char_len(ScimUsers::Id, ID_LEN).primary_key())
                    .col(string_len(ScimUsers::UserName, TEXT_LEN).unique_key())
                    .col(string_len_null(ScimUsers::ExternalId, TEXT_LEN))
                    .col(boolean(ScimUsers::Active))
                    .col(text(ScimUsers::Body))
                    .col(big_integer(ScimUsers::CreatedAt))
                    .col(big_integer(ScimUsers::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_scim_users_external_id")
                    .table(ScimUsers::Table)
                    .col(ScimUsers::ExternalId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(ScimGroups::Table)
                    .col(char_len(ScimGroups::Id, ID_LEN).primary_key())
                    .col(string_len(ScimGroups::DisplayName, TEXT_LEN).unique_key())
                    .col(string_len_null(ScimGroups::ExternalId, TEXT_LEN))
                    .col(text(ScimGroups::Body))
                    .col(big_integer(ScimGroups::CreatedAt))
                    .col(big_integer(ScimGroups::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_scim_groups_external_id")
                    .table(ScimGroups::Table)
                    .col(ScimGroups::ExternalId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(ScimGroupMembers::Table)
                    .col(char_len(ScimGroupMembers::GroupId, ID_LEN))
                    .col(char_len(ScimGroupMembers::UserId, ID_LEN))
                    .primary_key(
                        Index::create()
                            .col(ScimGroupMembers::GroupId)
                            .col(ScimGroupMembers::UserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_scim_group_members_group")
                            .from(ScimGroupMembers::Table, ScimGroupMembers::GroupId)
                            .to(ScimGroups::Table, ScimGroups::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_scim_group_members_user")
                            .from(ScimGroupMembers::Table, ScimGroupMembers::UserId)
                            .to(ScimUsers::Table, ScimUsers::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        // For a user's groups.
        manager
            .create_index(
                Index::create()
                    .name("ix_scim_group_members_user")
                    .table(ScimGroupMembers::Table)
                    .col(ScimGroupMembers::UserId)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::MySql {
            for statement in [
                "ALTER TABLE scim_users MODIFY user_name varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                "ALTER TABLE scim_users MODIFY external_id varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL",
                "ALTER TABLE scim_users MODIFY body MEDIUMTEXT NOT NULL",
                "ALTER TABLE scim_groups MODIFY display_name varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL",
                "ALTER TABLE scim_groups MODIFY external_id varchar(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL",
                "ALTER TABLE scim_groups MODIFY body MEDIUMTEXT NOT NULL",
            ] {
                manager.get_connection().execute_unprepared(statement).await?;
            }
        }
        Ok(())
    }
}
