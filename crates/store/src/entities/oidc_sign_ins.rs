use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "oidc_sign_ins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub state_hash: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub redirect: Option<String>,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
