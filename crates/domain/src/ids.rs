//! Typed IDs, so arguments of different kinds can't be swapped, and coded enums, whose values have stable codes.
//! Generating new IDs is the caller's job.

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

/// Declares an enum whose values have a stable code, with `ALL`, `code` and `from_code`.
///
/// Each variant is written once, beside its code, so `ALL` can't leave one out: a `match` can't check a list, and a
/// variant missing from a hand-written `ALL` compiles, then `from_code` answers `None` for a code that is stored.
/// Codes are written out, never derived from the variant names, because they are stored for good and renaming a
/// variant must not change one. The caller writes the derives, which the macro leaves as they are.
macro_rules! coded_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $code:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every value, in the order they are declared.
            pub const ALL: [Self; [$(Self::$variant),+].len()] = [$(Self::$variant),+];

            /// The value's stable code, as it is stored or sent. Never renamed.
            pub const fn code(self) -> &'static str {
                match self {
                    $( Self::$variant => $code, )+
                }
            }

            /// The value whose code is `code`, or `None` for a code no version of the application wrote.
            pub fn from_code(code: &str) -> Option<Self> {
                Self::ALL.into_iter().find(|value| value.code() == code)
            }
        }
    };
}
pub(crate) use coded_enum;

id_type!(UserId);
id_type!(TeamId);
id_type!(HeapId);
id_type!(ItemId);
id_type!(StepId);
id_type!(QuestionId);
id_type!(CommentId);
id_type!(TurnId);
id_type!(RunId);
id_type!(TagId);
id_type!(SavedViewId);
id_type!(InviteLinkId);
id_type!(ParameterId);
id_type!(OptionId);
id_type!(
    /// A personal token (`domain::personal_token`).
    PersonalTokenId
);
id_type!(
    /// The OAuth tokens of one sign-in by a client, and every pair refreshed from them (`domain::oauth`).
    TokenFamilyId
);

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use crate::audit::{AuditAction, AuditField, EntityType};
    use crate::invite_link::InviteLinkState;
    use crate::item::{Kind, Need, Size, Stage, StateKind};
    use crate::list_view::{BlockedLayout, RemindersMode, StateMode};
    use crate::question::{QuestionKind, QuestionState};
    use crate::run::{RunEnd, RunReached};
    use crate::step::StepState;
    use crate::tag::TagColor;
    use crate::team::Role;
    use crate::turn::{Goal, Reach, TakeBackWhy, TurnEnd, TurnSource};

    /// Pins an enum's codes, written out here, in order: a new variant, a changed code or a reorder fails. Every
    /// value must also read back from its code.
    macro_rules! pinned {
        ($enum:ident, [$($code:literal),+ $(,)?]) => {
            expect_that!($enum::ALL.map($enum::code), eq([$($code),+]));
            for value in $enum::ALL {
                expect_that!($enum::from_code(value.code()), some(eq(value)));
            }
        };
    }

    #[gtest]
    fn every_code_is_pinned() {
        pinned!(
            AuditAction,
            [
                "user_registered",
                "user_updated",
                "team_created",
                "team_updated",
                "member_added",
                "member_role_changed",
                "member_removed",
                "member_left",
                "member_joined",
                "group_member_added",
                "group_member_removed",
                "heap_created",
                "heap_updated",
                "heap_context_updated",
                "heap_archived",
                "heap_unarchived",
                "heap_priority_changed",
                "item_created",
                "item_updated",
                "item_moved",
                "item_placed",
                "step_added",
                "step_updated",
                "question_asked",
                "question_answered",
                "reply_changed",
                "question_withdrawn",
                "comment_added",
                "turn_opened",
                "turn_taken_back",
                "turn_reached",
                "turn_handed_back",
                "turn_watching",
                "turn_retried",
                "turn_redone",
                "tag_created",
                "tag_updated",
                "tag_merged",
                "saved_view_created",
                "saved_view_updated",
                "saved_view_archived",
                "saved_view_unarchived",
                "parameter_created",
                "parameter_updated",
                "parameter_archived",
                "parameter_merged",
                "option_archived",
                "invite_link_created",
                "invite_link_revoked",
                "password_changed",
                "password_reset_link_made",
                "password_reset",
                "account_deactivated",
                "account_reactivated",
                "email_change_asked",
                "email_changed",
                "email_confirmed",
                "operator_granted",
                "operator_removed"
            ]
        );
        pinned!(
            AuditField,
            [
                "step",
                "step_title",
                "step_context",
                "step_state",
                "step_place",
                "question",
                "question_kind",
                "question_text",
                "question_details",
                "question_choices",
                "question_many",
                "question_step",
                "comment",
                "comment_text",
                "reply",
                "reply_comment",
                "reach",
                "goal",
                "line",
                "source",
                "why",
                "how",
                "reason",
                "blocker_done",
                "title",
                "heap",
                "item_title",
                "key",
                "placement",
                "kind",
                "stage",
                "name",
                "display_name",
                "description",
                "color",
                "context",
                "state",
                "blocked_by",
                "origin",
                "blocked_reason",
                "need",
                "effort",
                "impact",
                "due",
                "tags",
                "parameters",
                "role",
                "weight",
                "need_weight",
                "age_weight",
                "deadline_weight",
                "value_weight",
                "tags_weight",
                "tag_order",
                "options",
                "archived_options",
                "items_switched",
                "ended_sign_ins",
                "list_view",
                "agents_prepare",
                "ends_at",
                "heap_id",
                "made_by",
                "priority",
                "question_link",
                "question_piece",
                "question_round",
                "slug",
                "username",
                "groups",
                "by",
                "archived_at",
                "merged",
                "email",
                "url",
                "used"
            ]
        );
        pinned!(
            EntityType,
            [
                "user",
                "team",
                "heap",
                "item",
                "tag",
                "saved_view",
                "parameter",
                "invite_link"
            ]
        );
        pinned!(Size, ["s", "m", "l", "xl"]);
        pinned!(Need, ["idea", "nice_to_have", "expected", "critical"]);
        pinned!(Kind, ["note", "task", "reminder"]);
        pinned!(Stage, ["as_written", "with_context", "defined"]);
        pinned!(
            StateKind,
            ["open", "in_progress", "blocked", "done", "wont_do", "became"]
        );
        pinned!(QuestionKind, ["question", "request", "review", "done_proposal"]);
        pinned!(QuestionState, ["open", "answered", "withdrawn"]);
        pinned!(StateMode, ["pinned", "shown", "hidden"]);
        pinned!(BlockedLayout, ["grouped", "mixed"]);
        pinned!(RemindersMode, ["hidden", "shown"]);
        pinned!(
            RunEnd,
            [
                "reached",
                "asked",
                "failed",
                "taken_back",
                "handed_back",
                "limited",
                "watching"
            ]
        );
        pinned!(RunReached, ["with_context", "defined", "became"]);
        pinned!(Reach, ["one", "all_the_way"]);
        pinned!(Goal, ["with_context", "defined", "done"]);
        pinned!(TurnSource, ["go_ahead", "prepare"]);
        pinned!(TurnEnd, ["taken_back", "reached", "handed_back"]);
        pinned!(
            TakeBackWhy,
            [
                "take_back",
                "started",
                "closed",
                "heap_archived",
                "moved",
                "goal_met",
                "handed_back"
            ]
        );
        pinned!(StepState, ["open", "ticked", "dropped"]);
        pinned!(Role, ["admin", "member"]);
        pinned!(
            TagColor,
            ["blue", "sky", "teal", "green", "lime", "rose", "pink", "grey"]
        );
        pinned!(InviteLinkState, ["working", "ended", "revoked"]);

        expect_that!(Size::from_code("unknown"), none());
    }
}
