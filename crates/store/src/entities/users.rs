use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub email: String,
    pub display_name: String,
    pub created_at: i64,
    pub version: i32,
    pub unconfirmed_since: Option<i64>,
    pub operator_since: Option<i64>,
    pub deactivated_at: Option<i64>,
    pub last_sign_in_at: Option<i64>,
    pub default_heap_id: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
