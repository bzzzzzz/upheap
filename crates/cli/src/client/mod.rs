//! The HTTP client: the server's base URL, the bearer token, the `Time-Zone` header, and problem+json errors.
//!
//! Calls return the server's body as well as the value read from it, so `--json` can print the body unchanged.

use std::time::Duration;

use api_types::oauth::{OAuthErrorDto, RevokeRequestForm, TokenRequestForm, TokenResponseDto};
use api_types::problem::Problem;
use reqwest::{RequestBuilder, Response, StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::is_loopback;

/// Whether requests to `server` may go through a proxy from `HTTPS_PROXY`, `HTTP_PROXY` or `ALL_PROXY`. Not for a
/// server on this machine: plain `http` is allowed only there, and a proxy would see its tokens in the clear (reqwest
/// doesn't leave loopback out by itself).
fn uses_proxy(server: &Url) -> bool {
    !is_loopback(server)
}

/// The query of a request that has none.
pub const NO_QUERY: &[(&str, &str)] = &[];

/// A client of one server.
pub struct ApiClient {
    http: reqwest::Client,
    server: Url,
    time_zone: Option<String>,
    /// The run to write in, sent as `Upheap-Run` with every request.
    run: Option<String>,
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
    /// The server sent the request elsewhere. The CLI doesn't follow redirects, so its tokens go only where it was
    /// told to send them.
    #[error("{server} answered with a redirect ({status}) to {location}; set the server to the address it's served at")]
    Redirected {
        server: String,
        status: StatusCode,
        location: String,
    },
    #[error("the server answered {status} with a body the CLI doesn't understand")]
    Unreadable { status: StatusCode, body: String },
}

impl ApiClient {
    /// A client of `server` that sends `time_zone`, an IANA name, with every request, so labels and dates come back
    /// in the machine's timezone. Without one, the server uses UTC.
    pub fn new(server: Url, time_zone: Option<String>) -> Result<Self, ClientError> {
        let builder = reqwest::Client::builder()
            .user_agent(concat!("upheap/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            // A redirect could carry a form with a refresh token or a code verifier to another host.
            .redirect(reqwest::redirect::Policy::none());
        let builder = if uses_proxy(&server) {
            builder
        } else {
            builder.no_proxy()
        };
        let http = builder.build().map_err(ClientError::Setup)?;
        Ok(Self {
            http,
            server,
            time_zone,
            run: None,
        })
    }

    /// This client writing in `run`, a run ID, when there is one: every request then says so, and the server holds it
    /// to what a run may do.
    #[must_use]
    pub fn in_run(mut self, run: Option<String>) -> Self {
        self.run = run;
        self
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
        self.read_json(response).await
    }

    /// `POST /api/v1/<path>` with `body` as JSON, as the holder of `token`.
    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &[&str],
        body: &(impl Serialize + ?Sized),
        token: &str,
    ) -> Result<Fetched<T>, ClientError> {
        let request = self.http.post(self.api_url(path)).json(body).bearer_auth(token);
        let response = self.send(request).await?;
        self.read_json(response).await
    }

    /// `PATCH /api/v1/<path>` with `body` as JSON, for the item or other row at `version`. The server wants the
    /// version in `If-Match`, quoted (`"3"`), and answers `409 version_conflict` if the row has moved on.
    pub async fn patch<T: DeserializeOwned>(
        &self,
        path: &[&str],
        body: &(impl Serialize + ?Sized),
        version: u32,
        token: &str,
    ) -> Result<Fetched<T>, ClientError> {
        let request = self
            .http
            .patch(self.api_url(path))
            .header(reqwest::header::IF_MATCH, format!("\"{version}\""))
            .json(body)
            .bearer_auth(token);
        let response = self.send(request).await?;
        self.read_json(response).await
    }

    /// Exchanges an authorization code or a refresh token at `POST /api/v1/oauth/token`.
    pub async fn request_tokens(&self, form: &TokenRequestForm) -> Result<TokenResponseDto, ClientError> {
        let request = self.http.post(self.api_url(&["oauth", "token"])).form(form);
        let response = self.send(request).await?;
        if !response.status().is_success() {
            return Err(self.read_failure(response).await);
        }
        self.read_json(response).await.map(|fetched| fetched.value)
    }

    /// Revokes a token, and the sign-in it belongs to, at `POST /api/v1/oauth/revoke`.
    pub async fn revoke(&self, form: &RevokeRequestForm) -> Result<(), ClientError> {
        let request = self.http.post(self.api_url(&["oauth", "revoke"])).form(form);
        let response = self.send(request).await?;
        if !response.status().is_success() {
            return Err(self.read_failure(response).await);
        }
        Ok(())
    }

    async fn send(&self, request: RequestBuilder) -> Result<Response, ClientError> {
        let request = match &self.time_zone {
            Some(zone) => request.header("Time-Zone", zone),
            None => request,
        };
        let request = match &self.run {
            Some(run) => request.header("Upheap-Run", run),
            None => request,
        };
        let response = request.send().await.map_err(|source| ClientError::Unreachable {
            server: self.server.to_string(),
            source,
        })?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("nowhere")
                .to_owned();
            return Err(ClientError::Redirected {
                server: self.server.to_string(),
                status: response.status(),
                location,
            });
        }
        Ok(response)
    }

    /// The body of a successful response, read as `T`. A failure is read as a problem.
    async fn read_json<T: DeserializeOwned>(&self, response: Response) -> Result<Fetched<T>, ClientError> {
        let status = response.status();
        if !status.is_success() {
            return Err(self.read_failure(response).await);
        }
        let body = self.read_body(response).await?;
        match serde_json::from_str(&body) {
            Ok(value) => Ok(Fetched { value, body }),
            Err(_) => Err(ClientError::Unreadable { status, body }),
        }
    }

    /// The error a failed response stands for: an OAuth refusal, a problem, or neither.
    async fn read_failure(&self, response: Response) -> ClientError {
        let status = response.status();
        let body = match self.read_body(response).await {
            Ok(body) => body,
            Err(err) => return err,
        };
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

    /// The body as text. A connection that breaks or times out halfway is the server being out of reach, like one
    /// that never connects.
    async fn read_body(&self, response: Response) -> Result<String, ClientError> {
        response.text().await.map_err(|source| ClientError::Unreachable {
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

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_server_on_this_machine_is_reached_without_a_proxy() -> googletest::Result<()> {
        for local in [
            "http://127.0.0.1:8080",
            "http://localhost:8080",
            "http://[::1]:8080",
            "https://localhost",
        ] {
            expect_that!(uses_proxy(&Url::parse(local)?), eq(false), "{local}");
        }
        expect_that!(uses_proxy(&Url::parse("https://upheap.example.test")?), eq(true));
        Ok(())
    }
}
