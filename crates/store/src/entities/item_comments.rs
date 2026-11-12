use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item_comments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub item_id: String,
    pub number: i32,
    pub body: String,
    pub created_by: String,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
