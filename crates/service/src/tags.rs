//! Tag use cases: listing, creating and editing a heap's tags. Tags are never deleted.

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::permission::{Permission, TagAction, TagListAction, allowed_tag_actions, allowed_tag_list_actions};
use domain::tag::{NewTag, Tag, TagName, TagPatch};
use domain::{HeapId, TagId, Timestamp, Version};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::access::{authorize_heap, ensure_active};
use crate::audit::{describe_new_tag, diff_tags};
use crate::{Principal, ServiceError, Services};

/// A tag with what the caller may do with it.
#[derive(Debug)]
pub struct TagView {
    pub tag: Tag,
    pub allowed_actions: Vec<TagAction>,
}

/// Every tag of a heap, by name.
#[derive(Debug)]
pub struct TagList {
    pub heap: Heap,
    pub tags: Vec<TagView>,
    pub allowed_actions: Vec<TagListAction>,
}

impl Services {
    pub async fn list_tags(&self, principal: Principal, heap_id: HeapId) -> Result<TagList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let role = access.membership.role;
        let tags = store::tags::list_tags(&self.db, heap_id)
            .await?
            .into_iter()
            .map(|tag| TagView {
                tag,
                allowed_actions: allowed_tag_actions(&access.heap, role),
            })
            .collect();
        Ok(TagList {
            allowed_actions: allowed_tag_list_actions(&access.heap, role),
            heap: access.heap,
            tags,
        })
    }

    /// Adds a tag to the heap. Fails with [`ServiceError::TagNameTaken`] if the heap has one with that name,
    /// ignoring case.
    pub async fn create_tag(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewTag,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap(&txn, principal, heap_id, Permission::EditItems).await?;
        ensure_active(&access.heap)?;
        store::heaps::lock_heap(&txn, heap_id).await?;
        ensure_name_free(&txn, heap_id, &fields.name, None).await?;

        let now = self.now();
        let tag = Tag::new(TagId::from_uuid(self.new_id(now)), heap_id, fields, now);
        store::tags::insert_tag(&txn, &tag)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &tag))?;
        let entry = self.tag_audit_entry(
            &access.heap,
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
    /// A patch that changes nothing succeeds and returns the tag unchanged, without an audit entry.
    pub async fn update_tag(
        &self,
        principal: Principal,
        tag_id: TagId,
        patch: TagPatch,
        expected: Version,
    ) -> Result<TagView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::tags::find_tag(&txn, tag_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap(&txn, principal, current.heap_id, Permission::EditItems).await?;
        ensure_active(&access.heap)?;
        store::heaps::lock_heap(&txn, current.heap_id).await?;
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
        ensure_name_free(&txn, current.heap_id, &edited.name, Some(tag_id)).await?;
        let saved = store::tags::update_tag(&txn, &edited, expected)
            .await
            .map_err(|e| name_taken_on_duplicate(e, &edited))?
            .ok_or(ServiceError::VersionConflict {
                expected,
                current: expected.next(),
            })?;
        let now = self.now();
        let entry = self.tag_audit_entry(
            &access.heap,
            principal,
            AuditAction::TagUpdated,
            &saved,
            diff_tags(&current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(heap_id = %saved.heap_id, tag_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), "tag updated");
        Ok(TagView {
            tag: saved,
            allowed_actions,
        })
    }

    fn tag_audit_entry(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        tag: &Tag,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::Tag,
            entity_id: *tag.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

/// Refuses a name another tag of the heap already has. `except` is the tag being renamed.
///
/// The unique index backs this up when two requests race; see [`name_taken_on_duplicate`].
async fn ensure_name_free(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    name: &TagName,
    except: Option<TagId>,
) -> Result<(), ServiceError> {
    let existing = store::tags::find_tag_by_name(db, heap_id, name).await?;
    match existing {
        Some(other) if Some(other.id) != except => Err(ServiceError::TagNameTaken {
            name: name.as_str().to_owned(),
        }),
        _ => Ok(()),
    }
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
