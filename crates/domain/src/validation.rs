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
    /// A reminder is added without a date, or its date is cleared.
    #[error("a reminder needs a date")]
    ReminderNeedsDate,
    #[error("timestamp is out of range")]
    TimestampOutOfRange,
    #[error("at most {max} tags are allowed")]
    TooManyTags { max: usize },
    #[error("at most {max} options are allowed")]
    TooManyOptions { max: usize },
    #[error("weight must be from 0 to {max}")]
    WeightOutOfRange { max: u16 },
    #[error("priority must be from {min} to {max}")]
    PriorityOutOfRange { min: u8, max: u8 },
    /// A weights save or preview names a parameter more than once.
    #[error("each parameter may be listed once")]
    DuplicateParameter,
    #[error("limit must be from {min} to {max}")]
    LimitOutOfRange { min: usize, max: usize },
    /// Not a valid username or slug. `hint` is one made from the input that would be, when there is one.
    #[error("{field} must be lowercase letters, digits and single dashes, starting with a letter")]
    HandleInvalid { field: Field, hint: Option<String> },
    #[error("item key is not valid")]
    ItemKeyInvalid,
    /// A list sorted by a custom parameter names it in `sort_parameter`, and no other sort does.
    #[error("send sort_parameter exactly when sort is parameter")]
    SortParameterMismatch,
    /// A text that must be one line has a line break.
    #[error("{field} must be a single line")]
    MultipleLines { field: Field },
    /// A question with choices has too few or too many.
    #[error("a question has from {min} to {max} choices")]
    ChoiceCount { min: usize, max: usize },
    /// Two choices of a question are the same, ignoring case.
    #[error("each choice may be listed once, whatever its case")]
    ChoiceDuplicate,
    /// A field is sent for something that can't have it, such as `many` on a question without choices.
    #[error("{field} is not allowed here")]
    NotAllowed { field: Field },
    /// A reply picks a choice the question doesn't have.
    #[error("a pick is not one of the question's choices")]
    PickOutOfRange,
    /// A reply picks the same choice twice.
    #[error("each choice may be picked once")]
    PickRepeated,
    /// A reply picks nothing where a question with choices needs a pick.
    #[error("pick at least one choice")]
    PickNone,
    /// A reply picks more than one where the question takes one.
    #[error("this question takes one pick")]
    PickTooMany,
    /// A field is not valid and no more specific rule says why, such as a link that is not a web address.
    #[error("{field} is not valid")]
    FieldInvalid { field: Field },
    /// A field names a value the server doesn't know, such as an enum value from a newer client.
    #[error("{field} is not a value this server knows")]
    ValueUnknown { field: Field },
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
    /// The password a signed-in user gives to prove it is them, when they change it.
    CurrentPassword,
    /// An item's kind.
    Kind,
    /// An item's stage.
    Stage,
    /// A step's context.
    Context,
    /// A step's state.
    State,
    /// The step a new step goes after.
    After,
    /// A comment's text.
    Body,
    /// A question's short text.
    Text,
    /// A question's long version.
    Details,
    /// A question's choices.
    Choices,
    /// Whether a question takes any number of picks.
    Many,
    /// The step of the item a question is about.
    Step,
    /// The choices a reply picks.
    Picked,
    /// A reply's comment.
    Comment,
    /// The reply in a question's edit.
    Reply,
    /// The line the person sends an agent with.
    Line,
    /// How far a turn goes.
    Reach,
    /// Where a turn goes to.
    Goal,
    /// The heaps an agent asks the queue about.
    Heaps,
    /// What a run says it reached.
    Result,
    /// Why a run handed the turn back.
    Reason,
    /// The links a run reached with.
    Links,
    /// What a run cost, in US dollars.
    CostUsd,
    /// The link a review asks about.
    Link,
    /// The piece of work a review is about.
    Piece,
    /// The item a new item is made from.
    Origin,
    /// Why an item is blocked, in words.
    BlockedReason,
    /// When a run's agent tool exited.
    ToolEndedAt,
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
            Self::CurrentPassword => "current_password",
            Self::Kind => "kind",
            Self::Stage => "stage",
            Self::Context => "context",
            Self::State => "state",
            Self::After => "after",
            Self::Body => "body",
            Self::Text => "text",
            Self::Details => "details",
            Self::Choices => "choices",
            Self::Many => "many",
            Self::Step => "step",
            Self::Picked => "picked",
            Self::Comment => "comment",
            Self::Reply => "reply",
            Self::Line => "line",
            Self::Reach => "reach",
            Self::Goal => "goal",
            Self::Heaps => "heaps",
            Self::Result => "result",
            Self::Reason => "reason",
            Self::Links => "links",
            Self::CostUsd => "cost_usd",
            Self::Link => "link",
            Self::Piece => "piece",
            Self::Origin => "origin",
            Self::BlockedReason => "blocked_reason",
            Self::ToolEndedAt => "tool_ended_at",
        }
    }
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
