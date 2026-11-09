//! The rough due times a client offers in place of a date ("next week"), with the dates they stand for.

use api_types::due_choices::{DueChoiceDto, DueChoicesDto};
use axum::Json;
use axum::extract::State;
use domain::due::list_due_choices;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ClientTimeZone, CurrentUser, TimeZoneHeader};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_choices))
}

/// The rough due times to offer in a due picker, for today in the request's timezone.
#[utoipa::path(get, path = "/due-choices", tag = "items", security(("session" = [])), params(TimeZoneHeader),
    responses((status = 200, body = DueChoicesDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn list_choices(
    State(state): State<AppState>,
    CurrentUser(_): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<DueChoicesDto>, ApiError> {
    let choices = list_due_choices(state.services.now(), &tz);
    Ok(Json(DueChoicesDto {
        choices: choices.into_iter().map(DueChoiceDto::from).collect(),
    }))
}
