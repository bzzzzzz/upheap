//! The HTTP client: the server's base URL, the bearer token, the `Time-Zone` header, and problem+json errors.
//!
//! Calls return the server's body as well as the value read from it, so `--json` can print the body unchanged.

use std::time::Duration;

use api_types::oauth::{OAuthErrorDto, RevokeRequestForm, TokenRequestForm, TokenResponseDto};
use api_types::problem::Problem;
use reqwest::{RequestBuilder, Response, StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The query of a request that has none.
pub const NO_QUERY: &[(&str, &str)] = &[];

/// A client of one server.
pub struct ApiClient {
    http: reqwest::Client,
    server: Url,
    time_zone: Option<String>,
}

/// A value read from a response, with the body it was read from.
pub struct Fetched<T> {
    pub value: T,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("can't set up the HTTP client")]
    Setup(#[source] reqwest::Error),
    #[error("can't reach the server at {server}")]
    Unreachable {
        server: String,
        #[source]
        source: reqwest::Error,
    },
    /// The server refused the request, or failed, with a problem response.
    #[error("{} ({})", problem.title, problem.code)]
    Problem { problem: Box<Problem>, body: String },
    /// The token endpoints' refusal (RFC 6749 §5.2).
    #[error("{} ({})", refusal.error_description, refusal.error)]
    OAuthRefused { status: StatusCode, refusal: OAuthErrorDto },
    #[error("the server answered {status} with a body the CLI doesn't understand")]
    Unreadable { status: StatusCode, body: String },
}

impl ApiClient {
    /// A client of `server` that sends `time_zone`, an IANA name, with every request, so labels and dates come back
    /// in the machine's timezone. Without one, the server uses UTC.
    pub fn new(server: Url, time_zone: Option<String>) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("upheap/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ClientError::Setup)?;
        Ok(Self {
            http,
            server,
            time_zone,
        })
    }

    /// The server's base URL.
    pub fn server(&self) -> &Url {
        &self.server
    }

    /// The web app's page at `path`, such as `["oauth", "authorize"]`, with `query`.
    pub fn web_url(&self, path: &[&str], query: &(impl Serialize + ?Sized)) -> Url {
        let mut url = self.url_of(path);
        // Plain strings always encode.
        let encoded = serde_urlencoded::to_string(query).unwrap_or_default();
        url.set_query(Some(&encoded));
        url
    }

    /// `GET /api/v1/<path>` with `query`, as the holder of `token`.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &[&str],
        query: &(impl Serialize + ?Sized),
        token: &str,
    ) -> Result<Fetched<T>, ClientError> {
        let request = self.http.get(self.api_url(path)).query(query).bearer_auth(token);
        let response = self.send(request).await?;
        read_json(response).await
    }

    /// Exchanges an authorization code or a refresh token at `POST /api/v1/oauth/token`.
    pub async fn request_tokens(&self, form: &TokenRequestForm) -> Result<TokenResponseDto, ClientError> {
        let request = self.http.post(self.api_url(&["oauth", "token"])).form(form);
        let response = self.send(request).await?;
        if !response.status().is_success() {
            return Err(read_failure(response).await);
        }
        read_json(response).await.map(|fetched| fetched.value)
    }

    /// Revokes a token, and the sign-in it belongs to, at `POST /api/v1/oauth/revoke`.
    pub async fn revoke(&self, form: &RevokeRequestForm) -> Result<(), ClientError> {
        let request = self.http.post(self.api_url(&["oauth", "revoke"])).form(form);
        let response = self.send(request).await?;
        if !response.status().is_success() {
            return Err(read_failure(response).await);
        }
        Ok(())
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response, ClientError> {
        let request = match &self.time_zone {
            Some(zone) => request.header("Time-Zone", zone),
            None => request,
        };
        request.send().await.map_err(|source| ClientError::Unreachable {
            server: self.server.to_string(),
            source,
        })
    }

    fn api_url(&self, path: &[&str]) -> Url {
        let mut full = vec!["api", "v1"];
        full.extend_from_slice(path);
        self.url_of(&full)
    }

    /// The server's URL with `segments` added to its path, each escaped as needed.
    fn url_of(&self, segments: &[&str]) -> Url {
        let mut url = self.server.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        url
    }
}

/// The body of a successful response, read as `T`. A failure is read as a problem.
async fn read_json<T: DeserializeOwned>(response: Response) -> Result<Fetched<T>, ClientError> {
    let status = response.status();
    if !status.is_success() {
        return Err(read_failure(response).await);
    }
    let body = response.text().await.map_err(|_| ClientError::Unreadable {
        status,
        body: String::new(),
    })?;
    match serde_json::from_str(&body) {
        Ok(value) => Ok(Fetched { value, body }),
        Err(_) => Err(ClientError::Unreadable { status, body }),
    }
}

/// The error a failed response stands for: an OAuth refusal, a problem, or neither.
async fn read_failure(response: Response) -> ClientError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if let Ok(problem) = serde_json::from_str::<Problem>(&body) {
        return ClientError::Problem {
            problem: Box::new(problem),
            body,
        };
    }
    if let Ok(refusal) = serde_json::from_str::<OAuthErrorDto>(&body) {
        return ClientError::OAuthRefused { status, refusal };
    }
    ClientError::Unreadable { status, body }
}
