//! SCIM 2.0 provisioning endpoints, served at `/scim/v2` when the operator set a token.
//!
//! The RFCs, not Upheap, are this contract: providers are the only clients, so the endpoints are outside `/api/v1`
//! and `openapi.json`, speak `application/scim+json` and refuse in SCIM's error format ([`error::ScimFailure`]).
//! The bearer token is the operator's setting, not a user's, and the CSRF layer of `/api/v1` doesn't apply.

mod error;

use axum::Json;
use axum::Router;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use serde_json::{Value, json};
use service::scim::{ListQuery, ScimError};

use crate::AppState;
use error::ScimFailure;

const CONTENT_TYPE: &str = "application/scim+json";

/// The routes, behind the bearer token check. A path nobody serves stays unknown to every caller.
pub(crate) fn routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/scim/v2/ServiceProviderConfig", get(service_provider_config))
        .route("/scim/v2/Users", get(list_users).post(create_user))
        .route(
            "/scim/v2/Users/{id}",
            get(get_user).put(replace_user).patch(patch_user).delete(delete_user),
        )
        .route("/scim/v2/Groups", get(list_groups).post(create_group))
        .route(
            "/scim/v2/Groups/{id}",
            get(get_group)
                .put(replace_group)
                .patch(patch_group)
                .delete(delete_group),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate))
}

/// Refuses a request that doesn't carry the operator's token as `Authorization: Bearer <token>`.
async fn authenticate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim());
    match state.services.scim_authenticate(token) {
        Ok(()) => next.run(request).await,
        Err(error) => ScimFailure(error).into_response(),
    }
}

type Reply = Result<Response, ScimFailure>;

fn scim_json(status: StatusCode, body: &Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE));
    response
}

/// `201` with the new resource and its `Location`.
fn created(body: &Value) -> Response {
    let mut response = scim_json(StatusCode::CREATED, body);
    if let Some(location) = body["meta"]["location"]
        .as_str()
        .and_then(|l| HeaderValue::from_str(l).ok())
    {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
}

fn no_content() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

fn read_body(body: Result<Json<Value>, JsonRejection>) -> Result<Value, ScimFailure> {
    body.map(|Json(body)| body)
        .map_err(|_| ScimError::InvalidSyntax("the body isn't valid JSON").into())
}

#[derive(Deserialize)]
struct ListParams {
    filter: Option<String>,
    #[serde(rename = "startIndex")]
    start_index: Option<i64>,
    count: Option<i64>,
}

fn list_query(params: Result<Query<ListParams>, QueryRejection>) -> Result<ListParams, ScimFailure> {
    params
        .map(|Query(params)| params)
        .map_err(|_| ScimError::InvalidSyntax("startIndex and count must be whole numbers").into())
}

impl ListParams {
    fn as_query(&self) -> ListQuery<'_> {
        ListQuery {
            filter: self.filter.as_deref(),
            start_index: self.start_index,
            count: self.count,
        }
    }
}

/// What this server supports, for the providers that read it.
async fn service_provider_config() -> Response {
    scim_json(
        StatusCode::OK,
        &json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
            "patch": { "supported": true },
            "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
            "filter": { "supported": true, "maxResults": service::scim::MAX_PAGE },
            "changePassword": { "supported": false },
            "sort": { "supported": false },
            "etag": { "supported": false },
            "authenticationSchemes": [{
                "type": "oauthbearertoken",
                "name": "Bearer token",
                "description": "The token the operator set as UPHEAP_SCIM_TOKEN",
                "primary": true,
            }],
        }),
    )
}

async fn list_users(State(state): State<AppState>, params: Result<Query<ListParams>, QueryRejection>) -> Reply {
    let params = list_query(params)?;
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_list_users(params.as_query()).await?,
    ))
}

async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    Ok(scim_json(StatusCode::OK, &state.services.scim_get_user(&id).await?))
}

async fn create_user(State(state): State<AppState>, body: Result<Json<Value>, JsonRejection>) -> Reply {
    Ok(created(&state.services.scim_create_user(&read_body(body)?).await?))
}

async fn replace_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Reply {
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_replace_user(&id, &read_body(body)?).await?,
    ))
}

async fn patch_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Reply {
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_patch_user(&id, &read_body(body)?).await?,
    ))
}

async fn delete_user(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.services.scim_delete_user(&id).await?;
    Ok(no_content())
}

async fn list_groups(State(state): State<AppState>, params: Result<Query<ListParams>, QueryRejection>) -> Reply {
    let params = list_query(params)?;
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_list_groups(params.as_query()).await?,
    ))
}

async fn get_group(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    Ok(scim_json(StatusCode::OK, &state.services.scim_get_group(&id).await?))
}

async fn create_group(State(state): State<AppState>, body: Result<Json<Value>, JsonRejection>) -> Reply {
    Ok(created(&state.services.scim_create_group(&read_body(body)?).await?))
}

async fn replace_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Reply {
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_replace_group(&id, &read_body(body)?).await?,
    ))
}

async fn patch_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Reply {
    Ok(scim_json(
        StatusCode::OK,
        &state.services.scim_patch_group(&id, &read_body(body)?).await?,
    ))
}

async fn delete_group(State(state): State<AppState>, Path(id): Path<String>) -> Reply {
    state.services.scim_delete_group(&id).await?;
    Ok(no_content())
}
