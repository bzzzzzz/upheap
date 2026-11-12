//! Adding a comment to an item. It answers with the whole item.

use api_types::comments::AddCommentDto;
use api_types::common::ItemDto;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::to_item_dto;
use crate::error::Problem;
use crate::extract::{ApiJson, ClientTimeZone, TimeZoneHeader};
use crate::paths::{ItemParams, ItemPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(add_comment))
}

/// Adds a comment to an item, open or closed, and answers with the whole item. The new comment is the one with the
/// highest `number`. Comments are never changed or deleted, and an item has at most 200. Adding one needs no
/// `If-Match`; the item's `version` and `updated_at` don't change.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/comments", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader), request_body = AddCommentDto,
    responses(
        (status = 201, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived` or `too_many_comments`", body = Problem),
        (status = 422, description = "A blank or too long `body` (`field` `body`)", body = Problem),
    ))]
async fn add_comment(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<AddCommentDto>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let body = body.to_body().map_err(ServiceError::from)?;
    let view = state.services.add_comment(path.principal, path.item_id, body).await?;
    Ok((StatusCode::CREATED, Json(to_item_dto(view, state.services.now(), &tz))))
}
