use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item_questions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub item_id: String,
    pub number: i32,
    pub kind: String,
    pub text: String,
    pub details: Option<String>,
    pub choices: String,
    pub many: bool,
    pub step_number: Option<i32>,
    pub state: String,
    pub asked_by: Option<String>,
    pub asked_at: i64,
    pub reply_picked: Option<String>,
    pub reply_comment: Option<String>,
    pub replied_by: Option<String>,
    pub replied_at: Option<i64>,
    pub withdrawn_by: Option<String>,
    pub withdrawn_at: Option<i64>,
    pub run_id: Option<String>,
    pub review_link: Option<String>,
    pub review_piece: Option<String>,
    pub review_round: Option<i32>,
    pub version: i32,
}

impl ActiveModelBehavior for ActiveModel {}
