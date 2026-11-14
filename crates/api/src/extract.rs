//! Request extractors and middleware: session cookie, bearer token, CSRF header, timezone, `If-Match`, and
//! body/query/path wrappers whose rejections are problem responses.

use axum::extract::MatchedPath;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::HeaderValue;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use domain::RunId;
use domain::Version;
use domain::run::{RunError, RunForbidden};
use jiff::tz::TimeZone;
use service::Principal;
use service::ServiceError;
use service::auth::SessionToken;
use service::oauth::OAuthSecret;
use utoipa::IntoParams;

use crate::CookieSettings;
use crate::{ApiError, AppState};

pub(crate) const SESSION_COOKIE: &str = "heap_session";
pub(crate) const CSRF_HEADER: &str = "x-csrf-protection";
pub(crate) const TIME_ZONE_HEADER: &str = "time-zone";
/// The run an agent writes in, set by the CLI when `UPHEAP_RUN` is.
pub(crate) const RUN_HEADER: &str = "upheap-run";

/// What a request made in a run (with the `Upheap-Run` header) may do besides read: the one place that says which
/// writes an agent has (design: Agent runs, Queue and Runs). A request is allowed when its method and route template
/// are listed here, and the service then checks the run and the item, and for the item's `PATCH`, which fields. Every
/// other write is refused with `run_forbidden`: replying to a question, sending and taking back, retrying, claiming
/// another run, heaps, teams, tags, parameters and views belong to the person. A run adds a task or a note, with a
/// context, to any heap the person can edit; one it makes from its own item (a split) may start blocked by another
/// it made. It closes its own item as became once something was made from it.
///
/// Templates are those the routes are registered with, under `/api/v1`.
const RUN_ALLOWED_WRITES: &[(Method, &str)] = &[
    (Method::POST, "/teams/{team}/heaps/{heap}/items"),
    (Method::POST, "/teams/{team}/heaps/{heap}/items/{item}/became"),
    (Method::PATCH, "/teams/{team}/heaps/{heap}/items/{item}"),
    (Method::POST, "/teams/{team}/heaps/{heap}/items/{item}/steps"),
    (Method::PATCH, "/teams/{team}/heaps/{heap}/items/{item}/steps/{step}"),
    (Method::POST, "/teams/{team}/heaps/{heap}/items/{item}/questions"),
    (Method::POST, "/teams/{team}/heaps/{heap}/items/{item}/comments"),
    (Method::POST, "/work/runs/{run}/heartbeat"),
    (Method::POST, "/work/runs/{run}/reached"),
    (Method::POST, "/work/runs/{run}/hand-back"),
    (Method::POST, "/work/runs/{run}/finish"),
];

/// `principal` made in the run `Upheap-Run` names, when the request has the header. A header that isn't a run ID is
/// `400`, on a read as on a write. A read in a run is just a read; a write must be one a run may make
/// ([`RUN_ALLOWED_WRITES`]).
fn in_run(parts: &Parts, principal: Principal) -> Result<Principal, ApiError> {
    let Some(value) = parts.headers.get(RUN_HEADER) else {
        return Ok(principal);
    };
    let run = value
        .to_str()
        .ok()
        .and_then(|raw| uuid::Uuid::parse_str(raw.trim()).ok())
        .map(RunId::from_uuid)
        .ok_or_else(|| ApiError::BadRequest {
            code: "invalid_header",
            detail: "Upheap-Run must be a run ID".to_owned(),
        })?;
    if matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(principal);
    }
    let route = parts
        .extensions
        .get::<MatchedPath>()
        .map(|matched| matched.as_str())
        .map(|path| path.strip_prefix("/api/v1").unwrap_or(path));
    let allowed = route.is_some_and(|route| {
        RUN_ALLOWED_WRITES
            .iter()
            .any(|(method, template)| *method == parts.method && *template == route)
    });
    if !allowed {
        return Err(ApiError::Service(RunError::Forbidden(RunForbidden::Person).into()));
    }
    Ok(Principal {
        run: Some(run),
        ..principal
    })
}

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

/// The caller, from a bearer token or the session cookie. Records `user_id` on the request span.
///
/// A request whose `Authorization` header uses the `Bearer` scheme is authenticated by it alone, and any cookie is
/// ignored, even when the token is malformed or empty. That is what lets such requests skip the CSRF check
/// ([`require_csrf_header`]): a forged cross-site request can carry the victim's cookie but can't set this header, so
/// it either has no bearer header and needs the CSRF one, or has one and gets nothing from the cookie. Other schemes,
/// such as the `Basic` credentials a browser sends to a proxy with `auth_basic`, are ignored: the browser adds those
/// to cross-site requests too, so they prove nothing, and the cookie and the CSRF check apply as usual.
pub(crate) struct CurrentUser(pub Principal);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let principal = if has_bearer_scheme(&parts.headers) {
            let token = bearer_token(&parts.headers).ok_or(ApiError::BearerRejected)?;
            state
                .services
                .authenticate_access_token(&token)
                .await
                .map_err(|err| match err {
                    ServiceError::Unauthenticated => ApiError::BearerRejected,
                    other => ApiError::Service(other),
                })?
        } else {
            session_principal(parts, state).await?
        };
        tracing::Span::current().record("user_id", tracing::field::display(principal.user_id));
        let principal = in_run(parts, principal)?;
        Ok(Self(principal))
    }
}

