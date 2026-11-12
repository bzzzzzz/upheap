//! OAuth 2.0 for first-party non-web clients such as the CLI: the authorization code flow with PKCE (RFC 6749,
//! RFC 7636) and token revocation (RFC 7009).
//!
//! The browser half is the web app's consent page, which checks the request with `GET /oauth/authorize` and, once the
//! user allows it, approves it with `POST /oauth/authorize` and sends the browser to the redirect URI it gets back.
//! Both are ordinary JSON endpoints with problem responses, and approving takes the session cookie and the CSRF header
//! like every other write from the app.
//!
//! The client half, `POST /oauth/token` and `POST /oauth/revoke`, follows the RFCs instead of this API's conventions,
//! so any OAuth client library can talk to it: form-encoded bodies, and refusals in the RFC 6749 §5.2 error format
//! rather than problem+json. They read no cookie, so there's nothing for a forged cross-site request to borrow, and
//! they take no CSRF header (see `api_router`). A failure of the server itself (`500`, `503`) is still a problem
//! response, as the RFC defines no error for it there.

use api_types::oauth::{
    ApprovalDto, AuthorizationRequestDto, ClientDto, OAuthErrorDto, RevokeRequestForm, TokenRequestForm,
    TokenResponseDto,
};
use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use domain::oauth::{AuthorizationError, DeviceName};
use service::ServiceError;
use service::oauth::{CodeExchange, OAuthSecret, TokenPair};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiJson, ApiQuery, SessionUser};
use crate::{ApiError, AppState};

/// The consent page's endpoints, behind the CSRF check.
pub(crate) fn authorize_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(check_authorization, approve_authorization))
}

/// The client's endpoints, which take no CSRF header.
pub(crate) fn token_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(issue_token))
        .routes(routes!(revoke_token))
}

/// Checks an authorization request without approving it.
///
/// The consent page calls this before it shows anything, so it never asks about, or sends the browser to, a request
/// the server would refuse. Once this succeeds, the client and the redirect URI are good, so a user who declines is
/// sent to the redirect URI with `error=access_denied` and the `state`, which the page adds itself.
#[utoipa::path(get, path = "/oauth/authorize", tag = "oauth", params(AuthorizationRequestDto),
    responses(
        (status = 200, description = "The request can be approved", body = ClientDto),
        (status = 422, description = "`client_unknown` (`field` `client_id`), `redirect_uri_invalid`, \
            `response_type_unsupported`, `code_challenge_invalid`, `code_challenge_method_unsupported` or \
            `state_missing`, each naming its `field`", body = Problem),
    ))]
async fn check_authorization(
    State(state): State<AppState>,
    ApiQuery(request): ApiQuery<AuthorizationRequestDto>,
) -> Result<Json<ClientDto>, ApiError> {
    let checked = state.services.check_authorization(request.as_params())?;
    Ok(Json(ClientDto {
        client_id: checked.client.id.to_owned(),
        display_name: checked.client.display_name.to_owned(),
    }))
}

/// Approves an authorization request for the signed-in user, from the consent page.
///
/// Takes the session cookie only, never a bearer token: approving is the user's decision in their browser, not
/// something a client holding a token can do for itself. The code in `redirect_to` is good once, for ten minutes.
#[utoipa::path(post, path = "/oauth/authorize", tag = "oauth", security(("session" = [])),
    request_body = AuthorizationRequestDto,
    responses(
        (status = 200, description = "Approved; send the browser to `redirect_to`", body = ApprovalDto),
        (status = 401, description = "`unauthenticated`: no session cookie, or the request carried a bearer token", body = Problem),
        (status = 422, description = "As for `GET /oauth/authorize`", body = Problem),
    ))]
async fn approve_authorization(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    ApiJson(request): ApiJson<AuthorizationRequestDto>,
) -> Result<Json<ApprovalDto>, ApiError> {
    let approval = state
        .services
        .approve_authorization(principal, request.as_params())
        .await?;
    // The code is base64url, but the state is the client's own text, so both are encoded.
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("code", approval.code.expose())
        .append_pair("state", &approval.request.state)
        .finish();
    Ok(Json(ApprovalDto {
        // A registered redirect URI has no query (domain::oauth::RedirectUri), so this one starts it.
        redirect_to: format!("{}?{query}", approval.request.redirect_uri.as_str()),
    }))
}

fn to_token_response(pair: TokenPair) -> TokenResponseDto {
    TokenResponseDto {
        access_token: pair.access_token.expose().to_owned(),
        token_type: "Bearer".to_owned(),
        expires_in: pair.expires_in.as_seconds_ceil(),
        refresh_token: pair.refresh_token.expose().to_owned(),
    }
}

/// A refusal on the token endpoints, in the RFC's format.
#[derive(Debug)]
struct OAuthRefusal {
    status: StatusCode,
    error: &'static str,
    description: String,
}

impl OAuthRefusal {
    fn invalid_request(description: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "invalid_request",
            description: description.into(),
        }
    }

    fn missing(parameter: &str) -> Self {
        Self::invalid_request(format!("{parameter} is missing"))
    }

    /// The refusal for a service error, or the error itself when it isn't the client's doing, which keeps its
    /// problem response.
    fn from_service(err: ServiceError) -> Result<Self, ApiError> {
        match err {
            ServiceError::InvalidGrant(grant) => Ok(Self {
                status: StatusCode::BAD_REQUEST,
                error: "invalid_grant",
                description: grant.to_string(),
            }),
            ServiceError::Authorization(AuthorizationError::ClientUnknown) => Ok(Self {
                status: StatusCode::UNAUTHORIZED,
                error: "invalid_client",
                description: AuthorizationError::ClientUnknown.to_string(),
            }),
            other => Err(ApiError::Service(other)),
        }
    }
}

