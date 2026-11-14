//! Signing in through one OpenID Connect provider (architecture.md, Authentication and authorization).
//!
//! A sign-in starts with a row in `oidc_sign_ins` holding the nonce and PKCE verifier, keyed by the hash of the
//! `state` the browser carries in a cookie and to the provider. The callback finds the row by it, has openidconnect
//! exchange the code and check the ID token, and then either opens a session for a known identity or keeps the
//! verified name and email on the row for the username step. Discovery is fetched on every start and callback, so a
//! provider that is down never stops the server.
//!
//! Nothing here logs a token, a code, the client secret, the state or the verifier.

use std::time::Duration as StdDuration;

use domain::handle::Handle;
use domain::oidc::{choose_display_name, is_app_path, pending_sign_in_ends_at};
use domain::text::Name;
use domain::user::{DisplayName, Email};
use domain::validation::Field;
use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use store::oidc_sign_ins::{NewOidcSignIn, OidcSignIn, VerifiedIdentity};
use store::{StoreError, TransactionTrait};

use crate::auth::{SessionGrant, random_token, sha256_hex};
use crate::{ServiceError, Services};

/// The path the provider sends the person back to, after `BASE_URL`.
const CALLBACK_PATH: &str = "/api/v1/auth/oidc/callback";

/// The provider's answers are awaited this long.
const HTTP_TIMEOUT: StdDuration = StdDuration::from_secs(10);

/// Sign-ins in flight that ended are deleted this many at a time, by each start.
const HOUSEKEEPING_BATCH: u64 = 10;

/// The provider, from the operator's configuration (README, Configuration).
#[derive(Clone, Debug)]
pub struct OidcSettings {
    /// As configured, trailing `/` included: the ID token's `iss` must equal it byte for byte, and identities are
    /// keyed by it.
    issuer: IssuerUrl,
    client_id: ClientId,
    /// Redacted in `Debug`.
    client_secret: ClientSecret,
    /// What is asked besides `openid`, which openidconnect always sends.
    scopes: Vec<String>,
    label: String,
    /// `BASE_URL`, without a trailing slash: where a finished sign-in sends the browser.
    app_url: String,
    redirect_uri: RedirectUrl,
    /// Follows no redirects, which would open the server to SSRF through a provider's answers, and gives up after
    /// [`HTTP_TIMEOUT`].
    http: reqwest::Client,
}

