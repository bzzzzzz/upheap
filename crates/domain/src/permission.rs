//! The roles table from the offering, as code. Service checks and `allowed_actions` both come from here.

use crate::audit::AuditAction;
use crate::heap::Heap;
use crate::item::Item;
use crate::parameter::Parameter;
use crate::saved_view::SavedView;
use crate::team::{Membership, Role, admin_remains};
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

/// The permission needed to see audit entries of `action`: the one needed to take it (Off Audit §4). Members see
/// item, tag and saved view changes; only admins see heap, parameter, team and membership changes.
///
/// Leaving a team is the exception: every member may leave, but leaving is a membership change, which the offering
/// shows to admins only. Display name changes are shown to every member, since that is where teammates see the name.
pub const fn audit_permission(action: AuditAction) -> Permission {
    match action {
        AuditAction::UserRegistered | AuditAction::UserUpdated => Permission::ViewTeam,
        AuditAction::TeamCreated
        | AuditAction::TeamUpdated
        | AuditAction::MemberAdded
        | AuditAction::MemberRoleChanged
        | AuditAction::MemberRemoved
        | AuditAction::MemberLeft => Permission::ManageTeam,
        AuditAction::HeapCreated
        | AuditAction::HeapUpdated
        | AuditAction::HeapArchived
        | AuditAction::HeapUnarchived => Permission::ManageHeaps,
        AuditAction::ItemCreated
        | AuditAction::ItemUpdated
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
    /// Change the title, description, effort, impact, due time or tags.
    Edit,
    /// Move between non-closed states, or close.
    ChangeState,
    /// Move a closed item back to open.
    Reopen,
}

/// What `role` can do with `item` in `heap` at `now`. Empty for archived heaps.
pub fn allowed_item_actions(item: &Item, heap: &Heap, role: Role, now: Timestamp) -> Vec<ItemAction> {
    if heap.is_archived() || !Permission::EditItems.is_granted_to(role) {
        return Vec::new();
    }
    if !item.is_closed() {
        return vec![ItemAction::Edit, ItemAction::ChangeState];
    }
    if item.can_reopen(now) {
        return vec![ItemAction::Reopen];
    }
    Vec::new()
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

/// Parameters are heap configuration, which only admins change (Off Roles). Nothing in an archived heap changes.
const fn can_configure(heap: &Heap, role: Role) -> bool {
    !heap.is_archived() && Permission::ConfigureHeap.is_granted_to(role)
}

/// An action a client may offer on a team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamAction {
    /// Change the team's name.
    Rename,
    /// Add a user as a member.
    AddMember,
    /// Add a heap to the team.
    CreateHeap,
    /// Leave the team. Not offered to the last admin.
    Leave,
}

/// What `user_id` can do with the team whose memberships are `members`. Empty for non-members.
pub fn allowed_team_actions(members: &[Membership], user_id: UserId) -> Vec<TeamAction> {
    let Some(role) = members.iter().find(|m| m.user_id == user_id).map(|m| m.role) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    if Permission::ManageTeam.is_granted_to(role) {
        actions.extend([TeamAction::Rename, TeamAction::AddMember]);
    }
    if Permission::ManageHeaps.is_granted_to(role) {
        actions.push(TeamAction::CreateHeap);
    }
    if Permission::LeaveTeam.is_granted_to(role) && admin_remains(members, user_id, None) {
        actions.push(TeamAction::Leave);
    }
    actions
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
    /// Change the heap's name. Not offered while it is archived.
    Rename,
    /// Make the heap read-only.
    Archive,
    /// Make an archived heap writable again.
    Unarchive,
    /// Read the heap's own history: created, renamed, archived. Shown to those who may manage heaps (see
    /// [`audit_permission`]).
    ViewHistory,
}

/// What `role` can do with `heap`. An archived heap allows only unarchiving and reading its history.
pub fn allowed_heap_actions(heap: &Heap, role: Role) -> Vec<HeapAction> {
    if !Permission::ManageHeaps.is_granted_to(role) {
        return Vec::new();
    }
    if heap.is_archived() {
        return vec![HeapAction::Unarchive, HeapAction::ViewHistory];
    }
    vec![HeapAction::Rename, HeapAction::Archive, HeapAction::ViewHistory]
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::TeamId;

    fn member(n: u128, role: Role) -> Membership {
        Membership {
            team_id: TeamId::from_uuid(Uuid::from_u128(1)),
            user_id: UserId::from_uuid(Uuid::from_u128(n)),
            role,
            joined_at: Timestamp::EPOCH,
        }
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
            allowed_team_actions(&members, admin.user_id),
            elements_are![
                eq(&TeamAction::Rename),
                eq(&TeamAction::AddMember),
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
            allowed_team_actions(&members, first.user_id),
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
    fn members_can_only_leave() {
        let admin = member(10, Role::Admin);
        let plain = member(11, Role::Member);
        let members = [admin, plain];

        expect_that!(
            allowed_team_actions(&members, plain.user_id),
            elements_are![eq(&TeamAction::Leave)]
        );
        expect_that!(allowed_member_actions(&members, &plain, &admin), is_empty());
        expect_that!(
            allowed_team_actions(&members, UserId::from_uuid(Uuid::from_u128(99))),
            is_empty()
        );
    }
}
