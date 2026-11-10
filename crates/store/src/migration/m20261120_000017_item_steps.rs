//! A task's steps. A step is its own row with its own `version`, so ticking one never bumps the item's.
//!
//! `number` is given when the step is added and never changes or repeats; `position` is the step's place in the
//! item's order and shifts when a step is added before it, so it is indexed but not unique (a shifting update would
//! trip a unique index mid-statement on Postgres). Rows are never deleted: a dropped step has state `dropped`.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, char_len, integer, string_len, text_null};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum ItemSteps {
    Table,
    Id,
    ItemId,
    Number,
    Position,
    Title,
    Context,
    State,
    CreatedAt,
    Version,
}

#[derive(DeriveIden)]
enum Items {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ItemSteps::Table)
                    .col(char_len(ItemSteps::Id, ID_LEN).primary_key())
                    .col(char_len(ItemSteps::ItemId, ID_LEN))
                    .col(integer(ItemSteps::Number))
                    .col(integer(ItemSteps::Position))
                    .col(string_len(ItemSteps::Title, 300))
                    .col(text_null(ItemSteps::Context))
                    .col(string_len(ItemSteps::State, 16))
                    .col(big_integer(ItemSteps::CreatedAt))
                    .col(integer(ItemSteps::Version))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_item_steps_item")
                            .from(ItemSteps::Table, ItemSteps::ItemId)
                            .to(Items::Table, Items::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_item_steps_number")
                    .table(ItemSteps::Table)
                    .col(ItemSteps::ItemId)
                    .col(ItemSteps::Number)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ix_item_steps_position")
                    .table(ItemSteps::Table)
                    .col(ItemSteps::ItemId)
                    .col(ItemSteps::Position)
                    .to_owned(),
            )
            .await
    }
}
