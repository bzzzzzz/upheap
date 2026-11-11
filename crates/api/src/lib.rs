//! The HTTP API of Upheap, served under `/api/v1`, plus the health probes.
//!
//! The request and response types are in `api_types`, which clients share. Handlers translate HTTP to use-case
//! calls and back; they never check permissions or compute anything about items themselves. Errors become
//! `application/problem+json` in one place, [`error::ApiError`].
//!
//! Features, both on by default:
//! - `accounts`: registration, sign-in and session cookies, OAuth and bearer tokens for the CLI, team management and
//!   user lookup. Without it, every request is the local build's one user ([`Authentication::LocalUser`]).
//! - `trace-and-timeout`: the request span and the 30-second timeout. They need `Instant::now()` and tokio's timer,
//!   which the browser lacks, so the local build keeps only the request IDs.

mod audit;
#[cfg(feature = "accounts")]
mod auth;
mod dto;
mod due_choices;
mod error;
mod extract;
mod health;
mod heaps;
#[cfg(feature = "accounts")]
mod invites;
mod items;
mod me;
#[cfg(feature = "accounts")]
mod oauth;
mod parameters;
mod paths;
mod questions;
mod request_id;
mod steps;
mod tags;
mod teams;
#[cfg(feature = "agents")]
mod turns;
#[cfg(feature = "accounts")]
mod users;
mod views;

#[cfg(feature = "accounts")]
use std::time::Duration;

use api_types::list_view::{BlockedLayoutDto, SortDto, StateModeDto};
use axum::Router;
#[cfg(feature = "trace-and-timeout")]
use axum::extract::{MatchedPath, Request};
use axum::middleware;
use service::{Principal, Services};
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
#[cfg(feature = "trace-and-timeout")]
use tower_http::timeout::TimeoutLayer;
#[cfg(feature = "trace-and-timeout")]
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa_axum::router::OpenApiRouter;

pub use error::ApiError;

/// Everything handlers need. Cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub services: Services,
    pub authentication: Authentication,
}

/// How a request's caller is found.
#[derive(Clone, Debug)]
pub enum Authentication {
    /// From the session cookie, which signing in sets as these settings say, or from an OAuth access token in an
    /// `Authorization: Bearer` header.
    #[cfg(feature = "accounts")]
    Sessions(CookieSettings),
    /// Always this user, without asking: the local build's one user, from [`Services::open_local_user`]. There is
    /// no signing in, so the routes for it answer `404`.
    LocalUser(Principal),
}

impl Authentication {
    /// The local build's one user: no one else to add, one team, and no signing in or out. `allowed_actions` leave
    /// out what that rules out.
    pub(crate) const fn is_single_user(&self) -> bool {
        matches!(self, Self::LocalUser(_))
    }
}

