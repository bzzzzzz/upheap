use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_log")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub team_id: String,
    pub heap_id: Option<String>,
    pub actor_user_id: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub diff: String,
    pub run_id: Option<String>,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
