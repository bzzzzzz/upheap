//! A fake OpenID Connect provider: an axum server on a free local port that serves discovery, the keys and a token
//! endpoint, and signs ID tokens with the fixed test key in `tests/fixtures`.
//!
//! The test reads `state`, `nonce` and `code_challenge` from the `Location` of the start, says which claims the
//! provider vouches for with [`FakeProvider::code_for`], and sends the code to the callback. The token endpoint
//! checks the PKCE verifier against the challenge, so a callback that exchanged a wrong one fails.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use openidconnect::core::{CoreIdToken, CoreIdTokenClaims, CoreJwsSigningAlgorithm, CoreRsaPrivateSigningKey};
use openidconnect::{
    Audience, EmptyAdditionalClaims, EndUserEmail, EndUserName, IssuerUrl, JsonWebKeyId, LocalizedClaim, Nonce,
    PrivateSigningKey, StandardClaims, SubjectIdentifier,
};
use serde::Deserialize;
use serde_json::{Value, json};
use service::oidc::OidcSettings;
use tokio::net::TcpListener;

pub const CLIENT_ID: &str = "upheap-test";
pub const CLIENT_SECRET: &str = "test-client-secret";
pub const LABEL: &str = "Acme SSO";
/// The public URL the tests give the server.
pub const APP_URL: &str = "https://upheap.example.test";

/// A test key, made for these tests only. It signs nothing real.
const TEST_SIGNING_KEY: &str = include_str!("../fixtures/oidc-test-signing-key.pem");
/// A second test key that the provider doesn't publish, to sign tokens that must be rejected.
const WRONG_SIGNING_KEY: &str = include_str!("../fixtures/oidc-test-wrong-signing-key.pem");
const KEY_ID: &str = "test-key";

/// What the provider vouches for with one code.
#[derive(Clone, Debug)]
pub struct Claims {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    /// What is wrong with the ID token, if anything. A valid token by default.
    pub fault: TokenFault,
}

/// How the fake provider spoils an ID token, to show the server rejects it.
#[derive(Clone, Debug, Default)]
pub struct TokenFault {
    /// Instead of the nonce the start sent.
    pub nonce: Option<String>,
    /// Instead of the client ID.
    pub audience: Option<String>,
    /// Instead of the issuer.
    pub issuer: Option<String>,
    /// Instead of one hour from now (a negative one is already past).
    pub expires_in: Option<chrono::Duration>,
    /// Sign with the key the provider doesn't publish.
    pub wrong_key: bool,
}

impl Claims {
    /// A person with a verified email address and a name.
    pub fn verified(subject: &str, email: &str, name: &str) -> Self {
        Self {
            subject: subject.to_owned(),
            email: Some(email.to_owned()),
            email_verified: Some(true),
            name: Some(name.to_owned()),
            fault: TokenFault::default(),
        }
    }
}

/// What the start sent to the provider's authorization endpoint.
#[derive(Debug)]
pub struct AuthorizationRequest {
    pub state: String,
    pub nonce: String,
    pub code_challenge: String,
}

struct Grant {
    claims: Claims,
    nonce: String,
    code_challenge: String,
}

struct Shared {
    issuer: String,
    key: CoreRsaPrivateSigningKey,
    wrong_key: CoreRsaPrivateSigningKey,
    grants: Mutex<HashMap<String, Grant>>,
}

pub struct FakeProvider {
    /// With a trailing slash and a path, to show that the issuer is used exactly as configured.
    pub issuer: String,
    shared: Arc<Shared>,
}

impl FakeProvider {
    pub async fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let issuer = format!("http://{}/realm/", listener.local_addr()?);
        let key = CoreRsaPrivateSigningKey::from_pem(TEST_SIGNING_KEY, Some(JsonWebKeyId::new(KEY_ID.to_owned())))
            .map_err(anyhow::Error::msg)?;
        let wrong_key =
            CoreRsaPrivateSigningKey::from_pem(WRONG_SIGNING_KEY, Some(JsonWebKeyId::new(KEY_ID.to_owned())))
                .map_err(anyhow::Error::msg)?;
        let shared = Arc::new(Shared {
            issuer: issuer.clone(),
            key,
            wrong_key,
            grants: Mutex::new(HashMap::new()),
        });
        let router = Router::new()
            .route("/realm/.well-known/openid-configuration", get(discovery))
            .route("/realm/keys", get(keys))
            .route("/realm/token", post(token))
            .with_state(shared.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self { issuer, shared })
    }

    /// The server's settings for this provider.
    pub fn settings(&self) -> Result<OidcSettings> {
        settings_for(&self.issuer)
    }

    /// A code that the token endpoint turns into an ID token with `claims`, for the sign-in `request` started.
    pub fn code_for(&self, request: &AuthorizationRequest, claims: Claims) -> String {
        let code = format!("code-{}", request.state);
        let grant = Grant {
            claims,
            nonce: request.nonce.clone(),
            code_challenge: request.code_challenge.clone(),
        };
        if let Ok(mut grants) = self.shared.grants.lock() {
            grants.insert(code.clone(), grant);
        }
        code
    }
}

