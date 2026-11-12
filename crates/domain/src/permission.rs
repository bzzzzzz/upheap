//! The roles table from the offering, as code. Service checks and `allowed_actions` both come from here.

use crate::audit::AuditAction;
use crate::heap::Heap;
use crate::invite_link::InviteLink;
use crate::item::{Item, Kind, Stage};
use crate::parameter::Parameter;
use crate::question::QuestionState;
use crate::saved_view::SavedView;
use crate::team::{Membership, Role, Team, admin_remains};
use crate::{Timestamp, UserId};

/// Something a team member may be allowed to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// See a heap, its items and its tags.
    ViewHeap,
    /// Add and edit items, tags and saved views.
    EditItems,
    /// Create, rename, archive and unarchive heaps.
    ManageHeaps,
    /// Change parameters, weights and the ranking policy.
    ConfigureHeap,
    /// See the team, its members and its heaps.
    ViewTeam,
    /// Leave the team. The last admin can't (see [`crate::team::TeamError::LastAdmin`]).
    LeaveTeam,
    /// Rename the team, add and remove members, promote and demote admins.
    ManageTeam,
}

impl Permission {
    pub const fn is_granted_to(self, role: Role) -> bool {
        match self {
            Self::ViewHeap | Self::EditItems | Self::ViewTeam | Self::LeaveTeam => true,
            Self::ManageHeaps | Self::ConfigureHeap | Self::ManageTeam => matches!(role, Role::Admin),
        }
    }
}

/// The permission needed to see audit entries of `action`: the one needed to take it (product.md, Audit log §4).
/// Members see item, tag and saved view changes; only admins see heap, parameter, team and membership changes.
///
/// Leaving a team is the exception: every member may leave, but leaving is a membership change, which the offering
/// shows to admins only. Display name changes are shown to every member, since that is where teammates see the name.
///
/// A password change, a reset link being made and a password set with one are recorded in the user's personal team,
/// whose one member is its admin, so only the user sees them.
pub const fn audit_permission(action: AuditAction) -> Permission {
    match action {
        AuditAction::UserRegistered | AuditAction::UserUpdated => Permission::ViewTeam,
        AuditAction::TeamCreated
        | AuditAction::TeamUpdated
        | AuditAction::MemberAdded
        | AuditAction::MemberRoleChanged
        | AuditAction::MemberRemoved
        | AuditAction::MemberLeft
        | AuditAction::MemberJoined
        | AuditAction::InviteLinkCreated
        | AuditAction::InviteLinkRevoked
        | AuditAction::PasswordChanged
        | AuditAction::PasswordResetLinkMade
        | AuditAction::PasswordReset => Permission::ManageTeam,
        AuditAction::HeapCreated
        | AuditAction::HeapUpdated
        | AuditAction::HeapArchived
        | AuditAction::HeapUnarchived => Permission::ManageHeaps,
        AuditAction::ItemCreated
        | AuditAction::ItemUpdated
        | AuditAction::ItemMoved
        | AuditAction::HeapContextUpdated
        | AuditAction::StepAdded
        | AuditAction::StepUpdated
        | AuditAction::QuestionAsked
        | AuditAction::QuestionAnswered
        | AuditAction::QuestionWithdrawn
        | AuditAction::CommentAdded
        | AuditAction::TurnOpened
        | AuditAction::TurnTakenBack
        | AuditAction::TurnReached
        | AuditAction::TurnRetried
        | AuditAction::TagCreated
        | AuditAction::TagUpdated
        | AuditAction::SavedViewCreated
        | AuditAction::SavedViewUpdated
        | AuditAction::SavedViewArchived => Permission::EditItems,
        AuditAction::ParameterCreated
        | AuditAction::ParameterUpdated
        | AuditAction::ParameterArchived
        | AuditAction::OptionArchived => Permission::ConfigureHeap,
    }
}

/// The audit actions `role` may see, per [`audit_permission`].
pub fn visible_audit_actions(role: Role) -> Vec<AuditAction> {
    AuditAction::ALL
        .into_iter()
        .filter(|action| audit_permission(*action).is_granted_to(role))
        .collect()
}

