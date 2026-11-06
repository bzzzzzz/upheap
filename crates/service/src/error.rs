use domain::item::ItemError;
use domain::permission::Permission;
use domain::{HeapId, ValidationError, Version};
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
    #[error("an account with this email already exists")]
    EmailTaken,
    #[error("registration is disabled on this server")]
    RegistrationClosed,
    #[error("heap {heap_id} is archived")]
    HeapArchived { heap_id: HeapId },
    #[error("the item changed since you loaded it (version {} expected, {} current)", expected.get(), current.get())]
    VersionConflict { expected: Version, current: Version },
    #[error(transparent)]
    Item(#[from] ItemError),
    #[error("password hashing failed: {0}")]
    PasswordHashing(String),
    #[error("storage failed")]
    Store(#[from] StoreError),
}
