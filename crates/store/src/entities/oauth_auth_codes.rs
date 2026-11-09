use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_auth_codes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub code_hash: String,
    pub user_id: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
    pub family_id: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
