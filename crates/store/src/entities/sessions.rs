use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub token_hash: String,
    pub user_id: String,
    pub created_at: i64,
    pub expires_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
