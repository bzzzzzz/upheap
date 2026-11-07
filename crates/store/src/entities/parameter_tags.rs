use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "parameter_tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub parameter_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag_id: String,
    pub added_at: i64,
    pub removed_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
