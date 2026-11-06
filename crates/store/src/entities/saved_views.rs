use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "saved_views")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub heap_id: String,
    pub name: String,
    /// JSON text; see `crate::saved_views`.
    pub list_view: String,
    pub created_by: String,
    pub created_at: i64,
    pub archived_at: Option<i64>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
