//! Reading the audit log: a heap's history and a team's history, or one entity's within them.

use domain::audit::{AuditAction, EntityType};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::common::UserRefDto;

#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQueryDto {
    /// Only the history of this entity: an item, tag, saved view or parameter of the heap, or the heap itself; for
    /// a team, the team, one of its heaps, or a member.
    pub entity_id: Option<Uuid>,
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1 to 200. Defaults to 50.
    pub limit: Option<u64>,
}

/// What was done. Each action is shown to those who may take it: members see item, tag and saved view changes,
/// admins see everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AuditAction)]
pub enum AuditActionDto {
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
    /// An action this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<AuditAction> for AuditActionDto {
    fn from(action: AuditAction) -> Self {
        match action {
            AuditAction::UserRegistered => Self::UserRegistered,
            AuditAction::UserUpdated => Self::UserUpdated,
            AuditAction::TeamCreated => Self::TeamCreated,
            AuditAction::TeamUpdated => Self::TeamUpdated,
            AuditAction::MemberAdded => Self::MemberAdded,
            AuditAction::MemberRoleChanged => Self::MemberRoleChanged,
            AuditAction::MemberRemoved => Self::MemberRemoved,
            AuditAction::MemberLeft => Self::MemberLeft,
            AuditAction::MemberJoined => Self::MemberJoined,
            AuditAction::HeapCreated => Self::HeapCreated,
            AuditAction::HeapUpdated => Self::HeapUpdated,
            AuditAction::HeapContextUpdated => Self::HeapContextUpdated,
            AuditAction::HeapArchived => Self::HeapArchived,
            AuditAction::HeapUnarchived => Self::HeapUnarchived,
            AuditAction::ItemCreated => Self::ItemCreated,
            AuditAction::ItemUpdated => Self::ItemUpdated,
            AuditAction::ItemMoved => Self::ItemMoved,
            AuditAction::StepAdded => Self::StepAdded,
            AuditAction::StepUpdated => Self::StepUpdated,
            AuditAction::QuestionAsked => Self::QuestionAsked,
            AuditAction::QuestionAnswered => Self::QuestionAnswered,
            AuditAction::QuestionWithdrawn => Self::QuestionWithdrawn,
            AuditAction::CommentAdded => Self::CommentAdded,
            AuditAction::TurnOpened => Self::TurnOpened,
            AuditAction::TurnTakenBack => Self::TurnTakenBack,
            AuditAction::TurnReached => Self::TurnReached,
            AuditAction::TurnRetried => Self::TurnRetried,
            AuditAction::TagCreated => Self::TagCreated,
            AuditAction::TagUpdated => Self::TagUpdated,
            AuditAction::SavedViewCreated => Self::SavedViewCreated,
            AuditAction::SavedViewUpdated => Self::SavedViewUpdated,
            AuditAction::SavedViewArchived => Self::SavedViewArchived,
            AuditAction::ParameterCreated => Self::ParameterCreated,
            AuditAction::ParameterUpdated => Self::ParameterUpdated,
            AuditAction::ParameterArchived => Self::ParameterArchived,
            AuditAction::OptionArchived => Self::OptionArchived,
            AuditAction::InviteLinkCreated => Self::InviteLinkCreated,
            AuditAction::InviteLinkRevoked => Self::InviteLinkRevoked,
            AuditAction::PasswordChanged => Self::PasswordChanged,
            AuditAction::PasswordResetLinkMade => Self::PasswordResetLinkMade,
            AuditAction::PasswordReset => Self::PasswordReset,
        }
    }
}

/// The kind of thing an entry is about. Members are `user`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AuditEntityType)]
pub enum EntityTypeDto {
    User,
    Team,
    Heap,
    Item,
    Tag,
    SavedView,
    Parameter,
    InviteLink,
}

impl From<EntityType> for EntityTypeDto {
    fn from(entity: EntityType) -> Self {
        match entity {
            EntityType::User => Self::User,
            EntityType::Team => Self::Team,
            EntityType::Heap => Self::Heap,
            EntityType::Item => Self::Item,
            EntityType::Tag => Self::Tag,
            EntityType::SavedView => Self::SavedView,
            EntityType::Parameter => Self::Parameter,
            EntityType::InviteLink => Self::InviteLink,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AuditEntity)]
pub struct AuditEntityDto {
    #[serde(rename = "type")]
    pub entity_type: EntityTypeDto,
    pub id: Uuid,
    /// What it is called now: an item's title, a tag's name, a member's display name.
    pub label: String,
}

/// One changed field, ready to show.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AuditChange)]
pub struct AuditChangeDto {
    /// The field as the diff names it: `title`, `tags`, or `parameters.<id>` for one parameter's value.
    pub field: String,
    /// The field in words.
    pub label: String,
    /// `null` when it had no value, and for values with no short form, such as what a saved view shows.
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AuditEntry)]
pub struct AuditEntryDto {
    pub id: Uuid,
    /// UTC milliseconds.
    pub created_at: i64,
    /// `created_at` in the request's timezone, such as "12 Mar 2027, 14:05".
    pub created_label: String,
    pub actor: UserRefDto,
    pub action: AuditActionDto,
    /// What was done, to show after the actor's name: "edited item", "joined the team".
    pub phrase: String,
    /// Whether the entity's label goes after `phrase`. `false` when the entry is about the actor themselves.
    pub names_entity: bool,
    pub entity: AuditEntityDto,
    /// The changed fields, in the order the entity's form shows them. Names are the current ones.
    pub changes: Vec<AuditChangeDto>,
    /// The change as recorded, with IDs rather than names: `{"field": {"before": …, "after": …}}` for edits, the
    /// values set for creations.
    #[schema(value_type = Object)]
    pub diff: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AuditLog)]
pub struct AuditLogDto {
    /// Newest first.
    pub entries: Vec<AuditEntryDto>,
    /// Pass as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}
