//! The one place where errors become HTTP responses.
//!
//! Every [`ServiceError`] variant is matched exhaustively, so a new variant doesn't compile until it has a
//! status and a code. Clients branch on `code`, never on `title` or `detail`.

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::ValidationError;
use domain::invite_link::InviteLinkError;
use domain::item::ItemError;
use domain::oauth::AuthorizationError;
use domain::parameter::ParameterError;
use domain::question::QuestionError;
use domain::step::StepError;
use domain::team::TeamError;
use domain::validation::Field;
use service::ServiceError;
use store_error::StoreErrorKind;

pub(crate) use api_types::problem::Problem;

use crate::request_id::current_request_id;

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
    /// The `Authorization` header holds no bearer token, or one that is unknown, expired or revoked.
    #[cfg(feature = "accounts")]
    BearerRejected,
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
        #[cfg(feature = "accounts")]
        ApiError::BearerRejected => simple(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "The access token is invalid, expired or revoked",
            Category::Permission,
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
            field: Some(Field::Email.as_str()),
            ..simple(StatusCode::CONFLICT, "email_taken", Category::Conflict)
        },
        ServiceError::HandleTaken { field } => Classified {
            field: Some(field.as_str()),
            ..simple(StatusCode::CONFLICT, "handle_taken", Category::Conflict)
        },
        ServiceError::Authorization(error) => classify_authorization(error, simple),
        // Only the token endpoint refuses grants, in its own format (`oauth::OAuthRefusal`); this is for any other.
        ServiceError::InvalidGrant(_) => simple(StatusCode::BAD_REQUEST, "invalid_grant", Category::Caller),
        ServiceError::RegistrationClosed => simple(StatusCode::FORBIDDEN, "registration_closed", Category::Permission),
        ServiceError::HeapArchived { .. } => simple(StatusCode::CONFLICT, "heap_archived", Category::Conflict),
        ServiceError::VersionConflict { .. } => simple(StatusCode::CONFLICT, "version_conflict", Category::Conflict),
        ServiceError::Item(ItemError::Closed { .. }) => simple(StatusCode::CONFLICT, "item_closed", Category::Conflict),
        ServiceError::Item(ItemError::ReopenWindowExpired { .. }) => {
            simple(StatusCode::CONFLICT, "reopen_window_expired", Category::Conflict)
        }
        ServiceError::Item(ItemError::KindChangeNotAllowed { .. }) => Classified {
            field: Some("kind"),
            ..simple(StatusCode::CONFLICT, "kind_change_not_allowed", Category::Conflict)
        },
        ServiceError::Item(ItemError::StageNeedsTask { .. }) => Classified {
            field: Some("stage"),
            ..simple(StatusCode::CONFLICT, "stage_needs_task", Category::Conflict)
        },
        ServiceError::Step(StepError::NeedsTask { .. }) => {
            simple(StatusCode::CONFLICT, "steps_need_task", Category::Conflict)
        }
        ServiceError::Step(StepError::TooMany { .. }) => {
            simple(StatusCode::CONFLICT, "too_many_steps", Category::Conflict)
        }
        ServiceError::Step(StepError::AfterNotFound { .. }) => Classified {
            field: Some("after"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "step_not_in_item", Category::Caller)
        },
        ServiceError::Question(QuestionError::TooMany { .. }) => {
            simple(StatusCode::CONFLICT, "too_many_questions", Category::Conflict)
        }
        ServiceError::Question(QuestionError::Answered { .. }) => {
            simple(StatusCode::CONFLICT, "question_answered", Category::Conflict)
        }
        ServiceError::Question(QuestionError::NotInItem { .. }) => {
            simple(StatusCode::NOT_FOUND, "question_not_in_item", Category::Caller)
        }
        ServiceError::Question(QuestionError::StepNotFound { .. }) => Classified {
            field: Some("step"),
            ..simple(StatusCode::UNPROCESSABLE_ENTITY, "value_unknown", Category::Caller)
        },
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
        ServiceError::InviteLink(InviteLinkError::Ended { .. }) => {
            simple(StatusCode::CONFLICT, "invite_link_ended", Category::Conflict)
        }
        ServiceError::InviteLink(InviteLinkError::Revoked { .. }) => {
            simple(StatusCode::CONFLICT, "invite_link_revoked", Category::Conflict)
        }
        ServiceError::InviteLink(InviteLinkError::RemovedSinceMade { .. }) => {
            simple(StatusCode::FORBIDDEN, "removed_from_team", Category::Permission)
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

fn classify_authorization(
    err: &AuthorizationError,
    simple: impl Fn(StatusCode, &'static str, Category) -> Classified,
) -> Classified {
    let (code, field) = match err {
        AuthorizationError::ResponseTypeUnsupported => ("response_type_unsupported", "response_type"),
        AuthorizationError::ClientUnknown => ("client_unknown", "client_id"),
        AuthorizationError::RedirectUriInvalid => ("redirect_uri_invalid", "redirect_uri"),
        AuthorizationError::CodeChallengeInvalid => ("code_challenge_invalid", "code_challenge"),
        AuthorizationError::CodeChallengeMethodUnsupported => {
            ("code_challenge_method_unsupported", "code_challenge_method")
        }
        AuthorizationError::StateMissing => ("state_missing", "state"),
    };
    Classified {
        field: Some(field),
        ..simple(StatusCode::UNPROCESSABLE_ENTITY, code, Category::Caller)
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
        ValidationError::EmailInvalid => ("email_invalid", Some(Field::Email.as_str())),
        ValidationError::PasswordTooShort { .. } => ("password_too_short", Some(Field::Password.as_str())),
        ValidationError::PasswordTooLong { .. } => ("password_too_long", Some(Field::Password.as_str())),
        ValidationError::DueDateInvalid => ("due_date_invalid", Some("due")),
        ValidationError::TimestampOutOfRange => ("timestamp_out_of_range", None),
        ValidationError::TooManyTags { .. } => ("too_many_tags", Some("tag_ids")),
        ValidationError::TooManyOptions { .. } => ("too_many_options", Some("param")),
        ValidationError::WeightOutOfRange { .. } => ("weight_out_of_range", Some("weight")),
        ValidationError::DuplicateParameter => ("duplicate_parameter", Some("parameters")),
        ValidationError::LimitOutOfRange { .. } => ("limit_out_of_range", Some("limit")),
        ValidationError::SortParameterMismatch => ("sort_parameter_mismatch", Some("sort_parameter")),
        ValidationError::ValueUnknown { field } => ("value_unknown", Some(field.as_str())),
        ValidationError::HandleInvalid { field, .. } => ("invalid_handle", Some(field.as_str())),
        ValidationError::ItemKeyInvalid => ("invalid_item_key", None),
        ValidationError::MultipleLines { field } => ("field_not_single_line", Some(field.as_str())),
        ValidationError::ChoiceCount { .. } => ("choice_count_invalid", Some(Field::Choices.as_str())),
        ValidationError::ChoiceDuplicate => ("choice_duplicate", Some(Field::Choices.as_str())),
        ValidationError::NotAllowed { field } => ("field_not_allowed", Some(field.as_str())),
        ValidationError::PickOutOfRange => ("pick_out_of_range", Some(Field::Picked.as_str())),
        ValidationError::PickRepeated => ("pick_repeated", Some(Field::Picked.as_str())),
        ValidationError::PickNone => ("pick_required", Some(Field::Picked.as_str())),
        ValidationError::PickTooMany => ("pick_too_many", Some(Field::Picked.as_str())),
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
            code: classified.code.to_owned(),
            title: classified.title,
            status: classified.status.as_u16(),
            field: classified.field.map(str::to_owned),
            hint: classified.hint,
            request_id,
        };
        let mut response = (
            classified.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(body),
        )
            .into_response();
        #[cfg(feature = "accounts")]
        if matches!(self, ApiError::BearerRejected) {
            // RFC 6750 §3: says how to authenticate, and why this token didn't.
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer error=\"invalid_token\""),
            );
        }
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
