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
    MemberJoined,
    HeapCreated,
    HeapUpdated,
    HeapContextUpdated,
    HeapArchived,
    HeapUnarchived,
    ItemCreated,
    ItemUpdated,
    ItemMoved,
    StepAdded,
    StepUpdated,
    QuestionAsked,
    QuestionAnswered,
    QuestionWithdrawn,
    CommentAdded,
    TurnOpened,
    TurnTakenBack,
    TurnReached,
    TurnRetried,
    TagCreated,
    TagUpdated,
    SavedViewCreated,
    SavedViewUpdated,
    SavedViewArchived,
    ParameterCreated,
    ParameterUpdated,
    ParameterArchived,
    OptionArchived,
    InviteLinkCreated,
    InviteLinkRevoked,
    PasswordChanged,
    PasswordResetLinkMade,
    PasswordReset,
}

impl AuditAction {
    /// Every action, in the order they are declared.
    pub const ALL: [Self; 41] = [
        Self::UserRegistered,
        Self::UserUpdated,
        Self::TeamCreated,
        Self::TeamUpdated,
        Self::MemberAdded,
        Self::MemberRoleChanged,
        Self::MemberRemoved,
        Self::MemberLeft,
        Self::MemberJoined,
        Self::HeapCreated,
        Self::HeapUpdated,
        Self::HeapContextUpdated,
        Self::HeapArchived,
        Self::HeapUnarchived,
        Self::ItemCreated,
        Self::ItemUpdated,
        Self::ItemMoved,
        Self::StepAdded,
        Self::StepUpdated,
        Self::QuestionAsked,
        Self::QuestionAnswered,
        Self::QuestionWithdrawn,
        Self::CommentAdded,
        Self::TurnOpened,
        Self::TurnTakenBack,
        Self::TurnReached,
        Self::TurnRetried,
        Self::TagCreated,
        Self::TagUpdated,
        Self::SavedViewCreated,
        Self::SavedViewUpdated,
        Self::SavedViewArchived,
        Self::ParameterCreated,
        Self::ParameterUpdated,
        Self::ParameterArchived,
        Self::OptionArchived,
        Self::InviteLinkCreated,
        Self::InviteLinkRevoked,
        Self::PasswordChanged,
        Self::PasswordResetLinkMade,
        Self::PasswordReset,
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
            Self::MemberJoined => "member_joined",
            Self::HeapCreated => "heap_created",
            Self::HeapUpdated => "heap_updated",
            Self::HeapContextUpdated => "heap_context_updated",
            Self::HeapArchived => "heap_archived",
            Self::HeapUnarchived => "heap_unarchived",
            Self::ItemCreated => "item_created",
            Self::ItemUpdated => "item_updated",
            Self::ItemMoved => "item_moved",
            Self::StepAdded => "step_added",
            Self::StepUpdated => "step_updated",
            Self::QuestionAsked => "question_asked",
            Self::QuestionAnswered => "question_answered",
            Self::QuestionWithdrawn => "question_withdrawn",
            Self::CommentAdded => "comment_added",
            Self::TurnOpened => "turn_opened",
            Self::TurnTakenBack => "turn_taken_back",
            Self::TurnReached => "turn_reached",
            Self::TurnRetried => "turn_retried",
            Self::TagCreated => "tag_created",
            Self::TagUpdated => "tag_updated",
            Self::SavedViewCreated => "saved_view_created",
            Self::SavedViewUpdated => "saved_view_updated",
            Self::SavedViewArchived => "saved_view_archived",
            Self::ParameterCreated => "parameter_created",
            Self::ParameterUpdated => "parameter_updated",
            Self::ParameterArchived => "parameter_archived",
            Self::OptionArchived => "option_archived",
            Self::InviteLinkCreated => "invite_link_created",
            Self::InviteLinkRevoked => "invite_link_revoked",
            Self::PasswordChanged => "password_changed",
            Self::PasswordResetLinkMade => "password_reset_link_made",
            Self::PasswordReset => "password_reset",
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
    /// A team's invite link.
    InviteLink,
}

impl EntityType {
    /// Every entity type, in the order they are declared.
    pub const ALL: [Self; 8] = [
        Self::User,
        Self::Team,
        Self::Heap,
        Self::Item,
        Self::Tag,
        Self::SavedView,
        Self::Parameter,
        Self::InviteLink,
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
            Self::InviteLink => "invite_link",
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