/// An action a client may offer on an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    /// Change the title, description, need, effort, impact, due time or tags.
    Edit,
    /// Move to another heap of the item's team.
    Move,
    /// Move between non-closed states, or close.
    ChangeState,
    /// Move a closed item back to open.
    Reopen,
    /// Make a note or a reminder a task.
    MakeTask,
    /// Add a step to a task.
    AddStep,
    /// Ask a question or make a request on an open item.
    AskQuestion,
    /// Add a comment, on an open or a closed item.
    Comment,
    /// Send an open item that no agent has to an agent.
    SendToAgent,
    /// Take an open item back from the agent that has it.
    TakeBack,
    /// Let the agents try again at an item whose turn is stuck.
    Retry,
}

/// What `role` can do with `item` in `heap` at `now`, given whether it has an open turn and whether that turn is
/// `stuck`. Empty for archived heaps.
pub fn allowed_item_actions(
    item: &Item,
    heap: &Heap,
    role: Role,
    has_open_turn: bool,
    stuck: bool,
    now: Timestamp,
) -> Vec<ItemAction> {
    if heap.is_archived() || !Permission::EditItems.is_granted_to(role) {
        return Vec::new();
    }
    if !item.is_closed() {
        let mut actions = vec![ItemAction::Edit, ItemAction::Move, ItemAction::ChangeState];
        match item.kind {
            Kind::Note | Kind::Reminder => actions.push(ItemAction::MakeTask),
            Kind::Task => actions.push(ItemAction::AddStep),
        }
        actions.push(ItemAction::AskQuestion);
        actions.push(ItemAction::Comment);
        if has_open_turn && stuck {
            actions.push(ItemAction::Retry);
        }
        if has_open_turn {
            actions.push(ItemAction::TakeBack);
        } else if item.kind != Kind::Reminder {
            actions.push(ItemAction::SendToAgent);
        }
        return actions;
    }
    if item.can_reopen(now) {
        return vec![ItemAction::Reopen, ItemAction::Comment];
    }
    vec![ItemAction::Comment]
}

/// The stages `role` may set on `item` in `heap` now: none unless they may edit it.
pub fn allowed_stage_choices(item: &Item, heap: &Heap, role: Role) -> Vec<Stage> {
    if heap.is_archived() || !Permission::EditItems.is_granted_to(role) {
        return Vec::new();
    }
    item.stage_choices()
}

/// An action a client may offer on a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    /// Change the step's title, context or state.
    Edit,
}

/// What `role` can do with the steps of `item` in `heap`: edit them while the item is open and the heap is active.
/// The same for every step of the item.
pub fn allowed_step_actions(item: &Item, heap: &Heap, role: Role) -> Vec<StepAction> {
    if item.is_closed() || !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![StepAction::Edit]
}

/// An action a client may offer on a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionAction {
    /// Reply to it.
    Reply,
}

/// What `role` can do with a question of `item` in `heap`: reply to it while it is open, the item is open and the
/// heap is active. An open question of a closed item reads only until the item is reopened.
pub fn allowed_question_actions(item: &Item, heap: &Heap, role: Role, state: QuestionState) -> Vec<QuestionAction> {
    if item.is_closed() || state != QuestionState::Open || !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![QuestionAction::Reply]
}

/// An action a client may offer on a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagAction {
    /// Rename it or change its description.
    Edit,
}

/// An action a client may offer on a heap's tag list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagListAction {
    /// Add a tag.
    Create,
}

/// What `role` can do with any tag of `heap`. Empty for archived heaps.
pub fn allowed_tag_actions(heap: &Heap, role: Role) -> Vec<TagAction> {
    if !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![TagAction::Edit]
}

/// What `role` can do with the tag list of `heap`. Empty for archived heaps.
pub fn allowed_tag_list_actions(heap: &Heap, role: Role) -> Vec<TagListAction> {
    if !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![TagListAction::Create]
}

/// Tags and saved views are shared by the team and edited under the same permission as items: the roles table
/// grants all three to every member. Nothing in an archived heap changes.
const fn can_edit_heap_content(heap: &Heap, role: Role) -> bool {
    !heap.is_archived() && Permission::EditItems.is_granted_to(role)
}

/// An action a client may offer on a saved view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedViewAction {
    /// Rename it or change what it shows.
    Edit,
    /// Take it out of the heap's list of views.
    Archive,
}

/// An action a client may offer on a heap's list of saved views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedViewListAction {
    /// Save a new view.
    Create,
}

