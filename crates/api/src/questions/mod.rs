//! Asking a question or making a request on an item, and replying to one. Both answer with the whole item.

use api_types::common::ItemDto;
use api_types::questions::{AskQuestionDto, QuestionPatchDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::{IfMatchHeader, to_item_dto};
use crate::error::Problem;
use crate::extract::{ApiJson, ClientTimeZone, IfMatch, TimeZoneHeader};
use crate::paths::{ItemParams, ItemPath, QuestionParams, QuestionPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(ask_question))
        .routes(routes!(reply_to_question))
}

/// Asks a question or makes a request on an open item, and answers with the whole item. The new question is the one
/// with the highest `number`. An item has at most 100 questions, answered ones included. Asking needs no
/// `If-Match`; a question has its own version, and the item's `version` doesn't change.
#[utoipa::path(post, path = "/teams/{team}/heaps/{heap}/items/{item}/questions", tag = "items", security(("session" = [])),
    params(ItemParams, TimeZoneHeader), request_body = AskQuestionDto,
    responses(
        (status = 201, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, body = Problem),
        (status = 409, description = "`heap_archived`, `item_closed` or `too_many_questions`", body = Problem),
        (status = 422, description = "Invalid fields, `value_unknown` (a `step` of 0) or `step_not_in_item` (`field` `step`)", body = Problem),
    ))]
async fn ask_question(
    State(state): State<AppState>,
    path: ItemPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<AskQuestionDto>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let fields = body.to_new_question().map_err(ServiceError::from)?;
    let view = state
        .services
        .ask_question(path.principal, path.item_id, fields)
        .await?;
    Ok((StatusCode::CREATED, Json(to_item_dto(view, state.services.now(), &tz))))
}

/// Replies to a question of an item and answers with the whole item. `{question}` is the question's number. Send the
/// question's `version` in `If-Match`; the item's `version` doesn't change. A question takes one reply: a second is
/// `question_answered`. Replying to an open question of a closed item is `item_closed`.
#[utoipa::path(patch, path = "/teams/{team}/heaps/{heap}/items/{item}/questions/{question}", tag = "items", security(("session" = [])),
    params(QuestionParams, IfMatchHeader, TimeZoneHeader), request_body = QuestionPatchDto,
    responses(
        (status = 200, body = ItemDto),
        (status = 400, description = "`invalid_body`, such as an unknown field", body = Problem),
        (status = 404, description = "No such item or question", body = Problem),
        (status = 409, description = "`version_conflict`, `question_answered`, `item_closed` or `heap_archived`", body = Problem),
        (status = 422, description = "Invalid fields: no `reply`, `pick_invalid` (`field` `picked`: out of range, repeated, none picked, or more than one where only one may be)", body = Problem),
        (status = 428, description = "`version_required`", body = Problem),
    ))]
async fn reply_to_question(
    State(state): State<AppState>,
    path: QuestionPath,
    ClientTimeZone(tz): ClientTimeZone,
    IfMatch(expected): IfMatch,
    ApiJson(body): ApiJson<QuestionPatchDto>,
) -> Result<Json<ItemDto>, ApiError> {
    let (picked, comment) = body.to_reply().map_err(ServiceError::from)?;
    let view = state
        .services
        .reply_to_question(path.principal, path.item_id, path.number, &picked, comment, expected)
        .await?;
    Ok(Json(to_item_dto(view, state.services.now(), &tz)))
}
