//! Looking up users, for team admins adding members.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserLookupQuery {
    /// A username, the start of a word of the display name, or the start of an email address up to at least `@` and one more
    /// character. Ignores case. At least 3 characters.
    pub query: String,
}

/// Only what an admin needs to pick the right person.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = UserMatch)]
pub struct UserMatchDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    /// Only when the query matched the start of the email, so a lookup by name never reveals addresses.
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = UserMatches)]
pub struct UserMatchesDto {
    /// Up to 10, by display name.
    pub users: Vec<UserMatchDto>,
}
