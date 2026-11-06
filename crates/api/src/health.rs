//! Liveness and readiness probes. Neither needs a session, and neither reveals versions or configuration.

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

use crate::AppState;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

/// `200` while the process is up.
async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// `200` when the database answers. The router is only served after migrations ran, so that's implied.
async fn readyz(State(state): State<AppState>) -> StatusCode {
    match state.services.check_database().await {
        Ok(()) => StatusCode::OK,
        Err(err) => {
            tracing::warn!(error = %err, "readiness check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
