use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub heap_id: String,
    pub name: String,
    pub name_key: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