/// The caller from the session cookie alone, for what only the user in their browser may do: approving a client's
/// access. A request with a bearer token is refused, so a client holding a token can't approve more access for
/// itself, and so the request can't have skipped the CSRF check.
pub(crate) struct SessionUser(pub Principal);

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if has_bearer_scheme(&parts.headers) {
            return Err(ServiceError::Unauthenticated.into());
        }
        let principal = session_principal(parts, state).await?;
        tracing::Span::current().record("user_id", tracing::field::display(principal.user_id));
        Ok(Self(principal))
    }
}

/// The caller if there is one, for what anyone holding a link may do and a signed-in caller gets more from: opening
/// an invite link. Records `user_id` on the request span when there is a caller.
///
/// A bearer token is authenticated exactly as for [`CurrentUser`], and a rejected one is `401`: a client that sends
/// one expects it to count. Otherwise a session cookie is used, and one that is unknown or expired counts as signed
/// out, not as `401`, because the page behind the link works signed out too and the person can't act on the error.
pub(crate) struct MaybeUser(pub Option<Principal>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let principal = if has_bearer_scheme(&parts.headers) {
            let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
            Some(principal)
        } else {
            match session_principal(parts, state).await {
                Ok(principal) => Some(principal),
                Err(ApiError::Service(ServiceError::Unauthenticated)) => None,
                Err(other) => return Err(other),
            }
        };
        Ok(Self(principal))
    }
}

async fn session_principal(parts: &Parts, state: &AppState) -> Result<Principal, ApiError> {
    let token = session_token(&parts.headers).ok_or(ServiceError::Unauthenticated)?;
    Ok(state.services.authenticate_session(&token).await?)
}

/// Whether the `Authorization` header uses the `Bearer` scheme (RFC 6750 §2.1), compared ignoring case, whatever
/// follows it. The one test of "is this a bearer request", for authentication and the CSRF check alike.
fn has_bearer_scheme(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(' ').next())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
}

/// The token from an `Authorization: Bearer <token>` header, or nothing when the header doesn't hold one: another
/// scheme, or `Bearer` with no token or a malformed one.
fn bearer_token(headers: &HeaderMap) -> Option<OAuthSecret> {
    if !has_bearer_scheme(headers) {
        return None;
    }
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (_, token) = value.split_once(' ')?;
    let token = token.trim();
    let valid = !token.is_empty() && !token.contains(' ');
    valid.then(|| OAuthSecret::from_client(token.to_owned()))
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

/// The `Time-Zone` request header, documented once.
#[derive(IntoParams)]
#[into_params(parameter_in = Header)]
#[allow(dead_code)] // Only read by utoipa to document the header; `ClientTimeZone` reads the value.
pub(crate) struct TimeZoneHeader {
    /// The device's IANA timezone, for example `Europe/Berlin`. Labels and dates use it. Defaults to UTC.
    #[param(rename = "Time-Zone")]
    time_zone: Option<String>,
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
///
/// Requests with a bearer token don't need it: they are authenticated by the token and never by a cookie
/// ([`CurrentUser`]), and a cross-site page can't set that header either. CSRF is a cookie problem. Other
/// `Authorization` schemes don't exempt a request, as a browser may add them to cross-site requests itself.
pub(crate) async fn require_csrf_header(request: Request, next: Next) -> Response {
    let safe = matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let exempt = safe || has_bearer_scheme(request.headers());
    if !exempt && !request.headers().contains_key(CSRF_HEADER) {
        return ApiError::CsrfHeaderMissing.into_response();
    }
    next.run(request).await
}

pub(crate) const OIDC_COOKIE: &str = "upheap_oidc";
/// The cookie of a sign-in through the provider reaches only the endpoints that read it.
const OIDC_COOKIE_PATH: &str = "/api/v1/auth/oidc";

/// The state of the sign-in through the provider this browser started, from the `Cookie` header, if there is one.
pub(crate) fn oidc_state(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == OIDC_COOKIE)
        .map(|(_, value)| value.to_owned())
}

/// A `Set-Cookie` value that keeps `state` for the 15 minutes a sign-in through the provider lasts. `Secure` when
/// the session cookie is.
pub(crate) fn oidc_cookie(state: &str, settings: &CookieSettings) -> Option<HeaderValue> {
    let secure = if settings.secure { "; Secure" } else { "" };
    let max_age = domain::oidc::PENDING_SIGN_IN_LIFETIME.as_seconds_ceil();
    HeaderValue::from_str(&format!(
        "{OIDC_COOKIE}={state}; Path={OIDC_COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .ok()
}

/// A `Set-Cookie` value that removes the sign-in cookie.
pub(crate) fn clear_oidc_cookie(settings: &CookieSettings) -> HeaderValue {
    let secure = if settings.secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{OIDC_COOKIE}=; Path={OIDC_COOKIE_PATH}; HttpOnly; SameSite=Lax; Max-Age=0{secure}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("upheap_oidc=; Path=/api/v1/auth/oidc; Max-Age=0"))
}
