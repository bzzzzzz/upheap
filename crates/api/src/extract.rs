//! Request extractors and middleware: session cookie, CSRF header, timezone, `If-Match`, and body/query/path
//! wrappers whose rejections are problem responses.

use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use domain::Version;
use jiff::tz::TimeZone;
use service::auth::SessionToken;
use service::{Principal, ServiceError};

use crate::{ApiError, AppState, CookieSettings};

pub(crate) const SESSION_COOKIE: &str = "heap_session";
pub(crate) const CSRF_HEADER: &str = "x-csrf-protection";
pub(crate) const TIME_ZONE_HEADER: &str = "time-zone";

/// JSON body; a malformed body is a `400` problem.
#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub(crate) struct ApiJson<T>(pub T);

/// Query string; a malformed query is a `400` problem.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub(crate) struct ApiQuery<T>(pub T);

/// Path parameters; a malformed ID is a `404` problem.
#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub(crate) struct ApiPath<T>(pub T);

/// The signed-in caller, from the session cookie. Records `user_id` on the request span.
pub(crate) struct CurrentUser(pub Principal);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = session_token(&parts.headers).ok_or(ServiceError::Unauthenticated)?;
        let principal = state.services.authenticate_session(&token).await?;
        tracing::Span::current().record("user_id", tracing::field::display(principal.user_id));
        Ok(Self(principal))
    }
}

/// The session token from the `Cookie` header, if there is one.
pub(crate) fn session_token(headers: &HeaderMap) -> Option<SessionToken> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| SessionToken::from_client(value.to_owned()))
}

/// A `Set-Cookie` value that stores `token` as the session cookie.
pub(crate) fn session_cookie(token: &SessionToken, settings: &CookieSettings) -> Option<HeaderValue> {
    let secure = if settings.secure { "; Secure" } else { "" };
    let max_age = settings.max_age.as_secs();
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}",
        token.expose()
    ))
    .ok()
}

/// A `Set-Cookie` value that removes the session cookie.
pub(crate) fn clear_session_cookie(settings: &CookieSettings) -> HeaderValue {
    let secure = if settings.secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("heap_session=; Path=/; Max-Age=0"))
}

/// The client's IANA timezone from the `Time-Zone` header. UTC when the header is missing.
pub(crate) struct ClientTimeZone(pub TimeZone);

impl<S: Send + Sync> FromRequestParts<S> for ClientTimeZone {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let Some(value) = parts.headers.get(TIME_ZONE_HEADER) else {
            return Ok(Self(TimeZone::UTC));
        };
        let invalid = || ApiError::BadRequest {
            code: "invalid_time_zone",
            detail: "Time-Zone must be an IANA timezone name such as Europe/Berlin".to_owned(),
        };
        let name = value.to_str().map_err(|_| invalid())?;
        TimeZone::get(name).map(Self).map_err(|_| invalid())
    }
}

/// The expected version from `If-Match`, as `"3"` or `3`. Missing is a `428` problem.
pub(crate) struct IfMatch(pub Version);

impl<S: Send + Sync> FromRequestParts<S> for IfMatch {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let value = parts.headers.get(header::IF_MATCH).ok_or(ApiError::VersionRequired)?;
        let raw = value
            .to_str()
            .unwrap_or_default()
            .trim()
            .trim_start_matches("W/")
            .trim_matches('"');
        raw.parse::<u32>()
            .ok()
            .filter(|v| *v >= 1)
            .map(|v| Self(Version::new(v)))
            .ok_or(ApiError::BadRequest {
                code: "invalid_if_match",
                detail: "If-Match must hold the version being edited, for example \"3\"".to_owned(),
            })
    }
}

/// Refuses state-changing requests that lack the CSRF header.
///
/// Browsers only let same-origin scripts set custom headers (cross-origin ones need a CORS preflight, which
/// this server never approves), so the header proves the request came from the app. Together with
/// `SameSite=Lax` cookies this blocks cross-site request forgery, including on sign-in.
pub(crate) async fn require_csrf_header(request: Request, next: Next) -> Response {
    let safe = matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && !request.headers().contains_key(CSRF_HEADER) {
        return ApiError::CsrfHeaderMissing.into_response();
    }
    next.run(request).await
}
