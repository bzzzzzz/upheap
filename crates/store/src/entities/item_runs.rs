use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub item_id: String,
    pub turn_id: String,
    pub user_id: String,
    pub started_at: i64,
    pub heard_at: i64,
    pub ended_at: Option<i64>,
    pub end_kind: Option<String>,
    pub from_stage: String,
    pub result: Option<String>,
    pub links: Option<String>,
    pub failure: Option<String>,
    pub cost_micro_usd: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
