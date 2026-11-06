//! Audit actions. Stored as stable strings; the application only ever inserts audit entries.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditAction {
    UserRegistered,
    UserUpdated,
    TeamCreated,
    TeamUpdated,
    MemberAdded,
    MemberRoleChanged,
    MemberRemoved,
    MemberLeft,
    HeapCreated,
    HeapUpdated,
    HeapArchived,
    HeapUnarchived,
    ItemCreated,
    ItemUpdated,
    TagCreated,
    TagUpdated,
}

impl AuditAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserRegistered => "user_registered",
            Self::UserUpdated => "user_updated",
            Self::TeamCreated => "team_created",
            Self::TeamUpdated => "team_updated",
            Self::MemberAdded => "member_added",
            Self::MemberRoleChanged => "member_role_changed",
            Self::MemberRemoved => "member_removed",
            Self::MemberLeft => "member_left",
            Self::HeapCreated => "heap_created",
            Self::HeapUpdated => "heap_updated",
            Self::HeapArchived => "heap_archived",
            Self::HeapUnarchived => "heap_unarchived",
            Self::ItemCreated => "item_created",
            Self::ItemUpdated => "item_updated",
            Self::TagCreated => "tag_created",
            Self::TagUpdated => "tag_updated",
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
    Tag,
}

impl EntityType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Heap => "heap",
            Self::Item => "item",
            Self::Tag => "tag",
        }
    }
}
