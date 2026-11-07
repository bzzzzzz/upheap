//! The local build: the backend compiled to WebAssembly, running in a browser worker on SQLite-wasm.
//!
//! The worker (`web/src/local/worker.ts`) opens the database, installs `globalThis.upheapSqlite` (`query` and
//! `execute`, see `web/src/local/sqlite.ts`), then calls [`Backend::open`] once and [`Backend::handle`] for each
//! request, one at a time. `service` and `api` are built without accounts, so every request is the one local user.
//!
//! Everything here is wasm-only, so a native `cargo build --workspace` builds an empty crate.
#![cfg(target_arch = "wasm32")]

mod clock;
mod proxy;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, Request, StatusCode, header};
use sea_orm::{Database, DbBackend};
use service::{Services, Settings};
use tower::ServiceExt;
use wasm_bindgen::prelude::*;

/// The largest response body read back from the Router.
const MAX_RESPONSE_BYTES: usize = 64 << 20;

/// The backend: the API Router over the local database, as the local user.
#[wasm_bindgen]
pub struct Backend {
    router: axum::Router,
}

/// A response for the worker to turn into a `Response`.
#[wasm_bindgen(getter_with_clone)]
pub struct HttpResponse {
    pub status: u16,
    /// Names and values, alternating.
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

#[wasm_bindgen]
impl Backend {
    /// Opens the database through `globalThis.upheapSqlite`, runs the migrations, and opens the local user, whom
    /// the first start makes with their team and first heap.
    pub async fn open() -> Result<Backend, JsError> {
        std::panic::set_hook(Box::new(|info| proxy::report(&format!("panic: {info}"))));
        let proxy: Arc<Box<dyn sea_orm::ProxyDatabaseTrait>> = Arc::new(Box::new(proxy::SqliteWasm::new()));
        let db = Database::connect_proxy(DbBackend::Sqlite, proxy).await.map_err(to_js)?;
        store::run_migrations(&db).await.map_err(to_js)?;
        let services = Services::new(db, Arc::new(clock::BrowserClock), Settings::default());
        let local_user = services.open_local_user().await.map_err(to_js)?;
        let router = api::router(api::AppState {
            services,
            authentication: api::Authentication::LocalUser(local_user),
        });
        Ok(Backend { router })
    }

    /// Sends one request through the Router. `uri` is the path and query; `headers` alternates names and values.
    ///
    /// Call it for one request at a time: a request's transaction must end before the next one starts. A commit
    /// that failed turns the response into a `500`.
    pub async fn handle(
        &self,
        method: String,
        uri: String,
        headers: Vec<String>,
        body: Vec<u8>,
    ) -> Result<HttpResponse, JsError> {
        let mut request = Request::builder()
            .method(Method::from_bytes(method.as_bytes()).map_err(to_js)?)
            .uri(uri)
            .body(Body::from(body))
            .map_err(to_js)?;
        for pair in headers.chunks_exact(2) {
            let name = HeaderName::from_bytes(pair[0].as_bytes()).map_err(to_js)?;
            let value = HeaderValue::from_str(&pair[1]).map_err(to_js)?;
            request.headers_mut().append(name, value);
        }

        let response = self.router.clone().oneshot(request).await.map_err(to_js)?;
        if let Some(err) = proxy::take_commit_error() {
            return Ok(commit_failed(&err));
        }
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some([name.as_str().to_owned(), value.to_str().ok()?.to_owned()]))
            .flatten()
            .collect();
        let body = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(to_js)?
            .to_vec();
        Ok(HttpResponse { status, headers, body })
    }
}

/// The `500` problem for a request whose commit failed, as the server answers unexpected errors. The proxy rolled
/// the transaction back, so none of the request was kept, whatever the handler answered.
fn commit_failed(err: &str) -> HttpResponse {
    proxy::report(&format!("commit failed: {err}"));
    let status = StatusCode::INTERNAL_SERVER_ERROR.as_u16();
    let body =
        serde_json::json!({ "code": "internal", "title": "Something went wrong on the server", "status": status });
    HttpResponse {
        status,
        headers: vec![
            header::CONTENT_TYPE.as_str().to_owned(),
            "application/problem+json".to_owned(),
        ],
        body: body.to_string().into_bytes(),
    }
}

/// The error and every source under it, since a store error's own message says only which layer failed.
fn to_js(err: impl std::error::Error) -> JsError {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    JsError::new(&message)
}
