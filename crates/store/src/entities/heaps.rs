use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "heaps")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub team_id: String,
    pub slug: String,
    pub name: String,
    pub need_weight: i32,
    pub age_weight: i32,
    pub deadline_weight: i32,
    pub value_weight: i32,
    pub tags_weight: i32,
    /// A JSON list of tag IDs, the highest first; `NULL` when no tag is ranked.
    pub tag_order: Option<String>,
    pub context: Option<String>,
    pub agents_prepare: bool,
    /// The note that became this heap.
    pub origin_id: Option<String>,
    pub created_at: i64,
    pub archived_at: Option<i64>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
