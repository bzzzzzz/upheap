use sea_orm::entity::prelude::*;

/// A heap's use of a team parameter, with the heap's weight for it. `stopped_at` is `NULL` while the heap uses it.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "heap_parameters")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub heap_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub parameter_id: String,
    pub weight: i32,
    pub used_at: i64,
    pub stopped_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
