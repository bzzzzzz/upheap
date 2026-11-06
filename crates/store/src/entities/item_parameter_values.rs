use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "item_parameter_values")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub item_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub parameter_id: String,
    /// `None` once the value was cleared.
    pub option_id: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
