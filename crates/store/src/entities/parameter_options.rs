use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "parameter_options")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub parameter_id: String,
    pub label: String,
    pub position: i32,
    pub archived_at: Option<i64>,
    pub replaced_by_option_id: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
