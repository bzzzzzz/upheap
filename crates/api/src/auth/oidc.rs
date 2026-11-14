//! Signing in through the OpenID Connect provider the operator set (architecture.md, Authentication and
//! authorization). The start and the callback are top-level navigations of the browser, so they are `GET`s and answer
//! `303`; the state cookie ties them to the browser that started.

use api_types::auth::{OidcPendingDto, OidcProviderDto, OidcRegisterRequest, OidcRegisteredDto};
use api_types::me::UserDto;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use service::auth::SessionGrant;
use service::oidc::{OidcCallback, OidcCallbackQuery, OidcRefused, OidcStart};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::signed_in_with;
use crate::error::Problem;
use crate::extract::{ApiJson, clear_oidc_cookie, oidc_cookie, oidc_state, session_cookie};
use crate::{ApiError, AppState, CookieSettings};

pub(super) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(provider))
        .routes(routes!(start))
        .routes(routes!(callback))
        .routes(routes!(pending))
        .routes(routes!(register))
}

/// The provider people may sign in through on this server.
#[utoipa::path(get, path = "/auth/oidc", tag = "auth",
    responses(
        (status = 200, description = "The label of the provider's sign-in button", body = OidcProviderDto),
        (status = 404, description = "`not_found`: the server has no provider", body = Problem),
    ))]
async fn provider(State(state): State<AppState>) -> Result<axum::Json<OidcProviderDto>, ApiError> {
    let label = state.services.oidc_label()?;
    Ok(axum::Json(OidcProviderDto { label }))
}

#[derive(Debug, Deserialize, IntoParams)]
struct StartQuery {
    /// The app path to go to once signed in, such as `/invite/<token>`. Dropped unless it starts with a single `/`
    /// and holds no control character or space.
    redirect: Option<String>,
}

/// Starts a sign-in: sends the browser to the provider.
///
/// A link to follow, not a request to make. Sets the `upheap_oidc` cookie that the callback and the username step
/// read. When the provider can't be reached, the browser is sent to the sign-in page with `sign_in_error` set to
/// `provider_unreachable`.
#[utoipa::path(get, path = "/auth/oidc/start", tag = "auth", params(StartQuery),
    responses(
        (status = 303, description = "To the provider's authorization endpoint, or to the sign-in page when it can't be reached",
            headers(("Location" = String, description = "Where to go"))),
        (status = 404, description = "`not_found`: the server has no provider", body = Problem),
    ))]
async fn start(State(state): State<AppState>, Query(query): Query<StartQuery>) -> Result<Response, ApiError> {
    let app_url = state.services.oidc_app_url()?;
    match state.services.start_oidc_sign_in(query.redirect.as_deref()).await? {
        OidcStart::Authorize {
            authorize_url,
            state: sign_in_state,
        } => {
            let mut response = see_other(&authorize_url);
            if let Some(cookie) = oidc_cookie(&sign_in_state, &state.cookies) {
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
            Ok(response)
        }
        OidcStart::Refused(refused) => Ok(refusal(&app_url, &refused, &state.cookies)),
    }
}

#[derive(Debug, Deserialize, IntoParams)]
struct CallbackQuery {
    /// The authorization code.
    code: Option<String>,
    state: Option<String>,
    /// Set when the person or the provider refused, such as `access_denied`.
    error: Option<String>,
}

/// Where the provider sends the browser back. Registered with the provider as `{BASE_URL}/api/v1/auth/oidc/callback`.
///
/// Always answers `303`. A known person is signed in and sent to the page the sign-in started from, or `/`. A new
/// one is sent to `/login/provider` to pick a username. A refusal is sent to `/login?sign_in_error=<code>`, with
/// `redirect` when the sign-in started from a page, and `login` for `email_taken`. The codes are `email_taken`,
/// `email_unverified`, `provider_refused`, `sign_in_expired` and `provider_unreachable`.
#[utoipa::path(get, path = "/auth/oidc/callback", tag = "auth", params(CallbackQuery),
    responses(
        (status = 303, description = "Signed in (sets the session cookie), to the username step, or to the sign-in page with the reason",
            headers(("Location" = String, description = "Where to go"))),
        (status = 404, description = "`not_found`: the server has no provider", body = Problem),
    ))]
