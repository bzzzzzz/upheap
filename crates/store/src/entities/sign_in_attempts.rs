use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sign_in_attempts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub email_hash: String,
    pub attempts: i32,
    pub window_ends_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
