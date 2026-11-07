use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "teams")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub slug: String,
    pub name: String,
    #[sea_orm(unique)]
    pub personal_user_id: Option<String>,
    pub created_at: i64,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
