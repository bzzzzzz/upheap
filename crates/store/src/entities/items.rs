use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub heap_id: String,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub effort: Option<String>,
    pub impact: Option<String>,
    pub due_at: Option<i64>,
    pub due_firm: Option<bool>,
    pub created_at: i64,
    pub closed_at: Option<i64>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
