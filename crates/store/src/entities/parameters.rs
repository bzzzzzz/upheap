use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "parameters")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// The owning heap; `NULL` for a team parameter. Exactly one of `heap_id` and `team_id` is set.
    pub heap_id: Option<String>,
    pub team_id: Option<String>,
    pub name: String,
    /// A heap parameter's weight. A team parameter holds `0` here and it is never read: each heap's weight for it is
    /// in `heap_parameters`.
    pub weight: i32,
    pub created_at: i64,
    pub archived_at: Option<i64>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
