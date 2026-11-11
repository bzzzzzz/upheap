use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item_turns")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub item_id: String,
    pub reach: String,
    pub goal: String,
    pub line: Option<String>,
    pub opened_by: Option<String>,
    pub source: String,
    pub opened_at: i64,
    pub ended_at: Option<i64>,
    pub ended_by: Option<String>,
    pub end_kind: Option<String>,
    pub failed_runs: i32,
}

impl ActiveModelBehavior for ActiveModel {}
