//! The rough due times a client offers in place of a date ("next week"), with the dates they stand for.

use axum::Json;
use axum::extract::State;
use domain::due::{DueChoice, list_due_choices};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ClientTimeZone, CurrentUser, TimeZoneHeader};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_choices))
}

/// A rough due time. Send its `date` as a loose due time (`firm: false`), and the item's due label comes back as this
/// `label`.
#[derive(Serialize, ToSchema)]
#[schema(as = DueChoice)]
pub(crate) struct DueChoiceDto {
    /// "next week".
    pub label: String,
    /// `YYYY-MM-DD`, in the request's timezone.
    pub date: String,
}

impl From<DueChoice> for DueChoiceDto {
    fn from(choice: DueChoice) -> Self {
        Self {
            label: choice.label.to_owned(),
            date: choice.date.to_string(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = DueChoices)]
pub(crate) struct DueChoicesDto {
    /// Nearest first. The dates move with the day, so ask again on another day.
    pub choices: Vec<DueChoiceDto>,
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