async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, ApiError> {
    let app_url = state.services.oidc_app_url()?;
    let cookies = &state.cookies;
    let cookie_state = oidc_state(&headers);
    let query = OidcCallbackQuery {
        code: query.code,
        state: query.state,
        error: query.error,
    };
    match state
        .services
        .finish_oidc_sign_in(cookie_state.as_deref(), query)
        .await?
    {
        OidcCallback::SignedIn { grant, redirect } => {
            let mut response = see_other(&format!("{app_url}{}", redirect.as_deref().unwrap_or("/")));
            set_session(&mut response, &grant, cookies)?;
            response
                .headers_mut()
                .append(header::SET_COOKIE, clear_oidc_cookie(cookies));
            Ok(response)
        }
        OidcCallback::NeedsUsername => Ok(see_other(&format!("{app_url}/login/provider"))),
        OidcCallback::Refused(refused) => Ok(refusal(&app_url, &refused, cookies)),
    }
}

/// Who the provider vouched for, for the username step.
#[utoipa::path(get, path = "/auth/oidc/pending", tag = "auth",
    responses(
        (status = 200, description = "The sign-in of this browser waits for a username", body = OidcPendingDto),
        (status = 404, description = "`not_found`: no provider, or no verified sign-in in this browser, or it ended", body = Problem),
    ))]
async fn pending(State(state): State<AppState>, headers: HeaderMap) -> Result<axum::Json<OidcPendingDto>, ApiError> {
    let sign_in_state = oidc_state(&headers).ok_or(service::ServiceError::NotFound)?;
    let pending = state.services.load_oidc_pending(&sign_in_state).await?;
    Ok(axum::Json(OidcPendingDto {
        display_name: pending.display_name.as_str().to_owned(),
        email: pending.email.as_str().to_owned(),
    }))
}

/// Finishes a first sign-in through the provider: makes the user with the username they picked, with a personal
/// team and heap, and signs in. Works while registration is closed.
///
/// Fails like registering when the username is invalid or taken, and leaves the sign-in to try again.
#[utoipa::path(post, path = "/auth/oidc/register", tag = "auth", request_body = OidcRegisterRequest,
    responses(
        (status = 201, description = "Registered and signed in; sets the session cookie", body = OidcRegisteredDto),
        (status = 404, description = "`not_found`: no provider, or no verified sign-in in this browser, or it ended", body = Problem),
        (status = 409, description = "`email_taken` (`field` `email`), or `handle_taken` (`field` `username`)", body = Problem),
        (status = 422, description = "`field_empty` or `invalid_handle` (with a `hint`) for the username", body = Problem),
    ))]
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<OidcRegisterRequest>,
) -> Result<Response, ApiError> {
    let cookies = &state.cookies;
    let sign_in_state = oidc_state(&headers).ok_or(service::ServiceError::NotFound)?;
    let registered = state
        .services
        .register_oidc_user(&sign_in_state, &body.username)
        .await?;
    let dto = OidcRegisteredDto {
        user: UserDto::from(&registered.grant.user),
        redirect: registered.redirect,
    };
    let mut response = signed_in_with(StatusCode::CREATED, &registered.grant, cookies, dto);
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_oidc_cookie(cookies));
    Ok(response)
}

fn see_other(location: &str) -> Response {
    // The locations are the configured base URL, a validated app path or a provider URL: always valid header values.
    match HeaderValue::from_str(location) {
        Ok(location) => (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response(),
        Err(_) => ApiError::Service(service::ServiceError::Unauthenticated).into_response(),
    }
}

fn set_session(response: &mut Response, grant: &SessionGrant, cookies: &CookieSettings) -> Result<(), ApiError> {
    let cookie = session_cookie(&grant.token, cookies).ok_or(service::ServiceError::Unauthenticated)?;
    response.headers_mut().append(header::SET_COOKIE, cookie);
    Ok(())
}

/// To the sign-in page, told why, with the state cookie ended.
fn refusal(app_url: &str, refused: &OidcRefused, cookies: &CookieSettings) -> Response {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("sign_in_error", refused.reason.code());
    if let Some(redirect) = &refused.redirect {
        query.append_pair("redirect", redirect);
    }
    if let Some(login) = &refused.login {
        query.append_pair("login", login);
    }
    let mut response = see_other(&format!("{app_url}/login?{}", query.finish()));
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_oidc_cookie(cookies));
    response
}
