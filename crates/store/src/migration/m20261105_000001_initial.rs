//! Users, sessions, teams, heaps, items and the audit log.
//!
//! Portable types only: IDs are `CHAR(36)` UUIDs, instants are `BIGINT` UTC milliseconds, structured data is
//! JSON in `TEXT`. Foreign keys never cascade.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{
    big_integer, big_integer_null, boolean_null, char_len, integer, string_len, text, text_null,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Email,
    DisplayName,
    CreatedAt,
    Version,
}

#[derive(DeriveIden)]
enum PasswordCredentials {
    Table,
    UserId,
    PasswordHash,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Sessions {
    Table,
    TokenHash,
    UserId,
    CreatedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
    Name,
    CreatedAt,
    Version,
}

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    TeamId,
    UserId,
    Role,
    JoinedAt,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
    TeamId,
    Name,
    AgeWeight,
    DeadlineWeight,
    ValueWeight,
    CreatedAt,
    ArchivedAt,
    Version,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
    HeapId,
    Title,
    Description,
    State,
    Effort,
    Impact,
    DueAt,
    DueFirm,
    CreatedAt,
    ClosedAt,
    Version,
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Id,
    TeamId,
    HeapId,
    ActorUserId,
    Action,
    EntityType,
    EntityId,
    Diff,
    CreatedAt,
}

fn id(col: impl IntoIden) -> ColumnDef {
    char_len(col, ID_LEN)
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
                    .table(Users::Table)
                    .col(id(Users::Id).primary_key())
                    .col(string_len(Users::Email, 254).unique_key())
                    .col(string_len(Users::DisplayName, 100))
                    .col(big_integer(Users::CreatedAt))
                    .col(integer(Users::Version))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PasswordCredentials::Table)
                    .col(id(PasswordCredentials::UserId).primary_key())
                    .col(text(PasswordCredentials::PasswordHash))
                    .col(big_integer(PasswordCredentials::UpdatedAt))
                    .foreign_key(&mut restrict(
                        "fk_password_credentials_user",
                        (PasswordCredentials::Table, PasswordCredentials::UserId),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Sessions::Table)
                    .col(char_len(Sessions::TokenHash, 64).primary_key())
                    .col(id(Sessions::UserId))
                    .col(big_integer(Sessions::CreatedAt))
                    .col(big_integer(Sessions::ExpiresAt))
                    .foreign_key(&mut restrict(
                        "fk_sessions_user",
                        (Sessions::Table, Sessions::UserId),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Teams::Table)
                    .col(id(Teams::Id).primary_key())
                    .col(string_len(Teams::Name, 100))
                    .col(big_integer(Teams::CreatedAt))
                    .col(integer(Teams::Version))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TeamMembers::Table)
                    .col(id(TeamMembers::TeamId))
                    .col(id(TeamMembers::UserId))
                    .col(string_len(TeamMembers::Role, 16))
                    .col(big_integer(TeamMembers::JoinedAt))
                    .primary_key(Index::create().col(TeamMembers::TeamId).col(TeamMembers::UserId))
                    .foreign_key(&mut restrict(
                        "fk_team_members_team",
                        (TeamMembers::Table, TeamMembers::TeamId),
                        (Teams::Table, Teams::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_team_members_user",
                        (TeamMembers::Table, TeamMembers::UserId),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_team_members_user")
                    .table(TeamMembers::Table)
                    .col(TeamMembers::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Heaps::Table)
                    .col(id(Heaps::Id).primary_key())
                    .col(id(Heaps::TeamId))
                    .col(string_len(Heaps::Name, 100))
                    .col(integer(Heaps::AgeWeight))
                    .col(integer(Heaps::DeadlineWeight))
                    .col(integer(Heaps::ValueWeight))
                    .col(big_integer(Heaps::CreatedAt))
                    .col(big_integer_null(Heaps::ArchivedAt))
                    .col(integer(Heaps::Version))
                    .foreign_key(&mut restrict(
                        "fk_heaps_team",
                        (Heaps::Table, Heaps::TeamId),
                        (Teams::Table, Teams::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_heaps_team")
                    .table(Heaps::Table)
                    .col(Heaps::TeamId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Items::Table)
                    .col(id(Items::Id).primary_key())
                    .col(id(Items::HeapId))
                    .col(string_len(Items::Title, 300))
                    .col(text_null(Items::Description))
                    .col(string_len(Items::State, 16))
                    .col(ColumnDef::new(Items::Effort).string_len(4).null())
                    .col(ColumnDef::new(Items::Impact).string_len(4).null())
                    .col(big_integer_null(Items::DueAt))
                    .col(boolean_null(Items::DueFirm))
                    .col(big_integer(Items::CreatedAt))
                    .col(big_integer_null(Items::ClosedAt))
                    .col(integer(Items::Version))
                    .foreign_key(&mut restrict(
                        "fk_items_heap",
                        (Items::Table, Items::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_items_heap_state")
                    .table(Items::Table)
                    .col(Items::HeapId)
                    .col(Items::State)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_items_heap_closed")
                    .table(Items::Table)
                    .col(Items::HeapId)
                    .col(Items::ClosedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(AuditLog::Table)
                    .col(id(AuditLog::Id).primary_key())
                    .col(id(AuditLog::TeamId))
                    .col(ColumnDef::new(AuditLog::HeapId).char_len(ID_LEN).null())
                    .col(id(AuditLog::ActorUserId))
                    .col(string_len(AuditLog::Action, 64))
                    .col(string_len(AuditLog::EntityType, 32))
                    .col(id(AuditLog::EntityId))
                    .col(text(AuditLog::Diff))
                    .col(big_integer(AuditLog::CreatedAt))
                    .foreign_key(&mut restrict(
                        "fk_audit_log_team",
                        (AuditLog::Table, AuditLog::TeamId),
                        (Teams::Table, Teams::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_audit_log_heap",
                        (AuditLog::Table, AuditLog::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_audit_log_actor",
                        (AuditLog::Table, AuditLog::ActorUserId),
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_heap")
                    .table(AuditLog::Table)
                    .col(AuditLog::HeapId)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_log_team")
                    .table(AuditLog::Table)
                    .col(AuditLog::TeamId)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await
    }
}
