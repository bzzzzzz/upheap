use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "scim_group_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
}

impl ActiveModelBehavior for ActiveModel {}
