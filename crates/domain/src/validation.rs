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
    #[error("at most {max} tags are allowed")]
    TooManyTags { max: usize },
    #[error("at most {max} options are allowed")]
    TooManyOptions { max: usize },
    #[error("weight must be from 0 to {max}")]
    WeightOutOfRange { max: u16 },
    /// Not a valid username or slug. `hint` is one made from the input that would be, when there is one.
    #[error("{field} must be lowercase letters, digits and single dashes, starting with a letter")]
    HandleInvalid { field: Field, hint: Option<String> },
    #[error("item key is not valid")]
    ItemKeyInvalid,
    /// A list sorted by a custom parameter names it in `sort_parameter`, and no other sort does.
    #[error("send sort_parameter exactly when sort is parameter")]
    SortParameterMismatch,
}

/// The input field a [`ValidationError`] is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Title,
    Description,
    DisplayName,
    Name,
    /// An option of a custom parameter.
    Label,
    /// A search text.
    Query,
    Username,
    /// A team's or heap's slug.
    Slug,
    Email,
    /// What sign-in takes: an email address or a username.
    Login,
    Password,
}

impl Field {
    /// The field's name in API payloads.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "description",
            Self::DisplayName => "display_name",
            Self::Name => "name",
            Self::Label => "label",
            Self::Query => "query",
            Self::Username => "username",
            Self::Slug => "slug",
            Self::Email => "email",
            Self::Login => "login",
            Self::Password => "password",
        }
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
