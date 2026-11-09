use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub token_hash: String,
    pub family_id: String,
    pub kind: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
