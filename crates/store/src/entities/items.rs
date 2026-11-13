use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub heap_id: String,
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub kind: String,
    pub stage: String,
    pub need: String,
    pub effort: Option<String>,
    pub impact: Option<String>,
    pub due_at: Option<i64>,
    pub due_firm: Option<bool>,
    /// UTC milliseconds; set for a reminder, and kept when it becomes a task.
    pub arrives_at: Option<i64>,
    pub created_at: i64,
    pub created_by: Option<String>,
    pub closed_at: Option<i64>,
    /// Who closed it. `None` for an open item and for one closed before this was recorded.
    pub closed_by: Option<String>,
    pub blocked_by_item_id: Option<String>,
    /// The item this one was made from; set when it is made and never changed.
    pub origin_item_id: Option<String>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
