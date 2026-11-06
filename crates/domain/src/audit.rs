//! Audit actions. Stored as stable strings; the application only ever inserts audit entries.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditAction {
    UserRegistered,
    TeamCreated,
    MemberAdded,
    HeapCreated,
    ItemCreated,
    ItemUpdated,
}

impl AuditAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserRegistered => "user_registered",
            Self::TeamCreated => "team_created",
            Self::MemberAdded => "member_added",
            Self::HeapCreated => "heap_created",
            Self::ItemCreated => "item_created",
            Self::ItemUpdated => "item_updated",
        }
    }
}

/// The kind of entity an audit entry is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityType {
    User,
    Team,
    Heap,
    Item,
}

impl EntityType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Heap => "heap",
            Self::Item => "item",
        }
    }
}
