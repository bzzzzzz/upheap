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
    SavedViewCreated,
    SavedViewUpdated,
    SavedViewArchived,
    ParameterCreated,
    ParameterUpdated,
    ParameterArchived,
    OptionArchived,
}

impl AuditAction {
    /// Every action, in the order they are declared.
    pub const ALL: [Self; 23] = [
        Self::UserRegistered,
        Self::UserUpdated,
        Self::TeamCreated,
        Self::TeamUpdated,
        Self::MemberAdded,
        Self::MemberRoleChanged,
        Self::MemberRemoved,
        Self::MemberLeft,
        Self::HeapCreated,
        Self::HeapUpdated,
        Self::HeapArchived,
        Self::HeapUnarchived,
        Self::ItemCreated,
        Self::ItemUpdated,
        Self::TagCreated,
        Self::TagUpdated,
        Self::SavedViewCreated,
        Self::SavedViewUpdated,
        Self::SavedViewArchived,
        Self::ParameterCreated,
        Self::ParameterUpdated,
        Self::ParameterArchived,
        Self::OptionArchived,
    ];

    /// The action stored as `code`, or `None` for a string no version of the application wrote.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == code)
    }

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
            Self::SavedViewCreated => "saved_view_created",
            Self::SavedViewUpdated => "saved_view_updated",
            Self::SavedViewArchived => "saved_view_archived",
            Self::ParameterCreated => "parameter_created",
            Self::ParameterUpdated => "parameter_updated",
            Self::ParameterArchived => "parameter_archived",
            Self::OptionArchived => "option_archived",
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
    SavedView,
    /// A custom parameter, its options and its tag links.
    Parameter,
}

impl EntityType {
    /// Every entity type, in the order they are declared.
    pub const ALL: [Self; 7] = [
        Self::User,
        Self::Team,
        Self::Heap,
        Self::Item,
        Self::Tag,
        Self::SavedView,
        Self::Parameter,
    ];

    /// The entity type stored as `code`, or `None` for a string no version of the application wrote.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|entity| entity.as_str() == code)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Heap => "heap",
            Self::Item => "item",
            Self::Tag => "tag",
            Self::SavedView => "saved_view",
            Self::Parameter => "parameter",
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn every_code_reads_back_as_its_value() {
        for action in AuditAction::ALL {
            expect_that!(AuditAction::from_code(action.as_str()), some(eq(action)));
        }
        for entity in EntityType::ALL {
            expect_that!(EntityType::from_code(entity.as_str()), some(eq(entity)));
        }
        expect_that!(AuditAction::from_code("item_deleted"), none());
    }
}
