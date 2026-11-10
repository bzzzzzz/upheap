use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "invite_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub team_id: String,
    #[sea_orm(unique)]
    pub token: String,
    pub created_by: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
