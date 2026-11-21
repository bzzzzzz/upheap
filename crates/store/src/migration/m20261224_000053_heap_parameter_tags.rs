//! A heap's links of its tags to a team parameter it uses: `heap_parameter_tags`, one row per heap, team parameter
//! and tag, with the same `added_at` and `removed_at` as `parameter_tags`. Unlinking sets `removed_at`, and linking
//! again clears it.
//!
//! It is a table of its own because these links belong to the heap, not to the parameter: `parameter_tags` rows have
//! no heap, and a heap may link its team's tags, and a heap tag that becomes a team tag, so the link stays the heap's
//! while the tag changes owner. A team parameter's own links, to the team's tags, stay in `parameter_tags`.
//!
//! The links carry no version of their own: a heap's write about them takes the parameter only shared.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum HeapParameterTags {
    Table,
    HeapId,
    ParameterId,
    TagId,
    AddedAt,
    RemovedAt,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Parameters {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
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
                    .table(HeapParameterTags::Table)
                    .col(char_len(HeapParameterTags::HeapId, ID_LEN))
                    .col(char_len(HeapParameterTags::ParameterId, ID_LEN))
                    .col(char_len(HeapParameterTags::TagId, ID_LEN))
                    .col(big_integer(HeapParameterTags::AddedAt))
                    .col(big_integer_null(HeapParameterTags::RemovedAt))
                    .primary_key(
                        Index::create()
                            .col(HeapParameterTags::HeapId)
                            .col(HeapParameterTags::ParameterId)
                            .col(HeapParameterTags::TagId),
                    )
                    .foreign_key(&mut restrict(
                        "fk_heap_parameter_tags_heap",
                        (HeapParameterTags::Table, HeapParameterTags::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_heap_parameter_tags_parameter",
                        (HeapParameterTags::Table, HeapParameterTags::ParameterId),
                        (Parameters::Table, Parameters::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_heap_parameter_tags_tag",
                        (HeapParameterTags::Table, HeapParameterTags::TagId),
                        (Tags::Table, Tags::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_heap_parameter_tags_tag")
                    .table(HeapParameterTags::Table)
                    .col(HeapParameterTags::TagId)
                    .to_owned(),
            )
            .await
    }
}
