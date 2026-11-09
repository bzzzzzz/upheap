//! OAuth 2.0 for first-party non-web clients such as the CLI: the authorization code flow with PKCE (RFC 6749,
//! RFC 7636) and token revocation (RFC 7009).
//!
//! The consent page's endpoints take JSON and answer problems; the client's, `POST /oauth/token` and
//! `POST /oauth/revoke`, take form-encoded bodies and refuse in the RFC 6749 §5.2 format, [`OAuthErrorDto`].

use domain::oauth::AuthorizationParams;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// The CLI's client ID, the one first-party client the server registers.
pub const CLI_CLIENT_ID: &str = domain::oauth::CLI_CLIENT.id;

/// An authorization request, as the client put it in the consent page's address (RFC 6749 §4.1.1, RFC 7636 §4.3).
/// Every value is checked on the server; a missing one is refused with the problem for that value.
#[derive(Clone, Debug, Serialize, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[schema(as = OAuthAuthorizationRequest)]
pub struct AuthorizationRequestDto {
    /// Must be `code`.
    #[serde(default)]
    pub response_type: String,
    /// A registered client: `upheap-cli`.
    #[serde(default)]
    pub client_id: String,
    /// `http://127.0.0.1:<port><path>` or `http://[::1]:<port><path>`, with no query or fragment.
    #[serde(default)]
    pub redirect_uri: String,
    /// The S256 challenge of the client's PKCE code verifier.
    #[serde(default)]
    pub code_challenge: String,
    /// Must be `S256`.
    #[serde(default)]
    pub code_challenge_method: String,
    /// The client's value, sent back to the redirect URI with the code.
    #[serde(default)]
    pub state: String,
}

impl AuthorizationRequestDto {
    /// The request's values, unchecked, as the domain checks them.
    pub fn as_params(&self) -> AuthorizationParams<'_> {
        AuthorizationParams {
            response_type: &self.response_type,
            client_id: &self.client_id,
            redirect_uri: &self.redirect_uri,
            code_challenge: &self.code_challenge,
            code_challenge_method: &self.code_challenge_method,
            state: &self.state,
        }
    }
}

/// The client an authorization request is from, once the request is known to be good.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthClient)]
pub struct ClientDto {
    pub client_id: String,
    /// What to call the client when asking the user, such as "Upheap CLI".
    pub display_name: String,
}

/// Where to send the browser now that the user allowed the request.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthApproval)]
pub struct ApprovalDto {
    /// The request's redirect URI with `code` and `state` added.
    pub redirect_to: String,
}

/// A token request (RFC 6749 §4.1.3 and §6), form-encoded.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthTokenRequest)]
pub struct TokenRequestForm {
    /// `authorization_code` or `refresh_token`.
    pub grant_type: String,
    /// A registered client: `upheap-cli`. Required with either grant, as the client is public.
    pub client_id: Option<String>,
    /// With `authorization_code`: the code the redirect URI received.
    pub code: Option<String>,
    /// With `authorization_code`: the redirect URI the code was issued for, exactly.
    pub redirect_uri: Option<String>,
    /// With `authorization_code`: the PKCE code verifier the challenge was made from.
    pub code_verifier: Option<String>,
    /// With `refresh_token`: the refresh token, which this request uses up.
    pub refresh_token: Option<String>,
}

/// A token revocation request (RFC 7009 §2.1), form-encoded.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthRevokeRequest)]
pub struct RevokeRequestForm {
    /// An access or refresh token. Either ends the client's sign-in.
    pub token: Option<String>,
    /// `access_token` or `refresh_token`. Accepted and not needed: the server finds the token either way.
    pub token_type_hint: Option<String>,
    /// When given, a token of another client is left alone.
    pub client_id: Option<String>,
}

/// A successful token response (RFC 6749 §5.1).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthTokens)]
pub struct TokenResponseDto {
    /// Send as `Authorization: Bearer <access_token>`.
    pub access_token: String,
    /// Always `Bearer`.
    pub token_type: String,
    /// Seconds until the access token expires.
    pub expires_in: i64,
    /// Good for one refresh, within 30 days.
    pub refresh_token: String,
}

/// A refused token or revocation request (RFC 6749 §5.2). The token endpoints answer with this, not a `Problem`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OAuthError)]
pub struct OAuthErrorDto {
    /// `invalid_request`, `invalid_client`, `invalid_grant` or `unsupported_grant_type`.
    pub error: String,
    /// What was wrong, for people. May change; branch on `error`.
    pub error_description: String,
}
