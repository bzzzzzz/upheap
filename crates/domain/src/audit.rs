//! Audit actions. Stored as stable strings; the application only ever inserts audit entries.

use crate::ids::coded_enum;

coded_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum AuditAction {
        UserRegistered = "user_registered",
        UserUpdated = "user_updated",
        TeamCreated = "team_created",
        TeamUpdated = "team_updated",
        MemberAdded = "member_added",
        MemberRoleChanged = "member_role_changed",
        MemberRemoved = "member_removed",
        MemberLeft = "member_left",
        MemberJoined = "member_joined",
        HeapCreated = "heap_created",
        HeapUpdated = "heap_updated",
        HeapContextUpdated = "heap_context_updated",
        HeapArchived = "heap_archived",
        HeapUnarchived = "heap_unarchived",
        HeapPriorityChanged = "heap_priority_changed",
        ItemCreated = "item_created",
        ItemUpdated = "item_updated",
        ItemMoved = "item_moved",
        StepAdded = "step_added",
        StepUpdated = "step_updated",
        QuestionAsked = "question_asked",
        QuestionAnswered = "question_answered",
        ReplyChanged = "reply_changed",
        QuestionWithdrawn = "question_withdrawn",
        CommentAdded = "comment_added",
        TurnOpened = "turn_opened",
        TurnTakenBack = "turn_taken_back",
        TurnReached = "turn_reached",
        TurnHandedBack = "turn_handed_back",
        TurnRetried = "turn_retried",
        TurnRedone = "turn_redone",
        TagCreated = "tag_created",
        TagUpdated = "tag_updated",
        SavedViewCreated = "saved_view_created",
        SavedViewUpdated = "saved_view_updated",
        SavedViewArchived = "saved_view_archived",
        ParameterCreated = "parameter_created",
        ParameterUpdated = "parameter_updated",
        ParameterArchived = "parameter_archived",
        OptionArchived = "option_archived",
        InviteLinkCreated = "invite_link_created",
        InviteLinkRevoked = "invite_link_revoked",
        PasswordChanged = "password_changed",
        PasswordResetLinkMade = "password_reset_link_made",
        PasswordReset = "password_reset",
    }
}

coded_enum! {
    /// The kind of entity an audit entry is about.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum EntityType {
        User = "user",
        Team = "team",
        Heap = "heap",
        Item = "item",
        Tag = "tag",
        SavedView = "saved_view",
        /// A custom parameter, its options and its tag links.
        Parameter = "parameter",
        /// A team's invite link.
        InviteLink = "invite_link",
    }
}