/// What `role` can do with `view` in `heap`. Empty for archived views and archived heaps.
pub fn allowed_saved_view_actions(view: &SavedView, heap: &Heap, role: Role) -> Vec<SavedViewAction> {
    if view.is_archived() || !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![SavedViewAction::Edit, SavedViewAction::Archive]
}

/// What `role` can do with the saved views of `heap`. Empty for archived heaps.
pub fn allowed_saved_view_list_actions(heap: &Heap, role: Role) -> Vec<SavedViewListAction> {
    if !can_edit_heap_content(heap, role) {
        return Vec::new();
    }
    vec![SavedViewListAction::Create]
}

/// An action a client may offer on a custom parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterAction {
    /// Rename it, change its weight, add, rename, reorder and archive options, and link it to tags.
    Edit,
    /// Take it out of use. Refused while an open item has a value for it.
    Archive,
    /// Read its history. Parameter changes are shown to those who may make them (see [`audit_permission`]), so this
    /// stays offered on archived parameters and in archived heaps.
    ViewHistory,
}

/// An action a client may offer on a heap's list of custom parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterListAction {
    /// Add a parameter.
    Create,
}

/// What `role` can do with `parameter` in `heap`. Only its history, for those who may configure the heap, once the
/// parameter or the heap is archived.
pub fn allowed_parameter_actions(parameter: &Parameter, heap: &Heap, role: Role) -> Vec<ParameterAction> {
    if !Permission::ConfigureHeap.is_granted_to(role) {
        return Vec::new();
    }
    if parameter.is_archived() || heap.is_archived() {
        return vec![ParameterAction::ViewHistory];
    }
    vec![
        ParameterAction::Edit,
        ParameterAction::Archive,
        ParameterAction::ViewHistory,
    ]
}

/// What `role` can do with the parameters of `heap`. Empty for archived heaps.
pub fn allowed_parameter_list_actions(heap: &Heap, role: Role) -> Vec<ParameterListAction> {
    if !can_configure(heap, role) {
        return Vec::new();
    }
    vec![ParameterListAction::Create]
}

/// Parameters are heap configuration, which only admins change (product.md, Roles and permissions). Nothing in an
/// archived heap changes.
const fn can_configure(heap: &Heap, role: Role) -> bool {
    !heap.is_archived() && Permission::ConfigureHeap.is_granted_to(role)
}

/// An action a client may offer on a team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamAction {
    /// Change the team's name.
    Rename,
    /// Change the team's slug. Not offered on a personal team.
    ChangeSlug,
    /// Add a user as a member. Not offered on a personal team.
    AddMember,
    /// Make, list and revoke the team's invite links. Not offered on a personal team.
    ManageInviteLinks,
    /// Add a heap to the team.
    CreateHeap,
    /// Leave the team. Not offered to the last admin, nor on a personal team.
    Leave,
}

/// What `user_id` can do with `team`, whose memberships are `members`. Empty for non-members.
pub fn allowed_team_actions(team: &Team, members: &[Membership], user_id: UserId) -> Vec<TeamAction> {
    let Some(role) = members.iter().find(|m| m.user_id == user_id).map(|m| m.role) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    let shared = !team.is_personal();
    if Permission::ManageTeam.is_granted_to(role) {
        actions.push(TeamAction::Rename);
        if shared {
            actions.extend([
                TeamAction::ChangeSlug,
                TeamAction::AddMember,
                TeamAction::ManageInviteLinks,
            ]);
        }
    }
    if Permission::ManageHeaps.is_granted_to(role) {
        actions.push(TeamAction::CreateHeap);
    }
    if shared && Permission::LeaveTeam.is_granted_to(role) && admin_remains(members, user_id, None) {
        actions.push(TeamAction::Leave);
    }
    actions
}

/// An action a client may offer on an invite link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InviteLinkAction {
    /// Stop the link working. Offered while it works.
    Revoke,
}

/// What `role` can do with `link` at `now`. Revoking needs the permission that making the link needed, and only a
/// working link can be revoked.
pub fn allowed_invite_link_actions(link: &InviteLink, role: Role, now: Timestamp) -> Vec<InviteLinkAction> {
    if Permission::ManageTeam.is_granted_to(role) && link.is_working(now) {
        return vec![InviteLinkAction::Revoke];
    }
    Vec::new()
}

/// An action a client may offer to someone holding an invite link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvitationAction {
    /// Join the team through the link, signed in.
    Join,
    /// Create an account through the link, joining the team in the same act.
    CreateAccount,
}

