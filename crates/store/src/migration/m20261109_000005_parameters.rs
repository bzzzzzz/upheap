//! Custom parameters, their options, the tags they are suggested for, and each item's value for them.
//!
//! Nothing here is deleted:
//! - Parameters and options are archived (`archived_at`). An archived option names the option that replaced it,
//!   so `archived_at` and `replaced_by_option_id` are set together. `position` orders the options in use,
//!   highest priority first; an archived option keeps the position it last had, which means nothing.
//! - Unlinking a tag sets `parameter_tags.removed_at`, and linking it again clears it, as with `item_tags`.
//! - Clearing an item's value sets `item_parameter_values.option_id` to `NULL` rather than deleting the row.
//!
//! Parameter names are unique among a heap's parameters in use, which a unique index can't express while archived
//! parameters keep their names. Every parameter write holds the heap lock, so the service checks it instead.

use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, char_len, char_len_null, integer, string_len};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ID_LEN: u32 = 36;

#[derive(DeriveIden)]
enum Parameters {
    Table,
    Id,
    HeapId,
    Name,
    Weight,
    CreatedAt,
    ArchivedAt,
    Version,
}

#[derive(DeriveIden)]
enum ParameterOptions {
    Table,
    Id,
    ParameterId,
    Label,
    Position,
    ArchivedAt,
    ReplacedByOptionId,
}

#[derive(DeriveIden)]
enum ParameterTags {
    Table,
    ParameterId,
    TagId,
    AddedAt,
    RemovedAt,
}

#[derive(DeriveIden)]
enum ItemParameterValues {
    Table,
    ItemId,
    ParameterId,
    OptionId,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Items {
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

fn index(name: &str, table: impl IntoIden, column: impl IntoIden) -> IndexCreateStatement {
    Index::create().name(name).table(table).col(column).to_owned()
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Parameters::Table)
                    .col(char_len(Parameters::Id, ID_LEN).primary_key())
                    .col(char_len(Parameters::HeapId, ID_LEN))
                    .col(string_len(Parameters::Name, 50))
                    .col(integer(Parameters::Weight))
                    .col(big_integer(Parameters::CreatedAt))
                    .col(big_integer_null(Parameters::ArchivedAt))
                    .col(integer(Parameters::Version))
                    .foreign_key(&mut restrict(
                        "fk_parameters_heap",
                        (Parameters::Table, Parameters::HeapId),
                        (Heaps::Table, Heaps::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(index("idx_parameters_heap", Parameters::Table, Parameters::HeapId))
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ParameterOptions::Table)
                    .col(char_len(ParameterOptions::Id, ID_LEN).primary_key())
                    .col(char_len(ParameterOptions::ParameterId, ID_LEN))
                    .col(string_len(ParameterOptions::Label, 50))
                    .col(integer(ParameterOptions::Position))
                    .col(big_integer_null(ParameterOptions::ArchivedAt))
                    .col(char_len_null(ParameterOptions::ReplacedByOptionId, ID_LEN))
                    .foreign_key(&mut restrict(
                        "fk_parameter_options_parameter",
                        (ParameterOptions::Table, ParameterOptions::ParameterId),
                        (Parameters::Table, Parameters::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_parameter_options_replaced_by",
                        (ParameterOptions::Table, ParameterOptions::ReplacedByOptionId),
                        (ParameterOptions::Table, ParameterOptions::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(index(
                "idx_parameter_options_parameter",
                ParameterOptions::Table,
                ParameterOptions::ParameterId,
            ))
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ParameterTags::Table)
                    .col(char_len(ParameterTags::ParameterId, ID_LEN))
                    .col(char_len(ParameterTags::TagId, ID_LEN))
                    .col(big_integer(ParameterTags::AddedAt))
                    .col(big_integer_null(ParameterTags::RemovedAt))
                    .primary_key(
                        Index::create()
                            .col(ParameterTags::ParameterId)
                            .col(ParameterTags::TagId),
                    )
                    .foreign_key(&mut restrict(
                        "fk_parameter_tags_parameter",
                        (ParameterTags::Table, ParameterTags::ParameterId),
                        (Parameters::Table, Parameters::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_parameter_tags_tag",
                        (ParameterTags::Table, ParameterTags::TagId),
                        (Tags::Table, Tags::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(index(
                "idx_parameter_tags_tag",
                ParameterTags::Table,
                ParameterTags::TagId,
            ))
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ItemParameterValues::Table)
                    .col(char_len(ItemParameterValues::ItemId, ID_LEN))
                    .col(char_len(ItemParameterValues::ParameterId, ID_LEN))
                    .col(char_len_null(ItemParameterValues::OptionId, ID_LEN))
                    .primary_key(
                        Index::create()
                            .col(ItemParameterValues::ItemId)
                            .col(ItemParameterValues::ParameterId),
                    )
                    .foreign_key(&mut restrict(
                        "fk_item_parameter_values_item",
                        (ItemParameterValues::Table, ItemParameterValues::ItemId),
                        (Items::Table, Items::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_item_parameter_values_parameter",
                        (ItemParameterValues::Table, ItemParameterValues::ParameterId),
                        (Parameters::Table, Parameters::Id),
                    ))
                    .foreign_key(&mut restrict(
                        "fk_item_parameter_values_option",
                        (ItemParameterValues::Table, ItemParameterValues::OptionId),
                        (ParameterOptions::Table, ParameterOptions::Id),
                    ))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(index(
                "idx_item_parameter_values_parameter",
                ItemParameterValues::Table,
                ItemParameterValues::ParameterId,
            ))
            .await?;
        manager
            .create_index(index(
                "idx_item_parameter_values_option",
                ItemParameterValues::Table,
                ItemParameterValues::OptionId,
            ))
            .await
    }
}
