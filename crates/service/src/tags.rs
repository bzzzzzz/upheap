//! Tag use cases: listing, creating and editing a heap's tags and a team's tags. Tags are never deleted.
//!
//! A heap's tags are its own and its team's. A heap tag write and a team tag write both lock the team row first
//! (the heap row after it, for a heap tag), so a heap tag and a team tag can't take one name at once
//! (design/architecture.md, Cycle detection and locking).

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::permission::{
    Permission, TagAction, TagListAction, allowed_tag_actions, allowed_tag_list_actions, allowed_team_tag_actions,
    allowed_team_tag_list_actions,
};
use domain::tag::{NewTag, Tag, TagName, TagOwner, TagPatch};
use domain::team::Membership;
use domain::{HeapId, TagId, TeamId, Timestamp, Version};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::access::{HeapAccess, authorize_heap, authorize_team, ensure_active};
use crate::audit::{describe_new_tag, diff_tags};
use crate::{Principal, ServiceError, Services};

/// A tag with what the caller may do with it.
#[derive(Debug)]
pub struct TagView {
    pub tag: Tag,
    pub allowed_actions: Vec<TagAction>,
}

/// The tags of a heap, by name: its own, and its team's apart.
#[derive(Debug)]
pub struct TagList {
    pub heap: Heap,
    pub tags: Vec<TagView>,
    pub team_tags: Vec<TagView>,
    pub allowed_actions: Vec<TagListAction>,
}

/// Every tag of a team (not of its heaps), by name.
#[derive(Debug)]
pub struct TeamTagList {
    pub tags: Vec<TagView>,
    pub allowed_actions: Vec<TagListAction>,
}