/// Why the provider's configuration can't be used.
#[derive(Debug, thiserror::Error)]
pub enum OidcSettingsError {
    #[error("the issuer is not a valid URL")]
    Issuer(#[source] openidconnect::url::ParseError),
    #[error("the label is empty")]
    EmptyLabel,
    #[error("the redirect URI is not a valid URL")]
    RedirectUri(#[source] openidconnect::url::ParseError),
    #[error("can't build the HTTP client")]
    HttpClient(#[source] reqwest::Error),
}

impl OidcSettings {
    /// Checks the configuration and builds the HTTP client. Makes no request. `scopes` is what was asked besides
    /// `openid`, which is dropped if present. `app_url` is `BASE_URL` without a trailing slash.
    pub fn new(
        issuer: &str,
        client_id: &str,
        client_secret: &str,
        label: &str,
        scopes: &str,
        app_url: &str,
    ) -> Result<Self, OidcSettingsError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(OidcSettingsError::EmptyLabel);
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(OidcSettingsError::HttpClient)?;
        Ok(Self {
            issuer: IssuerUrl::new(issuer.to_owned()).map_err(OidcSettingsError::Issuer)?,
            client_id: ClientId::new(client_id.to_owned()),
            client_secret: ClientSecret::new(client_secret.to_owned()),
            scopes: scopes
                .split_whitespace()
                .filter(|scope| *scope != "openid")
                .map(str::to_owned)
                .collect(),
            label: label.to_owned(),
            app_url: app_url.to_owned(),
            redirect_uri: RedirectUrl::new(format!("{app_url}{CALLBACK_PATH}"))
                .map_err(OidcSettingsError::RedirectUri)?,
            http,
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// The issuer exactly as configured: the key of every identity.
    pub fn issuer(&self) -> &str {
        self.issuer.as_str()
    }
}

/// Why a sign-in was refused, which the sign-in page says in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OidcRefusal {
    EmailTaken,
    EmailUnverified,
    ProviderRefused,
    SignInExpired,
    ProviderUnreachable,
}

impl OidcRefusal {
    /// The `sign_in_error` the sign-in page is sent with.
    pub fn code(self) -> &'static str {
        match self {
            Self::EmailTaken => "email_taken",
            Self::EmailUnverified => "email_unverified",
            Self::ProviderRefused => "provider_refused",
            Self::SignInExpired => "sign_in_expired",
            Self::ProviderUnreachable => "provider_unreachable",
        }
    }
}

/// Where a refused sign-in goes: the sign-in page, told why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OidcRefused {
    pub reason: OidcRefusal,
    /// The page the sign-in started from, so a refusal there keeps it.
    pub redirect: Option<String>,
    /// The email of the existing account, for [`OidcRefusal::EmailTaken`].
    pub login: Option<String>,
}

/// How a sign-in starts.
#[derive(Debug)]
pub enum OidcStart {
    /// Send the browser to the provider, and set the state cookie.
    Authorize {
        authorize_url: String,
        state: String,
    },
    Refused(OidcRefused),
}

/// What the provider's redirect to the callback carried.
#[derive(Debug)]
pub struct OidcCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// How a sign-in ends at the callback. In each case the state cookie ends.
#[derive(Debug)]
pub enum OidcCallback {
    /// A known identity: a session, and the page to go to.
    SignedIn {
        grant: SessionGrant,
        redirect: Option<String>,
    },
    /// A new person: the username step. The state cookie is kept for it.
    NeedsUsername,
    Refused(OidcRefused),
}

/// What the username step shows of the person.
#[derive(Debug)]
pub struct OidcPending {
    pub display_name: DisplayName,
    pub email: Email,
}

/// A user made by the username step.
#[derive(Debug)]
pub struct OidcRegistered {
    pub grant: SessionGrant,
    pub redirect: Option<String>,
}

type DiscoveryFailure = openidconnect::DiscoveryError<openidconnect::HttpClientError<reqwest::Error>>;
type ProviderClient =
    CoreClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointMaybeSet, EndpointMaybeSet>;

impl Services {
    /// The provider's label, or [`ServiceError::NotFound`] when the server has none.
    pub fn oidc_label(&self) -> Result<String, ServiceError> {
        Ok(self.oidc()?.label.clone())
    }

    /// `BASE_URL` without a trailing slash, which the pages a sign-in sends the browser to are under.
    pub fn oidc_app_url(&self) -> Result<String, ServiceError> {
        Ok(self.oidc()?.app_url.clone())
    }

    fn oidc(&self) -> Result<&OidcSettings, ServiceError> {
        self.settings.oidc.as_ref().ok_or(ServiceError::NotFound)
    }

    /// Starts a sign-in: stores it and says where to send the browser. `redirect` is kept only if it is an app path
    /// ([`is_app_path`]). A provider that can't be reached refuses the sign-in.
    pub async fn start_oidc_sign_in(&self, redirect: Option<&str>) -> Result<OidcStart, ServiceError> {
        let oidc = self.oidc()?;
        let redirect = redirect.filter(|path| is_app_path(path));
        let now = self.now();
        if let Err(error) = store::oidc_sign_ins::delete_ended_oidc_sign_ins(&self.db, now, HOUSEKEEPING_BATCH).await {
            tracing::warn!(?error, "can't delete ended provider sign-ins");
        }
        let client = match discover(oidc).await {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(
                    kind = discovery_kind(&error),
                    "provider sign-in not started: discovery failed"
                );
                return Ok(OidcStart::Refused(OidcRefused {
                    reason: OidcRefusal::ProviderUnreachable,
                    redirect: redirect.map(str::to_owned),
                    login: None,
                }));
            }
        };

        let state = random_token();
        let nonce = random_token();
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let ends_at =
            pending_sign_in_ends_at(now).ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        store::oidc_sign_ins::insert_oidc_sign_in(
            &self.db,
            &NewOidcSignIn {
                state_hash: &sha256_hex(&state),
                nonce: &nonce,
                pkce_verifier: verifier.secret(),
                redirect,
                now,
                expires_at: ends_at,
            },
        )
        .await?;

        let (csrf, nonce_value) = (CsrfToken::new(state.clone()), Nonce::new(nonce));
        let (authorize_url, _, _) = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                move || csrf,
                move || nonce_value,
            )
            .add_scopes(oidc.scopes.iter().cloned().map(Scope::new))
            .set_pkce_challenge(challenge)
            .url();
        tracing::info!("provider sign-in started");
        Ok(OidcStart::Authorize {
            authorize_url: authorize_url.to_string(),
            state,
        })
    }