/// What a viewer can do with the invite link they hold at `now`. `viewer_is_member` is `None` when nobody is signed
/// in, else whether they are already in the team. `last_removed_at` is their last removal from the team, if any.
/// Joining is offered only to a signed-in outsider the link would admit: working, and not made before their removal.
/// Creating an account is offered to someone signed out while the link works; a new account has no removal.
pub fn allowed_invitation_actions(
    link: &InviteLink,
    viewer_is_member: Option<bool>,
    now: Timestamp,
    last_removed_at: Option<Timestamp>,
) -> Vec<InvitationAction> {
    match viewer_is_member {
        None if link.check_join(now, None).is_ok() => vec![InvitationAction::CreateAccount],
        Some(false) if link.check_join(now, last_removed_at).is_ok() => vec![InvitationAction::Join],
        _ => Vec::new(),
    }
}

/// An action a client may offer on one member of a team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberAction {
    /// Make a member an admin.
    Promote,
    /// Make an admin a member. Not offered for the last admin.
    Demote,
    /// Take them out of the team. Not offered for the last admin, nor for the caller, who leaves instead.
    Remove,
}

/// What `caller` can do with `target`, both members of the team whose memberships are `members`.
pub fn allowed_member_actions(members: &[Membership], caller: &Membership, target: &Membership) -> Vec<MemberAction> {
    if !Permission::ManageTeam.is_granted_to(caller.role) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    match target.role {
        Role::Member => actions.push(MemberAction::Promote),
        Role::Admin if admin_remains(members, target.user_id, Some(Role::Member)) => {
            actions.push(MemberAction::Demote);
        }
        Role::Admin => {}
    }
    if target.user_id != caller.user_id && admin_remains(members, target.user_id, None) {
        actions.push(MemberAction::Remove);
    }
    actions
}

/// An action a client may offer on a heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapAction {
    /// Change the heap's name or slug. Not offered while it is archived.
    Rename,
    /// Make the heap read-only.
    Archive,
    /// Make an archived heap writable again.
    Unarchive,
    /// Write the heap's context. Offered to every member while the heap is active.
    EditContext,
    /// Change the heap's ranking weights, and its parameters' weights, together. Not offered while it is archived.
    EditWeights,
    /// Set whether agents prepare the items added to the heap. Not offered while it is archived.
    SetAgentsPrepare,
    /// Read the heap's own history: created, renamed, archived. Shown to those who may manage heaps (see
    /// [`audit_permission`]).
    ViewHistory,
}

