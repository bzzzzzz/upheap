//! Makes the request ID available to error responses, which only see the error.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

tokio::task_local! {
    static REQUEST_ID: String;
}

/// The current request's ID, if called while handling a request.
pub(crate) fn current_request_id() -> Option<String> {
    REQUEST_ID.try_with(Clone::clone).ok().filter(|id| !id.is_empty())
}

/// Runs the rest of the stack with the `x-request-id` header value in scope.
pub(crate) async fn scope_request_id(request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    REQUEST_ID.scope(id, next.run(request)).await
}