    /// Ends a sign-in at the callback. `cookie_state` is the state cookie of the browser, which must be the `state`
    /// the provider sent back.
    ///
    /// Every refusal deletes the sign-in, makes nothing and is logged with its reason. A known identity gets a
    /// session, as after a password; a new one is kept on the sign-in for [`Services::register_oidc_user`], and
    /// only if its email is verified and no account has it.
    pub async fn finish_oidc_sign_in(
        &self,
        cookie_state: Option<&str>,
        query: OidcCallbackQuery,
    ) -> Result<OidcCallback, ServiceError> {
        let oidc = self.oidc()?;
        let now = self.now();
        let state_hash = query.state.as_deref().map(sha256_hex);
        let found = match &state_hash {
            Some(hash) => store::oidc_sign_ins::find_oidc_sign_in(&self.db, hash).await?,
            None => None,
        };
        let redirect = found.as_ref().and_then(|row| row.redirect.clone());
        let refuse = async |reason: OidcRefusal, log: &str, login: Option<String>| {
            if let Some(hash) = &state_hash {
                store::oidc_sign_ins::delete_oidc_sign_in(&self.db, hash).await?;
            }
            tracing::info!(reason = reason.code(), "provider sign-in refused: {log}");
            Ok::<_, ServiceError>(OidcCallback::Refused(OidcRefused {
                reason,
                redirect: redirect.clone(),
                login,
            }))
        };

        let (current, state_hash) = match (&found, cookie_state, query.state.as_deref(), &state_hash) {
            (Some(row), Some(cookie), Some(state), Some(hash)) if cookie == state && row.expires_at > now => {
                (row, hash)
            }
            _ => return refuse(OidcRefusal::SignInExpired, "no live sign-in for this browser", None).await,
        };
        let Some(code) = query.code.filter(|_| query.error.is_none()) else {
            return refuse(
                OidcRefusal::ProviderRefused,
                "the provider sent an error or no code",
                None,
            )
            .await;
        };

        let claims = match verify_code(oidc, code, current).await {
            Ok(claims) => claims,
            Err(failure) => return refuse(failure.reason(), failure.log(), None).await,
        };
        if let Some(user_id) = store::identities::find_identity_user(&self.db, oidc.issuer(), &claims.subject).await? {
            let user = store::users::find_user(&self.db, user_id)
                .await?
                .ok_or_else(|| StoreError::corrupt("user_identities", &claims.subject, "no such user"))?;
            let txn = self.db.begin().await.map_err(StoreError::from)?;
            let grant = self.open_session(&txn, user, now).await?;
            store::oidc_sign_ins::delete_oidc_sign_in(&txn, state_hash).await?;
            txn.commit().await.map_err(StoreError::from)?;
            tracing::info!(user_id = %grant.user.id, "user signed in through the provider");
            return Ok(OidcCallback::SignedIn {
                grant,
                redirect: current.redirect.clone(),
            });
        }

        let Some(email) = claims.verified_email() else {
            return refuse(OidcRefusal::EmailUnverified, "no verified email", None).await;
        };
        if store::users::find_user_by_email(&self.db, &email).await?.is_some() {
            return refuse(
                OidcRefusal::EmailTaken,
                "an account has this email",
                Some(email.as_str().to_owned()),
            )
            .await;
        }
        let Some(display_name) = choose_display_name(claims.name.as_deref(), email.as_str()) else {
            return refuse(OidcRefusal::ProviderRefused, "no usable display name", None).await;
        };
        let ends_at =
            pending_sign_in_ends_at(now).ok_or(ServiceError::Invalid(domain::ValidationError::TimestampOutOfRange))?;
        let identity = VerifiedIdentity {
            subject: claims.subject,
            email,
            display_name,
        };
        if !store::oidc_sign_ins::set_oidc_sign_in_verified(&self.db, state_hash, &identity, ends_at).await? {
            return refuse(OidcRefusal::SignInExpired, "the sign-in ended meanwhile", None).await;
        }
        tracing::info!("provider sign-in verified, waiting for a username");
        Ok(OidcCallback::NeedsUsername)
    }

    /// The name and email the username step shows. [`ServiceError::NotFound`] when there is no provider, no such
    /// sign-in, it isn't verified yet, or it ended.
    pub async fn load_oidc_pending(&self, state: &str) -> Result<OidcPending, ServiceError> {
        let (_, verified) = self.find_verified_oidc_sign_in(state).await?;
        Ok(OidcPending {
            display_name: verified.display_name,
            email: verified.email,
        })
    }

