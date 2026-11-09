use domain::item::ItemError;
use domain::oauth::{AuthorizationError, GrantError};
use domain::parameter::ParameterError;
use domain::permission::Permission;
use domain::team::TeamError;
use domain::validation::Field;
use domain::{Duration, HeapId, ItemId, OptionId, ParameterId, SavedViewId, TagId, UserId, ValidationError, Version};
use store::StoreError;

/// Why a use case failed. `api` maps each variant to an HTTP status and a stable code.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("invalid input")]
    Invalid(#[from] ValidationError),
    /// The resource doesn't exist, or the caller isn't a member of the team that owns it.
    #[error("not found")]
    NotFound,
    #[error("missing permission {0:?}")]
    Forbidden(Permission),
    #[error("not signed in")]
    Unauthenticated,
    #[error("email or password is wrong")]
    InvalidCredentials,
    /// The email address used up its sign-in attempts for now.
    #[error("too many sign-in attempts; try again in {}", in_minutes(*retry_after))]
    TooManySignInAttempts { retry_after: Duration },
    #[error("an account with this email already exists")]
    EmailTaken,
    /// Another user or team has the username or team slug, or another heap of the team has the heap slug.
    #[error("this {field} is taken")]
    HandleTaken { field: Field },
    /// An OAuth authorization request that can't be approved, found before anything was stored.
    #[error(transparent)]
    Authorization(#[from] AuthorizationError),
    /// A token request whose code or refresh token can't be exchanged.
    #[error(transparent)]
    InvalidGrant(#[from] GrantError),
    #[error("registration is disabled on this server")]
    RegistrationClosed,
    #[error("heap {heap_id} is archived")]
    HeapArchived { heap_id: HeapId },
    #[error("this changed since you loaded it (version {} expected, {} current)", expected.get(), current.get())]
    VersionConflict { expected: Version, current: Version },
    #[error(transparent)]
    Item(#[from] ItemError),
    /// A tag ID in the request doesn't name a tag of the item's heap.
    #[error("tag {tag_id} is not a tag of this heap")]
    TagNotInHeap { tag_id: TagId },
    /// The blocker in the request isn't an item of the blocked item's heap.
    #[error("item {item_id} is not an item of this heap")]
    BlockerNotInHeap { item_id: ItemId },
    #[error("item {item_id} is closed, so nothing can wait for it")]
    BlockerClosed { item_id: ItemId },
    /// The blocker already waits, directly or through others, for the item being blocked.
    #[error("item {item_id} already waits for this item, so it can't block it")]
    BlockingCycle { item_id: ItemId },
    #[error(transparent)]
    Team(#[from] TeamError),
    /// The user to add to a team doesn't exist.
    #[error("there is no user {user_id}")]
    UserNotFound { user_id: UserId },
    /// An archived saved view can't be changed.
    #[error("saved view {view_id} is archived")]
    SavedViewArchived { view_id: SavedViewId },
    #[error("this heap already has a tag called {name:?}")]
    TagNameTaken { name: String },
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    #[error("this heap already has a parameter called {name:?}")]
    ParameterNameTaken { name: String },
    /// A parameter is archived only when no open item has a value for it.
    #[error("{items} open items still have a value for parameter {parameter_id}; clear them first")]
    ParameterInUse { parameter_id: ParameterId, items: usize },
    /// A list filter names an option that isn't an option of the heap's parameters.
    #[error("option {option_id} is not an option of this heap's parameters")]
    OptionNotInHeap { option_id: OptionId },
    /// The random source failed, so no item key could be made.
    #[error("random source failed: {0}")]
    RandomSource(String),
    #[error("password hashing failed: {0}")]
    PasswordHashing(String),
    #[error("storage failed")]
    Store(#[from] StoreError),
}

/// A duplicate-key failure on a write that sets a handle means another row has it.
pub(crate) fn handle_taken_on_duplicate(err: StoreError, field: Field) -> ServiceError {
    match err {
        StoreError::Duplicate(_) => ServiceError::HandleTaken { field },
        other => ServiceError::Store(other),
    }
}

/// "1 minute", "12 minutes": a wait in whole minutes, rounded up.
fn in_minutes(wait: Duration) -> String {
    match wait.as_minutes_ceil() {
        1 => "1 minute".to_owned(),
        minutes => format!("{minutes} minutes"),
    }
}