impl Services {
    pub async fn list_tags(&self, principal: Principal, heap_id: HeapId) -> Result<TagList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let role = access.membership.role;
        let (tags, team_tags) = store::tags::list_tags(&self.db, heap_id)
            .await?
            .into_iter()
            .map(|tag| {
                // A team's tag is edited on the team's path, even from an archived heap's list.
                let allowed_actions = match tag.owner {
                    TagOwner::Heap(_) => allowed_tag_actions(&access.heap, role),
                    TagOwner::Team(_) => allowed_team_tag_actions(role),
                };
                TagView { tag, allowed_actions }
            })
            .partition(|view| matches!(view.tag.owner, TagOwner::Heap(_)));
        Ok(TagList {
            allowed_actions: allowed_tag_list_actions(&access.heap, role),
            heap: access.heap,
            tags,
            team_tags,
        })
    }

    /// Adds a tag to the heap. Fails with [`ServiceError::TagNameTaken`] if the heap has one with that name,
    /// ignoring case, and with [`ServiceError::TeamTagNameTaken`] if its team has.
    pub async fn create_tag(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewTag,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_tag_write(&txn, principal, heap_id).await?;
        ensure_name_free(&txn, &access.heap, &fields.name, None).await?;

        let now = self.now();
        let tag = Tag::new(TagId::from_uuid(self.new_id(now)), TagOwner::Heap(heap_id), fields, now);
        store::tags::insert_tag(&txn, &tag)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &tag))?;
        let entry = self.tag_audit_entry(
            access.heap.team_id,
            Some(heap_id),
            principal,
            AuditAction::TagCreated,
            &tag,
            describe_new_tag(&tag),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, tag_id = %tag.id, user_id = %principal.user_id, "tag created");
        Ok(TagView {
            allowed_actions: allowed_tag_actions(&access.heap, access.membership.role),
            tag,
        })
    }

    /// Renames a tag or changes its description, if it is still at version `expected`.
    ///
    /// A patch that changes nothing succeeds and returns the tag unchanged, without an audit entry. A tag of another
    /// heap than `heap_id`, such as a team's tag, is [`ServiceError::NotFound`].
    pub async fn update_tag(
        &self,
        principal: Principal,
        heap_id: HeapId,
        tag_id: TagId,
        patch: TagPatch,
        expected: Version,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::tags::find_tag(&txn, tag_id)
            .await?
            .filter(|tag| tag.owner == TagOwner::Heap(heap_id))
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_tag_write(&txn, principal, heap_id).await?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        let allowed_actions = allowed_tag_actions(&access.heap, access.membership.role);
        let Some(edited) = current.apply_patch(&patch) else {
            return Ok(TagView {
                tag: current,
                allowed_actions,
            });
        };
        ensure_name_free(&txn, &access.heap, &edited.name, Some(tag_id)).await?;
        let saved = store::tags::update_tag(&txn, &edited, expected)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &edited))?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let now = self.now();
        let entry = self.tag_audit_entry(
            access.heap.team_id,
            Some(heap_id),
            principal,
            AuditAction::TagUpdated,
            &saved,
            diff_tags(&current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, tag_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), "tag updated");
        Ok(TagView {
            tag: saved,
            allowed_actions,
        })
    }

    /// The team's tags, by name.
    pub async fn list_team_tags(&self, principal: Principal, team_id: TeamId) -> Result<TeamTagList, ServiceError> {
        let membership = authorize_team(&self.db, principal, team_id, Permission::ViewTeam).await?;
        let role = membership.role;
        let tags = store::tags::list_team_tags(&self.db, team_id)
            .await?
            .into_iter()
            .map(|tag| TagView {
                tag,
                allowed_actions: allowed_team_tag_actions(role),
            })
            .collect();
        Ok(TeamTagList {
            tags,
            allowed_actions: allowed_team_tag_list_actions(role),
        })
    }

    /// Adds a tag to the team, so every heap of the team has it. Fails with [`ServiceError::TagNameTaken`] if the
    /// team has a tag, or any of its heaps (archived ones included) has one, with that name, ignoring case.
    pub async fn create_team_tag(
        &self,
        principal: Principal,
        team_id: TeamId,
        fields: NewTag,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let membership = authorize_team_write(&txn, principal, team_id).await?;
        ensure_team_tag_name_free(&txn, team_id, &fields.name, None).await?;

        let now = self.now();
        let tag = Tag::new(TagId::from_uuid(self.new_id(now)), TagOwner::Team(team_id), fields, now);
        store::tags::insert_tag(&txn, &tag)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &tag))?;
        let entry = self.tag_audit_entry(
            team_id,
            None,
            principal,
            AuditAction::TagCreated,
            &tag,
            describe_new_tag(&tag),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, tag_id = %tag.id, user_id = %principal.user_id, "team tag created");
        Ok(TagView {
            allowed_actions: allowed_team_tag_actions(membership.role),
            tag,
        })
    }

    /// Renames a team's tag or changes its description, if it is still at version `expected`.
    ///
    /// A patch that changes nothing succeeds and returns the tag unchanged, without an audit entry. A tag that
    /// isn't a tag of `team_id` is [`ServiceError::NotFound`].
    pub async fn update_team_tag(
        &self,
        principal: Principal,
        team_id: TeamId,
        tag_id: TagId,
        patch: TagPatch,
        expected: Version,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let membership = authorize_team_write(&txn, principal, team_id).await?;
        let current = store::tags::find_tag(&txn, tag_id)
            .await?
            .filter(|tag| tag.owner == TagOwner::Team(team_id))
            .ok_or(ServiceError::NotFound)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        let allowed_actions = allowed_team_tag_actions(membership.role);
        let Some(edited) = current.apply_patch(&patch) else {
            return Ok(TagView {
                tag: current,
                allowed_actions,
            });
        };
        ensure_team_tag_name_free(&txn, team_id, &edited.name, Some(tag_id)).await?;
        let saved = store::tags::update_tag(&txn, &edited, expected)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &edited))?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let now = self.now();
        let entry = self.tag_audit_entry(
            team_id,
            None,
            principal,
            AuditAction::TagUpdated,
            &saved,
            diff_tags(&current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, tag_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), "team tag updated");
        Ok(TagView {
            tag: saved,
            allowed_actions,
        })
    }

    /// The audit entry of a tag write: the heap's for a heap tag, the team's alone (`heap_id` is `None`) for a
    /// team tag.
    #[allow(clippy::too_many_arguments)] // Every one differs between the callers.
    fn tag_audit_entry(
        &self,
        team_id: TeamId,
        heap_id: Option<HeapId>,
        principal: Principal,
        action: AuditAction,
        tag: &Tag,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id,
            heap_id,
            actor: principal.user_id,
            action,
            entity_type: EntityType::Tag,
            entity_id: *tag.id.as_uuid(),
            diff,
            run_id: None,
            created_at: now,
        }
    }
}

