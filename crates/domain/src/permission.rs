//! The roles table from the offering, as code. Service checks and `allowed_actions` both come from here.

use crate::Timestamp;
use crate::heap::Heap;
use crate::item::Item;
use crate::team::Role;

/// Something a team member may be allowed to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// See a heap, its items and its tags.
    ViewHeap,
    /// Add and edit items and tags.
    EditItems,
    /// Create, archive and unarchive heaps.
    ManageHeaps,
    /// Change parameters, weights and the ranking policy.
    ConfigureHeap,
    /// Add and remove members, promote and demote admins.
    ManageMembers,
}

impl Permission {
    pub const fn is_granted_to(self, role: Role) -> bool {
        match self {
            Self::ViewHeap | Self::EditItems => true,
            Self::ManageHeaps | Self::ConfigureHeap | Self::ManageMembers => matches!(role, Role::Admin),
        }
    }
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
    if !can_edit_tags(heap, role) {
        return Vec::new();
    }
    vec![TagAction::Edit]
}

/// What `role` can do with the tag list of `heap`. Empty for archived heaps.
pub fn allowed_tag_list_actions(heap: &Heap, role: Role) -> Vec<TagListAction> {
    if !can_edit_tags(heap, role) {
        return Vec::new();
    }
    vec![TagListAction::Create]
}

/// Tags are edited under the same permission as items: the roles table grants both to every member.
const fn can_edit_tags(heap: &Heap, role: Role) -> bool {
    !heap.is_archived() && Permission::EditItems.is_granted_to(role)
}
