use sea_orm::entity::prelude::*;

/// A heap's link of one of its tags to a team parameter it uses. `removed_at` is `NULL` while the link holds.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "heap_parameter_tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub heap_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub parameter_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag_id: String,
    pub added_at: i64,
    pub removed_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
