//! The one place where errors become HTTP responses.
//!
//! Every [`ServiceError`] variant is matched exhaustively, so a new variant doesn't compile until it has a
//! status and a code. Clients branch on `code`, never on `title` or `detail`.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::ValidationError;
use domain::item::ItemError;
use domain::parameter::ParameterError;
use domain::team::TeamError;
use serde::Serialize;
use service::ServiceError;
use store_error::StoreErrorKind;
use utoipa::ToSchema;

use crate::request_id::current_request_id;

/// An error response body (RFC 9457 problem details).
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// Stable machine-readable error code, for example `version_conflict`.
    pub code: &'static str,
    /// Short human-readable summary. May change; don't match on it.
    pub title: String,
    pub status: u16,
    /// The input field the error is about, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
    /// A value of `field` that would be accepted, when the server can make one: the handle `invalid_handle` hints
    /// at, such as `my-team` for `My-Team`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Present on unexpected errors, so the operator can find the details in the log.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Why a request failed.
#[derive(Debug)]
pub enum ApiError {
    Service(ServiceError),
    /// The request was malformed: unparsable JSON, query or path.
    BadRequest {
        code: &'static str,
        detail: String,
    },
    /// A write needs an `If-Match` header with the expected version.
    VersionRequired,
    /// A state-changing request came without the CSRF header.
    CsrfHeaderMissing,
    NoSuchRoute,
    /// A service error caused by one query parameter or body field, which the problem names as its `field`.
    InField {
        error: ServiceError,
        field: &'static str,
    },
}

impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        Self::Service(err)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest {
            code: "invalid_body",
            detail: rejection.body_text(),
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest {
            code: "invalid_query",
            detail: rejection.body_text(),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(_: PathRejection) -> Self {
        // A malformed ID can't name anything that exists.
        Self::NoSuchRoute
    }
}

/// Error responses fall into these categories, and each category has a log level.
enum Category {
    Caller,
    Permission,
    Conflict,
    Dependency,
    Unexpected,
}

struct Classified {
    status: StatusCode,
    code: &'static str,
    title: String,
    field: Option<&'static str>,
    hint: Option<String>,
    category: Category,
}

fn classify(err: &ApiError) -> Classified {
    let simple = |status, code, title: &str, category| Classified {
        status,
        code,
        title: title.to_owned(),
        field: None,
        hint: None,
        category,
    };
    match err {
        ApiError::BadRequest { code, detail } => simple(StatusCode::BAD_REQUEST, code, detail, Category::Caller),
        ApiError::InField { error, field } => Classified {
            field: Some(field),
            ..classify_service(error)
        },
        ApiError::VersionRequired => simple(
            StatusCode::PRECONDITION_REQUIRED,
            "version_required",
            "Send the expected version in an If-Match header",
            Category::Caller,
        ),
        ApiError::CsrfHeaderMissing => simple(
            StatusCode::FORBIDDEN,
            "csrf_header_missing",
            "State-changing requests need the X-CSRF-Protection header",
            Category::Caller,
        ),
        ApiError::NoSuchRoute => simple(StatusCode::NOT_FOUND, "not_found", "Not found", Category::Permission),
        ApiError::Service(err) => classify_service(err),
    }
}

fn classify_service(err: &ServiceError) -> Classified {
    let simple = |status, code, category| Classified {
        status,
        code,
        title: err.to_string(),
        field: None,
        hint: None,
        category,
    };
    match err {
        ServiceError::Invalid(invalid) => classify_validation(invalid),
        ServiceError::NotFound => simple(StatusCode::NOT_FOUND, "not_found", Category::Permission),
        ServiceError::Forbidden(_) => simple(StatusCode::FORBIDDEN, "forbidden", Category::Permission),
        ServiceError::Unauthenticated => simple(StatusCode::UNAUTHORIZED, "unauthenticated", Category::Permission),
        ServiceError::InvalidCredentials => {
            simple(StatusCode::UNAUTHORIZED, "invalid_credentials", Category::Permission)
        }
        ServiceError::TooManySignInAttempts { .. } => {
            simple(StatusCode::TOO_MANY_REQUESTS, "too_many_attempts", Category::Permission)
        }
        ServiceError::EmailTaken => Classified {
            field: Some("email"),
            ..simple(StatusCode::CONFLICT, "email_taken", Category::Conflict)
        },
        ServiceError::HandleTaken { field } => Classified {
            field: Some(field.as_str()),
            ..simple(StatusCode::CONFLICT, "handle_taken", Category::Conflict)
        },
        ServiceError::RegistrationClosed => simple(StatusCode::FORBIDDEN, "registration_closed", Category::Permission),
        ServiceError::HeapArchived { .. } => simple(StatusCode::CONFLICT, "heap_archived", Category::Conflict),
        ServiceError::VersionConflict { .. } => simple(StatusCode::CONFLICT, "version_conflict", Category::Conflict),
        ServiceError::Item(ItemError::Closed { .. }) => simple(StatusCode::CONFLICT, "item_closed", Category::Conflict),
        ServiceError::Item(ItemError::ReopenWindowExpired { .. }) => {
            simple(StatusCode::CONFLICT, "reopen_window_expired", Category::Conflict)
        }
        ServiceError::Item(ItemError::NotBlocked { .. }) => Classified {
            field: Some("blocked_by_item_id"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "item_not_blocked", Category::Caller)
        },
        ServiceError::BlockerNotInHeap { .. } => Classified {
            field: Some("blocked_by_item_id"),
            ..simple(
                StatusCode::UNPROCESSABLE_ENTITY,
                "blocker_not_in_heap",
                Category::Caller,
            )
        },
        ServiceError::BlockerClosed { .. } => Classified {
            field: Some("blocked_by_item_id"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "blocker_closed", Category::Caller)
        },
        ServiceError::BlockingCycle { .. } => Classified {
            field: Some("blocked_by_item_id"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "blocking_cycle", Category::Caller)
        },
        ServiceError::TagNotInHeap { .. } => Classified {
            field: Some("tag_ids"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "tag_not_in_heap", Category::Caller)
        },
        ServiceError::Team(TeamError::LastAdmin { .. }) => {
            simple(StatusCode::CONFLICT, "last_admin", Category::Conflict)
        }
        ServiceError::Team(TeamError::PersonalTeam { .. }) => {
            simple(StatusCode::CONFLICT, "personal_team", Category::Conflict)
        }
        ServiceError::UserNotFound { .. } => Classified {
            field: Some("user_id"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "user_not_found", Category::Caller)
        },
        ServiceError::SavedViewArchived { .. } => {
            simple(StatusCode::CONFLICT, "saved_view_archived", Category::Conflict)
        }
        ServiceError::TagNameTaken { .. } => Classified {
            field: Some("name"),
            ..simple(StatusCode::CONFLICT, "tag_name_taken", Category::Conflict)
        },
        ServiceError::Parameter(error) => classify_parameter(error, simple),
        ServiceError::ParameterNameTaken { .. } => Classified {
            field: Some("name"),
            ..simple(StatusCode::CONFLICT, "parameter_name_taken", Category::Conflict)
        },
        ServiceError::ParameterInUse { .. } => simple(StatusCode::CONFLICT, "parameter_in_use", Category::Conflict),
        ServiceError::OptionNotInHeap { .. } => Classified {
            field: Some("param"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "option_not_in_heap", Category::Caller)
        },
        ServiceError::PasswordHashing(_) | ServiceError::RandomSource(_) => unexpected(),
        ServiceError::Store(store) => match StoreErrorKind::of(store) {
            StoreErrorKind::Unavailable => Classified {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "unavailable",
                title: "The database is unavailable; try again shortly".to_owned(),
                field: None,
                hint: None,
                category: Category::Dependency,
            },
            StoreErrorKind::Other => unexpected(),
        },
    }
}

fn classify_parameter(
    err: &ParameterError,
    simple: impl Fn(StatusCode, &'static str, Category) -> Classified,
) -> Classified {
    let (status, code, field, category) = match err {
        ParameterError::Archived { .. } => (StatusCode::CONFLICT, "parameter_archived", None, Category::Conflict),
        ParameterError::TooManyOptions { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_options",
            Some("options"),
            Category::Caller,
        ),
        ParameterError::OptionLabelTaken { .. } => (
            StatusCode::CONFLICT,
            "option_label_taken",
            Some("options"),
            Category::Conflict,
        ),
        ParameterError::OptionsMismatch { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "options_mismatch",
            Some("options"),
            Category::Caller,
        ),
        ParameterError::ReplacementRequired { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "replacement_required",
            Some("replacement_option_id"),
            Category::Caller,
        ),
        ParameterError::NotInHeap { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "parameter_not_in_heap",
            Some("parameter_values"),
            Category::Caller,
        ),
        ParameterError::OptionNotInParameter { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "option_not_in_parameter",
            Some("parameter_values"),
            Category::Caller,
        ),
    };
    Classified {
        field,
        ..simple(status, code, category)
    }
}

fn classify_validation(err: &ValidationError) -> Classified {
    let hint = match err {
        ValidationError::HandleInvalid { hint, .. } => hint.clone(),
        _ => None,
    };
    let (code, field) = match err {
        ValidationError::Empty { field } => ("field_empty", Some(field.as_str())),
        ValidationError::TooLong { field, .. } => ("field_too_long", Some(field.as_str())),
        ValidationError::TooShort { field, .. } => ("field_too_short", Some(field.as_str())),
        ValidationError::EmailInvalid => ("email_invalid", Some("email")),
        ValidationError::PasswordTooShort { .. } => ("password_too_short", Some("password")),
        ValidationError::PasswordTooLong { .. } => ("password_too_long", Some("password")),
        ValidationError::DueDateInvalid => ("due_date_invalid", Some("due")),
        ValidationError::TimestampOutOfRange => ("timestamp_out_of_range", None),
        ValidationError::TooManyTags { .. } => ("too_many_tags", Some("tag_ids")),
        ValidationError::TooManyOptions { .. } => ("too_many_options", Some("param")),
        ValidationError::WeightOutOfRange { .. } => ("weight_out_of_range", Some("weight")),
        ValidationError::SortParameterMismatch => ("sort_parameter_mismatch", Some("sort_parameter")),
        ValidationError::HandleInvalid { field, .. } => ("invalid_handle", Some(field.as_str())),
        ValidationError::ItemKeyInvalid => ("invalid_item_key", None),
    };
    Classified {
        status: StatusCode::UNPROCESSABLE_ENTITY,
        code,
        title: err.to_string(),
        field,
        hint,
        category: Category::Caller,
    }
}

fn unexpected() -> Classified {
    Classified {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal",
        title: "Something went wrong on the server".to_owned(),
        field: None,
        hint: None,
        category: Category::Unexpected,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let classified = classify(&self);
        let request_id = match classified.category {
            Category::Unexpected => {
                tracing::error!(error = ?self, "request failed unexpectedly");
                current_request_id()
            }
            Category::Dependency => {
                tracing::warn!(error = ?self, "dependency unavailable");
                None
            }
            Category::Caller | Category::Permission | Category::Conflict => {
                tracing::debug!(code = classified.code, "request rejected");
                None
            }
        };
        let body = Problem {
            code: classified.code,
            title: classified.title,
            status: classified.status.as_u16(),
            field: classified.field,
            hint: classified.hint,
            request_id,
        };
        let mut response = (
            classified.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(body),
        )
            .into_response();
        if let ApiError::Service(ServiceError::TooManySignInAttempts { retry_after }) = &self {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, retry_after.as_seconds_ceil().max(1).into());
        }
        response
    }
}

/// Fallback for unknown routes under `/api/v1`.
pub(crate) async fn not_found() -> ApiError {
    ApiError::NoSuchRoute
}

mod store_error {
    //! `api` doesn't depend on `store`; the service re-exports what is needed to classify storage failures.

    pub(super) enum StoreErrorKind {
        Unavailable,
        Other,
    }

    impl StoreErrorKind {
        pub(super) fn of(err: &service::StoreError) -> Self {
            match err {
                service::StoreError::Unavailable(_) => Self::Unavailable,
                service::StoreError::Duplicate(_)
                | service::StoreError::Database(_)
                | service::StoreError::CorruptRow { .. } => Self::Other,
            }
        }
    }
}