/// How the session cookie is issued.
#[cfg(feature = "accounts")]
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
    // Only used in query parameters, which don't register their schemas.
    components(schemas(StateModeDto, BlockedLayoutDto, SortDto)),
    tags(
        (name = "auth", description = "Registration and sign-in"),
        (name = "oauth", description = "Signing in non-web clients, such as the CLI: OAuth 2.0 with PKCE"),
        (name = "me", description = "The signed-in user"),
        (name = "items", description = "Items and the ranked list"),
        (name = "tags", description = "A heap's tags"),
        (name = "teams", description = "Teams and their members"),
        (name = "heaps", description = "Creating, renaming and archiving heaps"),
        (name = "users", description = "Finding users to add to a team"),
        (name = "views", description = "A heap's saved views"),
        (name = "parameters", description = "A heap's custom parameters and their options"),
        (name = "audit", description = "The history of a heap or a team"),
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

/// Routes that take `SessionUser`, the cookie only: the bearer scheme is not added to them. Beside the OAuth prefix,
/// which is skipped the same way.
#[cfg(feature = "accounts")]
const SESSION_ONLY_PATHS: &[&str] = &["/api/v1/invites/{token}/join"];

/// Declares the bearer scheme, and offers it beside the session cookie wherever an operation takes the cookie,
/// since [`extract::CurrentUser`] takes either. The OAuth operations are left as they say: approving an
/// authorization request takes only the cookie, and so do the paths in [`SESSION_ONLY_PATHS`]. Run on the finished
/// document, as the paths come from the routers.
#[cfg(feature = "accounts")]
fn add_bearer_scheme(openapi: &mut utoipa::openapi::OpenApi) {
    use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement};
    let components = openapi.components.get_or_insert_with(Default::default);
    components.add_security_scheme(
        "bearer",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
    );
    let session = SecurityRequirement::new("session", Vec::<String>::new());
    let bearer = SecurityRequirement::new("bearer", Vec::<String>::new());
    for (path, item) in &mut openapi.paths.paths {
        if path.starts_with("/api/v1/oauth/") || SESSION_ONLY_PATHS.contains(&path.as_str()) {
            continue;
        }
        let operations = [
            &mut item.get,
            &mut item.put,
            &mut item.post,
            &mut item.delete,
            &mut item.patch,
        ];
        for operation in operations.into_iter().flatten() {
            if let Some(security) = &mut operation.security
                && security.contains(&session)
            {
                security.push(bearer.clone());
            }
        }
    }
}

fn api_router() -> OpenApiRouter<AppState> {
    let v1 = OpenApiRouter::new();
    #[cfg(feature = "accounts")]
    let v1 = v1
        .merge(auth::routes())
        .merge(oauth::authorize_routes())
        .merge(users::routes())
        .merge(invites::routes())
        .merge(me::sign_ins::routes());
    // Turns are for a build that has agents.
    #[cfg(feature = "agents")]
    let v1 = v1.merge(turns::routes());
    let v1 = v1
        .merge(me::routes())
        .merge(due_choices::routes())
        .merge(items::routes())
        .merge(questions::routes())
        .merge(steps::routes())
        .merge(tags::routes())
        .merge(teams::routes())
        .merge(heaps::routes())
        .merge(views::routes())
        .merge(parameters::routes())
        .merge(audit::routes())
        .fallback(error::not_found)
        .layer(middleware::from_fn(extract::require_csrf_header));
    // After the CSRF layer: a client's token requests carry no cookie, so a forged one would have nothing to borrow,
    // and OAuth clients don't send the header.
    #[cfg(feature = "accounts")]
    let v1 = v1.merge(oauth::token_routes());
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest("/api/v1", v1)
}

/// The OpenAPI document for the API. The checked-in `openapi.json` is generated from this.
pub fn openapi() -> utoipa::openapi::OpenApi {
    split_api_router().1
}

fn split_api_router() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    #[cfg_attr(not(feature = "accounts"), allow(unused_mut))]
    let (api, mut spec) = api_router().split_for_parts();
    #[cfg(feature = "accounts")]
    add_bearer_scheme(&mut spec);
    (api, spec)
}

/// The API and health routes with request IDs applied, and tracing and timeouts with `trace-and-timeout`.
pub fn router(state: AppState) -> Router {
    let (api, spec) = split_api_router();
    let spec_json = serde_json::to_string(&spec).unwrap_or_default();
    #[cfg(not(feature = "trace-and-timeout"))]
    let observed = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(middleware::from_fn(request_id::scope_request_id));
    #[cfg(feature = "trace-and-timeout")]
    let observed = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TraceLayer::new_for_http().make_span_with(|request: &Request| {
            let route = request.extensions().get::<MatchedPath>().map_or(request.uri().path(), MatchedPath::as_str);
            let request_id = request.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or_default();
            tracing::info_span!("request", method = %request.method(), route, request_id, user_id = tracing::field::Empty)
        }))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(middleware::from_fn(request_id::scope_request_id))
        .layer(TimeoutLayer::with_status_code(axum::http::StatusCode::REQUEST_TIMEOUT, std::time::Duration::from_secs(30)));

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
