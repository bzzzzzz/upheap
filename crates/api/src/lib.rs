//! The HTTP API of Upheap, served under `/api/v1`, plus the health probes.
//!
//! Handlers translate HTTP to use-case calls and back; they never check permissions or compute anything about
//! items themselves. Errors become `application/problem+json` in one place, [`error::ApiError`].

mod auth;
mod dto;
mod error;
mod extract;
mod health;
mod items;
mod me;
mod request_id;

use std::time::Duration;

use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::middleware;
use service::Services;
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa_axum::router::OpenApiRouter;

pub use error::ApiError;

/// Everything handlers need. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub services: Services,
    pub cookies: CookieSettings,
}

/// How the session cookie is issued.
#[derive(Clone, Debug)]
pub struct CookieSettings {
    /// Adds the `Secure` attribute. On unless the server is reached over plain HTTP.
    pub secure: bool,
    /// Lifetime of the cookie; matches the session lifetime.
    pub max_age: Duration,
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Upheap API", description = "A todo list that ranks itself."),
    modifiers(&SessionCookieScheme),
    tags(
        (name = "auth", description = "Registration and sign-in"),
        (name = "me", description = "The signed-in user"),
        (name = "items", description = "Items and the ranked list"),
    )
)]
struct ApiDoc;

struct SessionCookieScheme;

impl utoipa::Modify for SessionCookieScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(extract::SESSION_COOKIE))),
        );
    }
}

fn api_router() -> OpenApiRouter<AppState> {
    let v1 = OpenApiRouter::new()
        .merge(auth::routes())
        .merge(me::routes())
        .merge(items::routes())
        .fallback(error::not_found)
        .layer(middleware::from_fn(extract::require_csrf_header));
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest("/api/v1", v1)
}

/// The OpenAPI document for the API. The checked-in `openapi.json` is generated from this.
pub fn openapi() -> utoipa::openapi::OpenApi {
    api_router().split_for_parts().1
}

/// The API and health routes with request IDs, tracing and timeouts applied.
pub fn router(state: AppState) -> Router {
    let (api, spec) = api_router().split_for_parts();
    let spec_json = serde_json::to_string(&spec).unwrap_or_default();
    let observed = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TraceLayer::new_for_http().make_span_with(|request: &Request| {
            let route = request.extensions().get::<MatchedPath>().map_or(request.uri().path(), MatchedPath::as_str);
            let request_id = request.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or_default();
            tracing::info_span!("request", method = %request.method(), route, request_id, user_id = tracing::field::Empty)
        }))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(middleware::from_fn(request_id::scope_request_id))
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)));

    Router::new()
        .merge(api)
        .route(
            "/api/v1/openapi.json",
            axum::routing::get(
                move || async move { ([(axum::http::header::CONTENT_TYPE, "application/json")], spec_json) },
            ),
        )
        .merge(health::routes())
        .with_state(state)
        .layer(observed)
}
