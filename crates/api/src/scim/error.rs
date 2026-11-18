//! Refusals in SCIM's error format (RFC 7644 §3.12), which providers read: not `application/problem+json`.

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use service::scim::ScimError;

pub(crate) struct ScimFailure(pub ScimError);

impl From<ScimError> for ScimFailure {
    fn from(error: ScimError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ScimFailure {
    fn into_response(self) -> Response {
        let (status, scim_type, detail) = match &self.0 {
            ScimError::Unauthorized => (StatusCode::UNAUTHORIZED, None, self.0.to_string()),
            ScimError::NotFound => (StatusCode::NOT_FOUND, None, self.0.to_string()),
            ScimError::Uniqueness(_) => (StatusCode::CONFLICT, Some("uniqueness"), self.0.to_string()),
            ScimError::InvalidFilter => (StatusCode::BAD_REQUEST, Some("invalidFilter"), self.0.to_string()),
            ScimError::InvalidValue(_) => (StatusCode::BAD_REQUEST, Some("invalidValue"), self.0.to_string()),
            ScimError::InvalidSyntax(_) => (StatusCode::BAD_REQUEST, Some("invalidSyntax"), self.0.to_string()),
            ScimError::NoTarget(_) => (StatusCode::BAD_REQUEST, Some("noTarget"), self.0.to_string()),
            ScimError::Service(error) => {
                tracing::error!(?error, "SCIM request failed unexpectedly");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    None,
                    "Something went wrong on the server".to_owned(),
                )
            }
        };
        if status.is_client_error() {
            tracing::warn!(status = status.as_u16(), scim_type, "SCIM request refused");
        }
        let mut body = json!({
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"],
            "status": status.as_u16().to_string(),
            "detail": detail,
        });
        if let Some(scim_type) = scim_type {
            body["scimType"] = json!(scim_type);
        }
        let mut response = (status, Json(body)).into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(super::CONTENT_TYPE));
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}