/// For a write to a team's tags: locks the team row, then checks the caller may edit.
///
/// The lock comes first so the reads after it see what a competing write committed; a non-member gets
/// [`ServiceError::NotFound`], as for a missing team.
async fn authorize_team_write(
    db: &impl ConnectionTrait,
    principal: Principal,
    team_id: TeamId,
) -> Result<Membership, ServiceError> {
    store::teams::lock_team(db, team_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    authorize_team(db, principal, team_id, Permission::EditItems).await
}

/// For a write to a heap's tags: checks the caller may edit, locks the team row, then the heap row, and refuses an
/// archived heap on the row read under the lock.
///
/// The team row comes first so a heap tag and a team tag can't take one name at once; the heap row is locked
/// after it, as the rest of the code locks a team before what's in it.
async fn authorize_tag_write(
    db: &impl ConnectionTrait,
    principal: Principal,
    heap_id: HeapId,
) -> Result<HeapAccess, ServiceError> {
    let access = authorize_heap(db, principal, heap_id, Permission::EditItems).await?;
    store::teams::lock_team(db, access.heap.team_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let heap = store::heaps::lock_heap(db, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    ensure_active(&heap)?;
    Ok(HeapAccess {
        heap,
        membership: access.membership,
    })
}

/// Refuses a name another tag of the heap already has, or a tag of the heap's team has. `except` is the tag being
/// renamed.
///
/// The unique index backs this up when two requests race; see [`name_taken_on_duplicate`].
async fn ensure_name_free(
    db: &impl ConnectionTrait,
    heap: &Heap,
    name: &TagName,
    except: Option<TagId>,
) -> Result<(), ServiceError> {
    let existing = store::tags::find_tag_by_name(db, heap.id, name).await?;
    if existing.is_some_and(|other| Some(other.id) != except) {
        return Err(ServiceError::TagNameTaken {
            name: name.as_str().to_owned(),
        });
    }
    match store::tags::find_team_tag_by_name(db, heap.team_id, name).await? {
        Some(team_tag) => Err(ServiceError::TeamTagNameTaken {
            name: team_tag.name.as_str().to_owned(),
        }),
        None => Ok(()),
    }
}

/// Refuses a name another tag of the team has, or any heap of the team has a tag called. `except` is the tag being
/// renamed.
async fn ensure_team_tag_name_free(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    name: &TagName,
    except: Option<TagId>,
) -> Result<(), ServiceError> {
    let existing = store::tags::find_team_tag_by_name(db, team_id, name).await?;
    let taken = existing.is_some_and(|other| Some(other.id) != except)
        || store::tags::has_heap_tag_named_in_team(db, team_id, name).await?;
    if taken {
        return Err(ServiceError::TagNameTaken {
            name: name.as_str().to_owned(),
        });
    }
    Ok(())
}

/// A duplicate-key failure on a tag write means another request took the name first.
fn name_taken_on_duplicate(err: StoreError, tag: &Tag) -> ServiceError {
    match err {
        StoreError::Duplicate(_) => ServiceError::TagNameTaken {
            name: tag.name.as_str().to_owned(),
        },
        other => ServiceError::Store(other),
    }
}