/// What `role` can do with `heap`. An archived heap allows only unarchiving and reading its history, and a member
/// may only write its context.
pub fn allowed_heap_actions(heap: &Heap, role: Role) -> Vec<HeapAction> {
    let can_edit_context = !heap.is_archived() && Permission::EditItems.is_granted_to(role);
    if !Permission::ManageHeaps.is_granted_to(role) {
        return if can_edit_context {
            vec![HeapAction::EditContext]
        } else {
            Vec::new()
        };
    }
    if heap.is_archived() {
        return vec![HeapAction::Unarchive, HeapAction::ViewHistory];
    }
    let mut actions = vec![HeapAction::Rename, HeapAction::Archive];
    if can_edit_context {
        actions.push(HeapAction::EditContext);
    }
    if can_configure(heap, role) {
        actions.push(HeapAction::EditWeights);
        actions.push(HeapAction::SetAgentsPrepare);
    }
    actions.push(HeapAction::ViewHistory);
    actions
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::TeamId;
    use crate::Version;
    use crate::handle::Handle;
    use crate::text::Name;
    use crate::validation::Field;

    fn team(personal_user: Option<UserId>) -> Team {
        Team {
            id: TeamId::from_uuid(Uuid::from_u128(1)),
            slug: Handle::parse("ops", Field::Slug).expect("valid"),
            name: Name::parse("Ops").expect("valid"),
            personal_user,
            created_at: Timestamp::EPOCH,
            version: Version::INITIAL,
        }
    }

    fn member(n: u128, role: Role) -> Membership {
        Membership {
            team_id: TeamId::from_uuid(Uuid::from_u128(1)),
            user_id: UserId::from_uuid(Uuid::from_u128(n)),
            role,
            joined_at: Timestamp::EPOCH,
        }
    }

    fn heap(archived: bool) -> Heap {
        Heap {
            id: crate::HeapId::from_uuid(Uuid::from_u128(2)),
            team_id: TeamId::from_uuid(Uuid::from_u128(1)),
            slug: Handle::parse("ops", Field::Slug).expect("valid"),
            name: Name::parse("Ops").expect("valid"),
            policy: crate::heap::HeapPolicy::default(),
            context: None,
            agents_prepare: false,
            archived_at: archived.then_some(Timestamp::EPOCH),
            version: Version::INITIAL,
        }
    }

    #[gtest]
    fn admins_edit_weights_on_an_active_heap_only() {
        expect_that!(
            allowed_heap_actions(&heap(false), Role::Admin),
            elements_are![
                eq(&HeapAction::Rename),
                eq(&HeapAction::Archive),
                eq(&HeapAction::EditContext),
                eq(&HeapAction::EditWeights),
                eq(&HeapAction::SetAgentsPrepare),
                eq(&HeapAction::ViewHistory)
            ]
        );
        expect_that!(
            allowed_heap_actions(&heap(false), Role::Member),
            elements_are![eq(&HeapAction::EditContext)]
        );
        expect_that!(allowed_heap_actions(&heap(true), Role::Member), is_empty());
        expect_that!(
            allowed_heap_actions(&heap(true), Role::Admin),
            elements_are![eq(&HeapAction::Unarchive), eq(&HeapAction::ViewHistory)]
        );
    }

    #[gtest]
    fn members_see_content_changes_and_admins_see_everything() {
        let member = visible_audit_actions(Role::Member);
        for seen in [
            AuditAction::ItemUpdated,
            AuditAction::TagCreated,
            AuditAction::SavedViewArchived,
            AuditAction::UserUpdated,
        ] {
            expect_that!(member, contains(eq(&seen)));
        }
        for unseen in [
            AuditAction::MemberAdded,
            AuditAction::MemberLeft,
            AuditAction::HeapUpdated,
            AuditAction::ParameterUpdated,
            AuditAction::OptionArchived,
        ] {
            expect_that!(member, not(contains(eq(&unseen))));
        }
        expect_that!(visible_audit_actions(Role::Admin), len(eq(AuditAction::ALL.len())));
    }

    #[gtest]
    fn the_last_admin_is_not_offered_leave_demote_or_remove() {
        let admin = member(10, Role::Admin);
        let other = member(11, Role::Member);
        let members = [admin, other];

        expect_that!(
            allowed_team_actions(&team(None), &members, admin.user_id),
            elements_are![
                eq(&TeamAction::Rename),
                eq(&TeamAction::ChangeSlug),
                eq(&TeamAction::AddMember),
                eq(&TeamAction::ManageInviteLinks),
                eq(&TeamAction::CreateHeap)
            ]
        );
        expect_that!(allowed_member_actions(&members, &admin, &admin), is_empty());
        expect_that!(
            allowed_member_actions(&members, &admin, &other),
            elements_are![eq(&MemberAction::Promote), eq(&MemberAction::Remove)]
        );
    }

    #[gtest]
    fn with_two_admins_either_can_leave_or_be_demoted() {
        let first = member(10, Role::Admin);
        let second = member(11, Role::Admin);
        let members = [first, second];

        expect_that!(
            allowed_team_actions(&team(None), &members, first.user_id),
            contains(eq(&TeamAction::Leave))
        );
        expect_that!(
            allowed_member_actions(&members, &first, &second),
            elements_are![eq(&MemberAction::Demote), eq(&MemberAction::Remove)]
        );
        expect_that!(
            allowed_member_actions(&members, &first, &first),
            elements_are![eq(&MemberAction::Demote)]
        );
    }

    #[gtest]
    fn a_personal_team_offers_no_member_changes() {
        let owner = member(10, Role::Admin);
        let personal = team(Some(owner.user_id));

        expect_that!(
            allowed_team_actions(&personal, &[owner], owner.user_id),
            elements_are![eq(&TeamAction::Rename), eq(&TeamAction::CreateHeap)]
        );
    }

    #[gtest]
    fn members_can_only_leave() {
        let admin = member(10, Role::Admin);
        let plain = member(11, Role::Member);
        let members = [admin, plain];

        expect_that!(
            allowed_team_actions(&team(None), &members, plain.user_id),
            elements_are![eq(&TeamAction::Leave)]
        );
        expect_that!(allowed_member_actions(&members, &plain, &admin), is_empty());
        expect_that!(
            allowed_team_actions(&team(None), &members, UserId::from_uuid(Uuid::from_u128(99))),
            is_empty()
        );
    }

    fn item(kind: Kind, closed: bool) -> Item {
        use crate::item::{ItemState, NewItem, Title};
        use std::collections::{BTreeMap, BTreeSet};

        let fields = NewItem {
            kind: Kind::Note,
            title: Title::parse("Rotate the certs").expect("valid"),
            description: None,
            need: crate::item::Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
        };
        let mut item = Item::new(
            crate::ItemId::from_uuid(Uuid::from_u128(3)),
            crate::HeapId::from_uuid(Uuid::from_u128(2)),
            crate::handle::ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        );
        item.kind = kind;
        if closed {
            item.state = ItemState::Done {
                closed_at: Timestamp::EPOCH,
                closed_by: None,
            };
        }
        item
    }

    #[gtest]
    fn a_reminder_can_be_made_a_task_but_not_sent_to_an_agent_or_given_steps() {
        let now = Timestamp::EPOCH;
        let allowed = allowed_item_actions(
            &item(Kind::Reminder, false),
            &heap(false),
            Role::Member,
            false,
            false,
            now,
        );

        expect_that!(allowed.contains(&ItemAction::MakeTask), eq(true));
        expect_that!(allowed.contains(&ItemAction::SendToAgent), eq(false));
        expect_that!(allowed.contains(&ItemAction::AddStep), eq(false));
        expect_that!(allowed.contains(&ItemAction::Edit), eq(true));
    }

    #[gtest]
    fn an_open_item_can_be_moved_by_someone_who_can_edit_in_an_active_heap() {
        let now = Timestamp::EPOCH;
        let has_move = |item: &Item, heap: &Heap, role| {
            allowed_item_actions(item, heap, role, false, false, now).contains(&ItemAction::Move)
        };

        expect_that!(has_move(&item(Kind::Note, false), &heap(false), Role::Member), eq(true));
        expect_that!(has_move(&item(Kind::Task, false), &heap(false), Role::Member), eq(true));
        expect_that!(has_move(&item(Kind::Note, true), &heap(false), Role::Member), eq(false));
        expect_that!(has_move(&item(Kind::Note, false), &heap(true), Role::Member), eq(false));
    }

    #[gtest]
    fn only_an_open_note_can_be_made_a_task_by_someone_who_can_edit() {
        let now = Timestamp::EPOCH;
        let has_make_task = |item: &Item, heap: &Heap, role| {
            allowed_item_actions(item, heap, role, false, false, now).contains(&ItemAction::MakeTask)
        };

        expect_that!(
            has_make_task(&item(Kind::Note, false), &heap(false), Role::Member),
            eq(true)
        );
        expect_that!(
            has_make_task(&item(Kind::Task, false), &heap(false), Role::Member),
            eq(false)
        );
        expect_that!(
            has_make_task(&item(Kind::Note, true), &heap(false), Role::Member),
            eq(false)
        );
        expect_that!(
            has_make_task(&item(Kind::Note, false), &heap(true), Role::Member),
            eq(false)
        );
    }

    #[gtest]
    fn steps_are_added_to_an_open_task_and_edited_while_the_item_is_open() {
        let now = Timestamp::EPOCH;
        let has_add_step = |item: &Item, heap: &Heap| {
            allowed_item_actions(item, heap, Role::Member, false, false, now).contains(&ItemAction::AddStep)
        };

        expect_that!(has_add_step(&item(Kind::Task, false), &heap(false)), eq(true));
        expect_that!(has_add_step(&item(Kind::Note, false), &heap(false)), eq(false));
        expect_that!(has_add_step(&item(Kind::Task, true), &heap(false)), eq(false));
        expect_that!(has_add_step(&item(Kind::Task, false), &heap(true)), eq(false));

        expect_that!(
            allowed_step_actions(&item(Kind::Task, false), &heap(false), Role::Member),
            elements_are![eq(&StepAction::Edit)]
        );
        expect_that!(
            allowed_step_actions(&item(Kind::Task, true), &heap(false), Role::Member),
            is_empty()
        );
        expect_that!(
            allowed_step_actions(&item(Kind::Task, false), &heap(true), Role::Member),
            is_empty()
        );
    }

    #[gtest]
    fn an_open_item_is_sent_without_a_turn_and_taken_back_with_one() {
        let now = Timestamp::EPOCH;
        let actions = |item: &Item, heap: &Heap, has_turn| {
            let actions = allowed_item_actions(item, heap, Role::Member, has_turn, false, now);
            (
                actions.contains(&ItemAction::SendToAgent),
                actions.contains(&ItemAction::TakeBack),
            )
        };

        expect_that!(
            actions(&item(Kind::Note, false), &heap(false), false),
            eq((true, false))
        );
        expect_that!(actions(&item(Kind::Task, false), &heap(false), true), eq((false, true)));
        expect_that!(
            actions(&item(Kind::Task, true), &heap(false), false),
            eq((false, false))
        );
        expect_that!(actions(&item(Kind::Task, true), &heap(false), true), eq((false, false)));
        expect_that!(
            actions(&item(Kind::Task, false), &heap(true), false),
            eq((false, false))
        );
        expect_that!(actions(&item(Kind::Task, false), &heap(true), true), eq((false, false)));
    }

    #[gtest]
    fn retry_is_offered_on_a_stuck_turn_and_not_in_an_archived_heap() {
        let now = Timestamp::EPOCH;
        let task = item(Kind::Task, false);

        let stuck = allowed_item_actions(&task, &heap(false), Role::Member, true, true, now);
        expect_that!(stuck, contains(eq(&ItemAction::Retry)));
        expect_that!(stuck, contains(eq(&ItemAction::TakeBack)));

        let running = allowed_item_actions(&task, &heap(false), Role::Member, true, false, now);
        expect_that!(running, not(contains(eq(&ItemAction::Retry))));
        let archived = allowed_item_actions(&task, &heap(true), Role::Member, true, true, now);
        expect_that!(archived, is_empty());
    }

    #[gtest]
    fn only_an_admin_sets_agents_prepare_on_an_active_heap() {
        expect_that!(
            allowed_heap_actions(&heap(false), Role::Admin),
            contains(eq(&HeapAction::SetAgentsPrepare))
        );
        expect_that!(
            allowed_heap_actions(&heap(false), Role::Member),
            not(contains(eq(&HeapAction::SetAgentsPrepare)))
        );
        expect_that!(
            allowed_heap_actions(&heap(true), Role::Admin),
            not(contains(eq(&HeapAction::SetAgentsPrepare)))
        );
    }

    #[gtest]
    fn a_question_is_asked_on_an_open_item_and_replied_to_while_open() {
        let now = Timestamp::EPOCH;
        let can_ask = |item: &Item, heap: &Heap| {
            allowed_item_actions(item, heap, Role::Member, false, false, now).contains(&ItemAction::AskQuestion)
        };

        expect_that!(can_ask(&item(Kind::Note, false), &heap(false)), eq(true));
        expect_that!(can_ask(&item(Kind::Task, false), &heap(false)), eq(true));
        expect_that!(can_ask(&item(Kind::Task, true), &heap(false)), eq(false));
        expect_that!(can_ask(&item(Kind::Task, false), &heap(true)), eq(false));

        let reply = |item: &Item, heap: &Heap, state| allowed_question_actions(item, heap, Role::Member, state);
        expect_that!(
            reply(&item(Kind::Task, false), &heap(false), QuestionState::Open),
            elements_are![eq(&QuestionAction::Reply)]
        );
        expect_that!(
            reply(&item(Kind::Task, false), &heap(false), QuestionState::Answered),
            is_empty()
        );
        expect_that!(
            reply(&item(Kind::Task, false), &heap(false), QuestionState::Withdrawn),
            is_empty()
        );
        expect_that!(
            reply(&item(Kind::Task, true), &heap(false), QuestionState::Open),
            is_empty()
        );
        expect_that!(
            reply(&item(Kind::Task, false), &heap(true), QuestionState::Open),
            is_empty()
        );
    }

    #[gtest]
    fn a_comment_is_added_on_open_and_closed_items_but_not_in_an_archived_heap() {
        let now = Timestamp::EPOCH;
        let actions = |item: &Item, heap: &Heap| allowed_item_actions(item, heap, Role::Member, false, false, now);

        expect_that!(
            actions(&item(Kind::Note, false), &heap(false)),
            contains(eq(&ItemAction::Comment))
        );
        expect_that!(
            actions(&item(Kind::Task, false), &heap(false)),
            contains(eq(&ItemAction::Comment))
        );
        expect_that!(
            actions(&item(Kind::Reminder, false), &heap(false)),
            contains(eq(&ItemAction::Comment))
        );
        expect_that!(
            actions(&item(Kind::Task, true), &heap(false)),
            elements_are![eq(&ItemAction::Reopen), eq(&ItemAction::Comment)]
        );
        let long_closed = allowed_item_actions(
            &item(Kind::Task, true),
            &heap(false),
            Role::Member,
            false,
            false,
            Timestamp::EPOCH
                .checked_add(crate::time::Duration::from_days(30))
                .expect("in range"),
        );
        expect_that!(long_closed, elements_are![eq(&ItemAction::Comment)]);
        expect_that!(actions(&item(Kind::Task, false), &heap(true)), is_empty());
        expect_that!(actions(&item(Kind::Task, true), &heap(true)), is_empty());
    }

    #[gtest]
    fn stage_choices_are_offered_only_to_those_who_can_edit() {
        let note = item(Kind::Note, false);

        expect_that!(
            allowed_stage_choices(&note, &heap(false), Role::Member),
            elements_are![eq(&Stage::AsWritten), eq(&Stage::WithContext)]
        );
        expect_that!(allowed_stage_choices(&note, &heap(true), Role::Member), is_empty());
        expect_that!(
            allowed_stage_choices(&item(Kind::Task, true), &heap(false), Role::Member),
            is_empty()
        );
    }

    mod invitation {
        use super::*;
        use crate::invite_link::{INVITE_LINK_LIFETIME, InviteToken, Revocation};
        use crate::{Duration, InviteLinkId};

        fn link(revoked: bool) -> InviteLink {
            InviteLink {
                id: InviteLinkId::from_uuid(Uuid::from_u128(1)),
                team_id: TeamId::from_uuid(Uuid::from_u128(2)),
                token: InviteToken::new("t".to_owned()),
                created_by: UserId::from_uuid(Uuid::from_u128(3)),
                created_at: Timestamp::EPOCH.checked_add(Duration::from_days(1)).expect("in range"),
                revocation: revoked.then(|| Revocation {
                    at: Timestamp::EPOCH,
                    by: UserId::from_uuid(Uuid::from_u128(3)),
                }),
                version: Version::INITIAL,
            }
        }

        fn after(link: &InviteLink, offset: Duration) -> Timestamp {
            link.created_at.checked_add(offset).expect("in range")
        }

        #[gtest]
        fn an_outsider_may_join_through_a_working_link() {
            let link = link(false);
            let now = link.created_at;
            expect_that!(
                allowed_invitation_actions(&link, Some(false), now, None),
                elements_are![eq(&InvitationAction::Join)]
            );
            let earlier_removal = Some(Timestamp::EPOCH);
            expect_that!(
                allowed_invitation_actions(&link, Some(false), now, earlier_removal),
                elements_are![eq(&InvitationAction::Join)]
            );
        }

        #[gtest]
        fn a_signed_out_viewer_may_create_an_account_through_a_working_link() {
            let link = link(false);
            expect_that!(
                allowed_invitation_actions(&link, None, link.created_at, None),
                elements_are![eq(&InvitationAction::CreateAccount)]
            );
        }

        #[gtest]
        fn nothing_is_offered_to_a_member() {
            let link = link(false);
            expect_that!(
                allowed_invitation_actions(&link, Some(true), link.created_at, None),
                is_empty()
            );
        }

        #[gtest]
        fn nothing_is_offered_to_someone_removed_since_the_link_was_made() {
            let link = link(false);
            let removed = Some(after(&link, Duration::from_millis(1)));
            expect_that!(
                allowed_invitation_actions(&link, Some(false), link.created_at, removed),
                is_empty()
            );
        }

        #[gtest]
        fn nothing_is_offered_on_an_ended_or_revoked_link() {
            let working = link(false);
            let ended = after(&working, INVITE_LINK_LIFETIME);
            expect_that!(
                allowed_invitation_actions(&working, Some(false), ended, None),
                is_empty()
            );
            expect_that!(allowed_invitation_actions(&working, None, ended, None), is_empty());
            let revoked = link(true);
            expect_that!(
                allowed_invitation_actions(&revoked, Some(false), revoked.created_at, None),
                is_empty()
            );
            expect_that!(
                allowed_invitation_actions(&revoked, None, revoked.created_at, None),
                is_empty()
            );
        }
    }
}
