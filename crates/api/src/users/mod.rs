//! Looking up users, for team admins adding members.

use api_types::users::{UserLookupQuery, UserMatchDto, UserMatchesDto};
use axum::Json;
use axum::extract::State;
use domain::user::UserQuery;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiQuery, CurrentUser};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(find_users))
}

/// The user whose username is `query`, users with a word of their display name starting with `query`, or, when `query` is the start of an email address
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
                username: found.user.username.to_string(),
                display_name: found.user.display_name.as_str().to_owned(),
                email: found.email_matched.then(|| found.user.email.as_str().to_owned()),
            })
            .collect(),
    }))
}
