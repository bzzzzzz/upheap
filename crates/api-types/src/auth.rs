//! Registration and sign-in for local accounts.

use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Becomes the slug of
    /// the personal team, and can't be changed later.
    #[schema(example = "eugene")]
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct SignInRequest {
    /// An email address, or a username. `email` is still accepted in its place.
    #[serde(alias = "email")]
    pub login: String,
    pub password: String,
}
