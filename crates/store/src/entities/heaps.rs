use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "heaps")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub age_weight: i32,
    pub deadline_weight: i32,
    pub value_weight: i32,
    pub created_at: i64,
    pub archived_at: Option<i64>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
