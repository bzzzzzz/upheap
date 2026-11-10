//! Typed IDs, so arguments of different kinds can't be swapped. Generating new IDs is the caller's job.

use std::fmt;

use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(UserId);
id_type!(TeamId);
id_type!(HeapId);
id_type!(ItemId);
id_type!(StepId);
id_type!(TagId);
id_type!(SavedViewId);
id_type!(InviteLinkId);
id_type!(ParameterId);
id_type!(OptionId);
id_type!(
    /// The OAuth tokens of one sign-in by a client, and every pair refreshed from them (`domain::oauth`).
    TokenFamilyId
);
