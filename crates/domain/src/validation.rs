//! Input validation failures. Each variant maps to one stable API error code.

/// Why a user-supplied value was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("{field} must not be empty")]
    Empty { field: Field },
    #[error("{field} is longer than {max} characters")]
    TooLong { field: Field, max: usize },
    #[error("{field} must be at least {min} characters")]
    TooShort { field: Field, min: usize },
    #[error("email address is not valid")]
    EmailInvalid,
    #[error("password must be at least {min} characters")]
    PasswordTooShort { min: usize },
    #[error("password is longer than {max} characters")]
    PasswordTooLong { max: usize },
    #[error("due date is not a valid calendar date")]
    DueDateInvalid,
    #[error("timestamp is out of range")]
    TimestampOutOfRange,
    #[error("an item can have at most {max} tags")]
    TooManyTags { max: usize },
}

/// The input field a [`ValidationError`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Description,
    DisplayName,
    Name,
    /// A search text.
    Query,
}

impl Field {
    /// The field's name in API payloads.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
            Self::DisplayName => "display_name",
            Self::Name => "name",
            Self::Query => "query",
        }
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
