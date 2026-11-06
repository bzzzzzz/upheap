//! Looking up users, for team admins adding members.

use axum::Json;
use axum::extract::State;
use domain::user::UserQuery;
use serde::{Deserialize, Serialize};
use service::ServiceError;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::Problem;
use crate::extract::{ApiQuery, CurrentUser};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(find_users))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct UserLookupQuery {
    /// The start of a word of the display name, or the start of an email address up to at least `@` and one more
    /// character. Ignores case. At least 3 characters.
    pub query: String,
}

/// Only what an admin needs to pick the right person.
#[derive(Serialize, ToSchema)]
#[schema(as = UserMatch)]
pub(crate) struct UserMatchDto {
    pub id: Uuid,
    pub display_name: String,
    /// Only when the query matched the start of the email, so a lookup by name never reveals addresses.
    pub email: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = UserMatches)]
pub(crate) struct UserMatchesDto {
    /// Up to 10, by display name.
    pub users: Vec<UserMatchDto>,
}

/// Users with a word of their display name starting with `query`, or, when `query` is the start of an email address
/// with at least its local part, `@` and one character of the domain, users whose email starts with it. Only for
/// admins of some team, to add members.
#[utoipa::path(get, path = "/users", tag = "users", security(("session" = [])),
    params(UserLookupQuery),
    responses(
        (status = 200, body = UserMatchesDto),
        (status = 403, description = "The caller is no team's admin", body = Problem),
        (status = 422, description = "`field_too_short`", body = Problem),
    ))]
async fn find_users(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiQuery(params): ApiQuery<UserLookupQuery>,
) -> Result<Json<UserMatchesDto>, ApiError> {
    let query = UserQuery::parse(&params.query).map_err(ServiceError::from)?;
    let users = state.services.find_users(principal, query).await?;
    Ok(Json(UserMatchesDto {
        users: users
            .into_iter()
            .map(|found| UserMatchDto {
                id: *found.user.id.as_uuid(),
                display_name: found.user.display_name.as_str().to_owned(),
                email: found.email_matched.then(|| found.user.email.as_str().to_owned()),
            })
            .collect(),
    }))
}