    /// Makes the user of a verified sign-in with the username they picked: the user, their personal team and first
    /// heap, the identity and a session, in one transaction, and ends the sign-in. Registration being closed doesn't
    /// matter. Fails like registering when the username or the email is taken, and leaves the sign-in for another
    /// try; [`ServiceError::NotFound`] when there is none to finish, so a second call after success finds nothing.
    pub async fn register_oidc_user(&self, state: &str, username: &str) -> Result<OidcRegistered, ServiceError> {
        let oidc = self.oidc()?;
        let (row, verified) = self.find_verified_oidc_sign_in(state).await?;
        let username = Handle::parse(username, Field::Username)?;
        let team_name = Name::parse(verified.display_name.as_str())?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let made = self
            .insert_personal_user(&txn, verified.email, username, verified.display_name, team_name, now)
            .await?;
        store::identities::insert_identity(&txn, made.user.id, oidc.issuer(), &verified.subject, now).await?;
        let grant = self.open_session(&txn, made.user, now).await?;
        store::oidc_sign_ins::delete_oidc_sign_in(&txn, &sha256_hex(state)).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %grant.user.id, team_id = %made.team.id, heap_id = %made.heap.id, "user registered through the provider");
        Ok(OidcRegistered {
            grant,
            redirect: row.redirect,
        })
    }

    /// The sign-in `state` names, when the provider has verified the person and it hasn't ended.
    async fn find_verified_oidc_sign_in(&self, state: &str) -> Result<(OidcSignIn, VerifiedIdentity), ServiceError> {
        self.oidc()?;
        let row = store::oidc_sign_ins::find_oidc_sign_in(&self.db, &sha256_hex(state))
            .await?
            .filter(|row| row.expires_at > self.now())
            .ok_or(ServiceError::NotFound)?;
        let verified = row.verified.clone().ok_or(ServiceError::NotFound)?;
        Ok((row, verified))
    }
}

/// Fetches the provider's discovery document and keys, and makes the client for them.
async fn discover(oidc: &OidcSettings) -> Result<ProviderClient, DiscoveryFailure> {
    let metadata = CoreProviderMetadata::discover_async(oidc.issuer.clone(), &oidc.http).await?;
    Ok(
        CoreClient::from_provider_metadata(metadata, oidc.client_id.clone(), Some(oidc.client_secret.clone()))
            .set_redirect_uri(oidc.redirect_uri.clone()),
    )
}

fn discovery_kind(error: &DiscoveryFailure) -> &'static str {
    match error {
        openidconnect::DiscoveryError::Request(_) => "request failed",
        openidconnect::DiscoveryError::Response(..) => "bad status",
        openidconnect::DiscoveryError::Parse(_) => "unparsable document",
        openidconnect::DiscoveryError::Validation(_) => "invalid document",
        // `DiscoveryError` is non-exhaustive.
        _ => "other",
    }
}

/// What the ID token said, once checked.
struct IdentityClaims {
    subject: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
}

impl IdentityClaims {
    /// The email, only when the provider says it is verified and it is an address.
    fn verified_email(&self) -> Option<Email> {
        self.email
            .as_deref()
            .filter(|_| self.email_verified == Some(true))
            .and_then(|email| Email::parse(email).ok())
    }
}

/// Why a code couldn't be turned into checked claims.
enum VerifyFailure {
    Unreachable(&'static str),
    Refused(&'static str),
}

impl VerifyFailure {
    fn reason(&self) -> OidcRefusal {
        match self {
            Self::Unreachable(_) => OidcRefusal::ProviderUnreachable,
            Self::Refused(_) => OidcRefusal::ProviderRefused,
        }
    }

    fn log(&self) -> &'static str {
        match self {
            Self::Unreachable(what) | Self::Refused(what) => what,
        }
    }
}

/// Exchanges the code with the sign-in's verifier, and checks the ID token's signature, issuer, audience, expiry and
/// the sign-in's nonce. A network failure is told apart from a provider that answered no.
async fn verify_code(oidc: &OidcSettings, code: String, sign_in: &OidcSignIn) -> Result<IdentityClaims, VerifyFailure> {
    let client = match discover(oidc).await {
        Ok(client) => client,
        Err(error @ openidconnect::DiscoveryError::Request(_)) => {
            return Err(VerifyFailure::Unreachable(discovery_kind(&error)));
        }
        Err(error) => return Err(VerifyFailure::Refused(discovery_kind(&error))),
    };
    let request = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|_| VerifyFailure::Refused("no token endpoint"))?
        .set_pkce_verifier(PkceCodeVerifier::new(sign_in.pkce_verifier.clone()));
    let response = request.request_async(&oidc.http).await.map_err(|error| match error {
        openidconnect::RequestTokenError::Request(_) => VerifyFailure::Unreachable("token request failed"),
        openidconnect::RequestTokenError::ServerResponse(_) => VerifyFailure::Refused("token request refused"),
        openidconnect::RequestTokenError::Parse(..) => VerifyFailure::Refused("unparsable token response"),
        openidconnect::RequestTokenError::Other(_) => VerifyFailure::Refused("token request failed"),
    })?;
    let id_token = response.id_token().ok_or(VerifyFailure::Refused("no ID token"))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(sign_in.nonce.clone()))
        .map_err(|_| VerifyFailure::Refused("ID token not valid"))?;
    Ok(IdentityClaims {
        subject: claims.subject().as_str().to_owned(),
        email: claims.email().map(|email| email.as_str().to_owned()),
        email_verified: claims.email_verified(),
        name: claims
            .name()
            .and_then(|name| name.get(None))
            .map(|name| name.as_str().trim().to_owned()),
    })
}
