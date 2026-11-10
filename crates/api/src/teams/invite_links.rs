//! A team's invite links: making, listing and revoking them. The local build has one team of one person and nobody
//! to invite, so it leaves these out (the `accounts` feature).

use api_types::common::UserRefDto;
use api_types::invite_links::{InviteLinkActionDto, InviteLinkDto, InviteLinkListDto, InviteLinkStateDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::Timestamp;
use domain::due::describe_date;
use domain::invite_link::InviteLinkState;
use jiff::tz::TimeZone;
use service::teams::InviteLinkView;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ClientTimeZone, TimeZoneHeader};
use crate::paths::{InviteLinkParams, InviteLinkPath, TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(super) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_invite_links, create_invite_link))
        .routes(routes!(revoke_invite_link))
}

fn to_invite_link_dto(view: InviteLinkView, as_of: Timestamp, tz: &TimeZone) -> InviteLinkDto {
    let ends_at = view.link.ends_at();
    InviteLinkDto {
        id: *view.link.id.as_uuid(),
        // Only a working link shows its secret.
        token: (view.state == InviteLinkState::Working).then(|| view.link.token.expose().to_owned()),
        state: InviteLinkStateDto::from(view.state),
        created_by: UserRefDto::from(&view.created_by),
        created_at: view.link.created_at.as_millis(),
        ends_at: ends_at.as_millis(),
        ends_label: describe_date(ends_at, as_of, tz),
        revoked_by: view.revoked_by.as_ref().map(UserRefDto::from),
        revoked_at: view.link.revocation.map(|revocation| revocation.at.as_millis()),
        version: view.link.version.get(),
        allowed_actions: view
            .allowed_actions
            .into_iter()
            .map(InviteLinkActionDto::from)
            .collect(),
    }
}

/// The team's invite links, every one made and past, newest first. Working ones carry their token, so they can be
/// copied again. Admins only. A personal team has none.
#[utoipa::path(get, path = "/teams/{team}/invite-links", tag = "teams", security(("session" = [])),
    params(TeamParams, TimeZoneHeader),
    responses(
        (status = 200, body = InviteLinkListDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
    ))]
async fn list_invite_links(
    State(state): State<AppState>,
    path: TeamPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<InviteLinkListDto>, ApiError> {
    let views = state.services.list_invite_links(path.principal, path.team_id).await?;
    let as_of = state.services.now();
    Ok(Json(InviteLinkListDto {
        links: views
            .into_iter()
            .map(|view| to_invite_link_dto(view, as_of, &tz))
            .collect(),
    }))
}

/// Makes an invite link that works for a week. Admins only. Any number of links can work at once.
#[utoipa::path(post, path = "/teams/{team}/invite-links", tag = "teams", security(("session" = [])),
    params(TeamParams, TimeZoneHeader),
    responses(
        (status = 201, body = InviteLinkDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 403, body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`personal_team`", body = Problem),
    ))]
async fn create_invite_link(
    State(state): State<AppState>,
    path: TeamPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<(StatusCode, Json<InviteLinkDto>), ApiError> {
    let view = state.services.create_invite_link(path.principal, path.team_id).await?;
    let as_of = state.services.now();
    Ok((StatusCode::CREATED, Json(to_invite_link_dto(view, as_of, &tz))))
}

/// Stops an invite link working at once. Any admin of the team can, not only the one who made it. Answers `200`
/// with the link as it is when it was revoked or had ended already.
#[utoipa::path(post, path = "/teams/{team}/invite-links/{id}/revoke", tag = "teams", security(("session" = [])),
    params(InviteLinkParams, TimeZoneHeader),
    responses(
        (status = 200, body = InviteLinkDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 403, body = Problem),
        (status = 404, description = "No such team, or the team has no such link", body = Problem),
    ))]
async fn revoke_invite_link(
    State(state): State<AppState>,
    path: InviteLinkPath,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<InviteLinkDto>, ApiError> {
    let view = state
        .services
        .revoke_invite_link(path.principal, path.team_id, path.link_id)
        .await?;
    let as_of = state.services.now();
    Ok(Json(to_invite_link_dto(view, as_of, &tz)))
}
