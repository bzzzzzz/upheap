use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_token_families")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub user_id: String,
    pub client_id: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub device_name: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
