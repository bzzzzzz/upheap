//! A team's invite links. A link is made by one of the team's admins and works for a week unless an admin revokes
//! it first; the week is derived from `created_at` in `domain`, not stored. Revoking sets `revoked_at` and
//! `revoked_by` together, and rows are never deleted.
//!
//! The token is stored readable, unlike session tokens and OAuth secrets, which are stored hashed: admins copy a
//! link again for as long as it works, so the server has to be able to show it (architecture.md, Data model).
//! A token is 43 characters of base64url. The unique index refuses a collision.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, char_len_null, integer, string_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;
const TOKEN_LEN: u32 = 43;

#[derive(DeriveIden)]
enum InviteLinks {
    Table,
    Id,
    TeamId,
    Token,
    CreatedBy,
    CreatedAt,
    RevokedAt,
    RevokedBy,
    Version,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

fn restrict(name: &str, column: InviteLinks, to: (impl IntoIden, impl IntoIden)) -> ForeignKeyCreateStatement {
    ForeignKey::create()
        .name(name)
        .from(InviteLinks::Table, column)
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
                    .table(InviteLinks::Table)
                    .col(char_len(InviteLinks::Id, ID_LEN).primary_key())
                    .col(char_len(InviteLinks::TeamId, ID_LEN))
                    .col(string_len(InviteLinks::Token, TOKEN_LEN))
                    .col(char_len(InviteLinks::CreatedBy, ID_LEN))
                    .col(big_integer(InviteLinks::CreatedAt))
                    // Set together, when an admin revokes the link.
                    .col(big_integer_null(InviteLinks::RevokedAt))
                    .col(char_len_null(InviteLinks::RevokedBy, ID_LEN))
                    .col(integer(InviteLinks::Version))
                    .foreign_key(&mut restrict(
                        "fk_invite_links_team",
                        InviteLinks::TeamId,
                        (Teams::Table, Teams::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_invite_links_created_by",
                        InviteLinks::CreatedBy,
                        (Users::Table, Users::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_invite_links_revoked_by",
                        InviteLinks::RevokedBy,
                        (Users::Table, Users::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_invite_links_token")
                    .table(InviteLinks::Table)
                    .col(InviteLinks::Token)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_invite_links_team")
                    .table(InviteLinks::Table)
                    .col(InviteLinks::TeamId)
                    .to_owned(),
            )
            .await
    }
}