impl IntoResponse for OAuthRefusal {
    fn into_response(self) -> Response {
        tracing::debug!(error = self.error, "token request refused");
        let body = OAuthErrorDto {
            error: self.error.to_owned(),
            error_description: self.description,
        };
        (self.status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
    }
}

/// What a token endpoint answers when it fails: a refusal in the RFC's format, or a server failure as a problem.
enum TokenFailure {
    Refusal(OAuthRefusal),
    Server(ApiError),
}

impl From<OAuthRefusal> for TokenFailure {
    fn from(refusal: OAuthRefusal) -> Self {
        Self::Refusal(refusal)
    }
}

impl From<ServiceError> for TokenFailure {
    fn from(err: ServiceError) -> Self {
        match OAuthRefusal::from_service(err) {
            Ok(refusal) => Self::Refusal(refusal),
            Err(server) => Self::Server(server),
        }
    }
}

impl From<ApiError> for TokenFailure {
    fn from(err: ApiError) -> Self {
        Self::Server(err)
    }
}

impl IntoResponse for TokenFailure {
    fn into_response(self) -> Response {
        match self {
            Self::Refusal(refusal) => refusal.into_response(),
            Self::Server(err) => err.into_response(),
        }
    }
}

/// Exchanges an authorization code, or a refresh token, for an access token and a new refresh token.
///
/// A refresh token is good once. Presenting one that was used already revokes every token of the sign-in it came
/// from, as someone else must hold a copy; so does presenting a code a second time within its ten minutes.
#[utoipa::path(post, path = "/oauth/token", tag = "oauth",
    request_body(content = TokenRequestForm, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Issued; `Cache-Control: no-store`", body = TokenResponseDto),
        (status = 400, description = "`invalid_request`, `invalid_grant` or `unsupported_grant_type`", body = OAuthErrorDto),
        (status = 401, description = "`invalid_client`: no client with this `client_id`", body = OAuthErrorDto),
    ))]
async fn issue_token(
    State(state): State<AppState>,
    form: Result<Form<TokenRequestForm>, FormRejection>,
) -> Result<Response, TokenFailure> {
    let Form(form) = form.map_err(|rejection| OAuthRefusal::invalid_request(rejection.body_text()))?;
    let client_id = form.client_id.as_deref().ok_or(OAuthRefusal::missing("client_id"))?;
    let pair = match form.grant_type.as_str() {
        "authorization_code" => {
            // Before the exchange starts, so a bad name doesn't use the code up.
            let device_name = match form.device_name.as_deref() {
                Some(raw) => DeviceName::parse(raw).map_err(|err| OAuthRefusal::invalid_request(err.to_string()))?,
                None => None,
            };
            let code = OAuthSecret::from_client(form.code.ok_or(OAuthRefusal::missing("code"))?);
            let exchange = CodeExchange {
                device_name: device_name.as_ref(),
                client_id,
                code: &code,
                redirect_uri: form
                    .redirect_uri
                    .as_deref()
                    .ok_or(OAuthRefusal::missing("redirect_uri"))?,
                code_verifier: form
                    .code_verifier
                    .as_deref()
                    .ok_or(OAuthRefusal::missing("code_verifier"))?,
            };
            state.services.exchange_authorization_code(exchange).await?
        }
        "refresh_token" => {
            let token = OAuthSecret::from_client(form.refresh_token.ok_or(OAuthRefusal::missing("refresh_token"))?);
            state.services.refresh_tokens(client_id, &token).await?
        }
        _ => {
            return Err(OAuthRefusal {
                status: StatusCode::BAD_REQUEST,
                error: "unsupported_grant_type",
                description: "grant_type must be authorization_code or refresh_token".to_owned(),
            }
            .into());
        }
    };
    // Tokens mustn't be kept by a cache on the way (RFC 6749 §5.1).
    let headers = [(header::CACHE_CONTROL, "no-store"), (header::PRAGMA, "no-cache")];
    Ok((headers, Json(to_token_response(pair))).into_response())
}

/// Revokes a token, and with it every token of the sign-in it came from.
///
/// Answers `200` for a token the server doesn't know too, as RFC 7009 asks, so the answer says nothing about which
/// tokens exist and revoking twice is fine.
#[utoipa::path(post, path = "/oauth/revoke", tag = "oauth",
    request_body(content = RevokeRequestForm, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Revoked, or nothing to revoke"),
        (status = 400, description = "`invalid_request`: no `token`", body = OAuthErrorDto),
        (status = 401, description = "`invalid_client`: no client with this `client_id`", body = OAuthErrorDto),
    ))]
async fn revoke_token(
    State(state): State<AppState>,
    form: Result<Form<RevokeRequestForm>, FormRejection>,
) -> Result<StatusCode, TokenFailure> {
    let Form(form) = form.map_err(|rejection| OAuthRefusal::invalid_request(rejection.body_text()))?;
    let token = OAuthSecret::from_client(form.token.ok_or(OAuthRefusal::missing("token"))?);
    state.services.revoke_token(form.client_id.as_deref(), &token).await?;
    Ok(StatusCode::OK)
}