/// The server's settings for a provider at `issuer`.
pub fn settings_for(issuer: &str) -> Result<OidcSettings> {
    OidcSettings::new(issuer, CLIENT_ID, CLIENT_SECRET, LABEL, "openid email profile", APP_URL).context("oidc settings")
}

/// Reads what the start sent from the `Location` it answered.
pub fn parse_authorization(location: &str) -> Result<AuthorizationRequest> {
    let url = location.split_once('?').context("no query")?.1;
    let query: HashMap<String, String> = form_urlencoded::parse(url.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let get = |name: &str| query.get(name).cloned().with_context(|| format!("no {name}"));
    anyhow::ensure!(get("response_type")? == "code", "not a code flow");
    anyhow::ensure!(get("client_id")? == CLIENT_ID, "wrong client");
    anyhow::ensure!(
        get("redirect_uri")? == format!("{APP_URL}/api/v1/auth/oidc/callback"),
        "wrong redirect URI"
    );
    anyhow::ensure!(get("code_challenge_method")? == "S256", "not S256");
    let scopes = get("scope")?;
    anyhow::ensure!(
        scopes.split(' ').any(|s| s == "openid") && scopes.contains("email"),
        "scopes {scopes}"
    );
    Ok(AuthorizationRequest {
        state: get("state")?,
        nonce: get("nonce")?,
        code_challenge: get("code_challenge")?,
    })
}

async fn discovery(State(shared): State<Arc<Shared>>) -> Json<Value> {
    let issuer = &shared.issuer;
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}authorize"),
        "token_endpoint": format!("{issuer}token"),
        "jwks_uri": format!("{issuer}keys"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
    }))
}

async fn keys(State(shared): State<Arc<Shared>>) -> Json<Value> {
    let key = shared.key.as_verification_key();
    Json(json!({ "keys": [key] }))
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    code_verifier: String,
}

async fn token(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Form(request): Form<TokenRequest>,
) -> axum::response::Response {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("Basic "));
    let grant = shared
        .grants
        .lock()
        .ok()
        .and_then(|mut grants| grants.remove(&request.code));
    let (Some(grant), true, "authorization_code") = (grant, basic, request.grant_type.as_str()) else {
        return invalid_grant();
    };
    if s256(&request.code_verifier) != grant.code_challenge {
        return invalid_grant();
    }
    match id_token(&shared, &grant) {
        Some(id_token) => Json(json!({
            "access_token": "access-token",
            "token_type": "Bearer",
            "id_token": id_token,
        }))
        .into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn invalid_grant() -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" }))).into_response()
}

/// The PKCE S256 challenge of `verifier`.
fn s256(verifier: &str) -> String {
    use openidconnect::PkceCodeChallenge;
    PkceCodeChallenge::from_code_verifier_sha256(&openidconnect::PkceCodeVerifier::new(verifier.to_owned()))
        .as_str()
        .to_owned()
}

/// Signed with real time: openidconnect checks the token's times against the system clock, not the server's.
fn id_token(shared: &Shared, grant: &Grant) -> Option<String> {
    let now = chrono::Utc::now();
    let mut standard = StandardClaims::new(SubjectIdentifier::new(grant.claims.subject.clone()));
    if let Some(email) = &grant.claims.email {
        standard = standard.set_email(Some(EndUserEmail::new(email.clone())));
    }
    standard = standard.set_email_verified(grant.claims.email_verified);
    if let Some(name) = &grant.claims.name {
        standard = standard.set_name(Some(LocalizedClaim::from(EndUserName::new(name.clone()))));
    }
    let fault = &grant.claims.fault;
    let issuer = fault.issuer.as_ref().unwrap_or(&shared.issuer);
    let audience = fault.audience.as_deref().unwrap_or(CLIENT_ID);
    let expires_in = fault.expires_in.unwrap_or(chrono::Duration::hours(1));
    let nonce = fault.nonce.as_ref().unwrap_or(&grant.nonce);
    // Issued before it expired, even when it has.
    let issued_at = now.min(now + expires_in - chrono::Duration::minutes(1));
    let claims = CoreIdTokenClaims::new(
        IssuerUrl::new(issuer.clone()).ok()?,
        vec![Audience::new(audience.to_owned())],
        now + expires_in,
        issued_at,
        standard,
        EmptyAdditionalClaims {},
    )
    .set_nonce(Some(Nonce::new(nonce.clone())));
    let key = if fault.wrong_key {
        &shared.wrong_key
    } else {
        &shared.key
    };
    CoreIdToken::new(claims, key, CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256, None, None)
        .ok()
        .map(|token| token.to_string())
}
