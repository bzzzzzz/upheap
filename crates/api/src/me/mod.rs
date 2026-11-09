//! The signed-in user, their teams and heaps.

#[cfg(feature = "accounts")]
pub(crate) mod sign_ins;

use api_types::common::HeapSummaryDto;
use api_types::me::{HeapOverviewDto, MeActionDto, MeDto, MePatchDto, TeamDto, UserDto};
use axum::Json;
use axum::extract::State;
use domain::user::DisplayName;
use service::ServiceError;
use service::me::{HeapOverview, MeView, TeamView};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::IfMatchHeader;
use crate::error::Problem;
use crate::extract::{ApiJson, CurrentUser, IfMatch};
use crate::{ApiError, AppState, Authentication};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_me, update_me))
}

/// What `GET /me` answers for `me`. The local build's one user can't sign out or make teams, so gets no actions.
fn to_me_dto(me: MeView, authentication: &Authentication) -> MeDto {
    let allowed_actions = if authentication.is_single_user() {
        Vec::new()
    } else {
        vec![
            MeActionDto::SignOut,
            MeActionDto::CreateTeam,
            MeActionDto::ManageSignIns,
        ]
    };
    let team = |view: TeamView| TeamDto {
        id: *view.team.id.as_uuid(),
        slug: view.team.slug.to_string(),
        name: view.team.name.as_str().to_owned(),
        personal: view.team.is_personal(),
        role: view.role.into(),
        member_count: view.member_count,
        heaps: view.heaps.iter().map(to_heap_overview_dto).collect(),
    };
    MeDto {
        user: UserDto::from(&me.user),
        teams: me.teams.into_iter().map(team).collect(),
        allowed_actions,
    }
}

fn to_heap_overview_dto(overview: &HeapOverview) -> HeapOverviewDto {
    HeapOverviewDto {
        heap: HeapSummaryDto::from(&overview.heap),
        open_count: overview.open_count,
    }
}

/// The signed-in user with their teams and heaps.
#[utoipa::path(get, path = "/me", tag = "me", security(("session" = [])),
    responses((status = 200, body = MeDto), (status = 401, description = "`unauthenticated`", body = Problem)))]
async fn get_me(State(state): State<AppState>, CurrentUser(principal): CurrentUser) -> Result<Json<MeDto>, ApiError> {
    Ok(Json(to_me_dto(
        state.services.load_me(principal).await?,
        &state.authentication,
    )))
}

/// Changes the signed-in user's display name. Send the user's `version` in `If-Match`.
#[utoipa::path(patch, path = "/me", tag = "me", security(("session" = [])),
    params(IfMatchHeader), request_body = MePatchDto,
    responses(
        (status = 200, body = UserDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 409, description = "`version_conflict`", body = Problem),
        (status = 422, body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn update_me(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<MePatchDto>,
) -> Result<Json<UserDto>, ApiError> {
    let display_name = DisplayName::parse(&body.display_name).map_err(ServiceError::from)?;
    let user = state
        .services
        .update_display_name(principal, display_name, expected)
        .await?;
    Ok(Json(UserDto::from(&user)))
}
