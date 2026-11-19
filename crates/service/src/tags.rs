//! Tag use cases: listing, creating and editing a heap's tags and a team's tags. Tags are never deleted.
//!
//! A heap's tags are its own and its team's. A heap tag write locks its heap row, after a shared lock on the team's
//! lowest heap when that is another one, and a team tag write locks every heap of the team in heap ID order, so a
//! heap tag and a team tag can't take one name at once, even in a heap made while the team tag write runs, and a
//! merge, which writes in every heap, can't deadlock with an item write (design/architecture.md, Cycle detection and
//! locking).
//!
//! Making a team tag, or renaming one, into a name that heap tags of the team have merges them into it, once the
//! caller has confirmed the reach: the heap tags it takes in, and how many items carry each.

use std::collections::{BTreeMap, BTreeSet};

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::heap::Heap;
use domain::permission::{
    Permission, TagAction, TagListAction, allowed_tag_actions, allowed_tag_list_actions, allowed_team_tag_actions,
    allowed_team_tag_list_actions,
};
use domain::tag::{NewTag, Tag, TagName, TagOwner, TagPatch, merged_tag_order, reach_text};
use domain::team::Membership;
use domain::{HeapId, ItemId, TagId, TeamId, Timestamp, Version};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, DatabaseTransaction, StoreError, TransactionTrait};

use crate::access::{HeapAccess, authorize_heap, authorize_team, ensure_active};
use crate::audit::{describe_new_tag, diff_tags};
use crate::auth::sha256_hex;
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

/// The heap tags in use that a team tag's name would merge, and what they hold: the confirm of rule 9.
#[derive(Clone, Debug)]
pub struct MergeReach {
    /// Names this reach: a client that confirms with it has seen exactly these heap tags and counts.
    pub token: String,
    /// By heap name, then heap ID.
    pub entries: Vec<ReachEntry>,
}

/// One heap tag in a [`MergeReach`].
#[derive(Clone, Debug)]
pub struct ReachEntry {
    pub heap: Heap,
    pub tag: Tag,
    /// Items with an active link to the tag, whatever their state.
    pub items: u32,
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
        let lower = lower_team_heap(&self.db, heap_id).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_tag_write(&txn, principal, heap_id, lower).await?;
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
        let lower = lower_team_heap(&self.db, heap_id).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_tag_write(&txn, principal, heap_id, lower).await?;
        let current = store::tags::find_tag(&txn, tag_id)
            .await?
            .filter(|tag| tag.owner == TagOwner::Heap(heap_id))
            .ok_or(ServiceError::NotFound)?;
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
    /// team has a tag with that name, ignoring case.
    ///
    /// Heap tags of the team's heaps (archived ones included) with that name are merged into it. That needs
    /// `confirm`, the token of their reach; without it, or with a stale one, the write fails with
    /// [`ServiceError::TagMergeUnconfirmed`] and the current reach. A name no heap tag has needs none.
    pub async fn create_team_tag(
        &self,
        principal: Principal,
        team_id: TeamId,
        fields: NewTag,
        confirm: Option<&str>,
    ) -> Result<TagView, ServiceError> {
        let (txn, membership, heaps) = self.begin_team_tag_write(principal, team_id).await?;
        ensure_team_tag_name_free(&txn, team_id, &fields.name, None).await?;
        let reach = reach_of(&txn, &heaps, &fields.name).await?;
        ensure_confirmed(&fields.name, &reach, confirm)?;

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
        let merged = reach.entries.len();
        self.merge_heap_tags(&txn, principal, team_id, &tag, reach.entries, &heaps, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, tag_id = %tag.id, user_id = %principal.user_id, merged, "team tag created");
        Ok(TagView {
            allowed_actions: allowed_team_tag_actions(membership.role),
            tag,
        })
    }

    /// Renames a team's tag or changes its description, if it is still at version `expected`.
    ///
    /// A patch that changes nothing succeeds and returns the tag unchanged, without an audit entry. A tag that
    /// isn't a tag of `team_id` is [`ServiceError::NotFound`]. A rename to another name than the tag's own, ignoring
    /// case, merges the heap tags that name has, and needs `confirm` as [`Self::create_team_tag`] does.
    pub async fn update_team_tag(
        &self,
        principal: Principal,
        team_id: TeamId,
        tag_id: TagId,
        patch: TagPatch,
        expected: Version,
        confirm: Option<&str>,
    ) -> Result<TagView, ServiceError> {
        let (txn, membership, heaps) = self.begin_team_tag_write(principal, team_id).await?;
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
        let reach = if edited.name.to_key() == current.name.to_key() {
            MergeReach {
                token: String::new(),
                entries: Vec::new(),
            }
        } else {
            reach_of(&txn, &heaps, &edited.name).await?
        };
        ensure_confirmed(&edited.name, &reach, confirm)?;
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
        let merged = reach.entries.len();
        self.merge_heap_tags(&txn, principal, team_id, &saved, reach.entries, &heaps, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, tag_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), merged, "team tag updated");
        Ok(TagView {
            tag: saved,
            allowed_actions,
        })
    }

    /// Begins a write to a team's tags: a transaction holding the lock of every heap of the team, in heap ID order,
    /// with the caller's membership and the heaps as read under those locks.
    ///
    /// The heap IDs are read before the transaction, so the locks come before every other read in it (on MySQL a
    /// transaction's first plain read fixes its snapshot). Under the locks they are read again: a heap made in between
    /// could already hold a heap tag of the name this write takes, so if the set changed, the transaction is dropped
    /// and the write starts again with the new set. A heap made after that read meets this write on the team's lowest
    /// heap, which a heap tag write locks shared first (see [`authorize_tag_write`]). A team with no heaps has no lowest
    /// heap, so there the write locks the team row instead: making a heap checks the team row's key, so it waits for
    /// this write, and with no heaps there is no item write to deadlock with.
    async fn begin_team_tag_write(
        &self,
        principal: Principal,
        team_id: TeamId,
    ) -> Result<(DatabaseTransaction, Membership, Vec<Heap>), ServiceError> {
        let mut heap_ids = team_heap_ids(&self.db, team_id).await?;
        loop {
            let txn = self.db.begin().await.map_err(StoreError::from)?;
            if heap_ids.is_empty() {
                store::teams::lock_team(&txn, team_id)
                    .await?
                    .ok_or(ServiceError::NotFound)?;
            }
            let (membership, heaps) = authorize_team_write(&txn, principal, team_id, &heap_ids).await?;
            let now_ids = team_heap_ids(&txn, team_id).await?;
            if now_ids == heap_ids {
                return Ok((txn, membership, heaps));
            }
            txn.rollback().await.map_err(StoreError::from)?;
            heap_ids = now_ids;
        }
    }

    /// Merges the heap tags of `reach` into the team tag `team_tag`, at `now`, by the caller. The caller holds the lock
    /// of every heap of the team (`heaps`, as it read them under those locks), so the rows written here, in all of
    /// them, are read at the version they are written at.
    ///
    /// Each item that carries one of them, open or closed, gets the team tag in their place, and so do the parameters
    /// linked to them, the saved views filtering by them and the heaps' tag orders (the team tag takes the highest
    /// position among them). The heap tags are marked merged. The team tag's entry lists them; each item's entry says
    /// its tag change, so the item's history shows who merged and when. Parameters, views and orders follow the merge
    /// and get no entries.
    ///
    /// Size assumption: a merge does a handful of queries per item it reaches while holding every heap lock of the
    /// team, so item writes in the team wait for it. That is fine for merges of up to a few hundred items; there is
    /// no limit.
    #[allow(clippy::too_many_arguments)] // The merge's inputs, all read by the callers under the locks.
    async fn merge_heap_tags(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        team_id: TeamId,
        team_tag: &Tag,
        reach: Vec<ReachEntry>,
        heaps: &[Heap],
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        if reach.is_empty() {
            return Ok(());
        }
        let merged: BTreeSet<TagId> = reach.iter().map(|entry| entry.tag.id).collect();
        let merged_ids: Vec<TagId> = merged.iter().copied().collect();
        let team_tag_id = team_tag.id;
        // The set arithmetic of every place a tag is named: take the merged tags out, put the team tag in.
        let replaced = |tags: &BTreeSet<TagId>| -> Option<BTreeSet<TagId>> {
            (!tags.is_disjoint(&merged)).then(|| {
                let mut tags: BTreeSet<TagId> = tags.difference(&merged).copied().collect();
                tags.insert(team_tag_id);
                tags
            })
        };

        let item_ids: BTreeSet<ItemId> = store::items::list_active_tag_links(txn, &merged_ids)
            .await?
            .into_iter()
            .map(|(item, _)| item)
            .collect();
        for item_id in item_ids {
            let item = store::items::find_item(txn, item_id).await?.ok_or_else(|| {
                StoreError::corrupt("items", &item_id.to_string(), "an item with a tag link is missing")
            })?;
            let Some(tags) = replaced(&item.tags) else {
                continue;
            };
            let edited = domain::item::Item {
                tags: tags.clone(),
                ..item.clone()
            };
            store::items::update_item(txn, &edited, item.version, now)
                .await?
                .ok_or_else(|| locked_row_changed("items", &item_id.to_string()))?;
            let ids = |tags: &BTreeSet<TagId>| tags.iter().map(ToString::to_string).collect::<Vec<_>>();
            let diff = serde_json::json!({
                AuditField::Tags.code(): { "before": ids(&item.tags), "after": ids(&tags) },
            });
            let entry = NewAuditEntry {
                id: self.new_id(now),
                team_id,
                heap_id: Some(item.heap_id),
                actor: principal.user_id,
                action: AuditAction::TagMerged,
                entity_type: EntityType::Item,
                entity_id: *item.id.as_uuid(),
                diff,
                run_id: None,
                created_at: now,
            };
            store::audit::insert_audit_entry(txn, &entry).await?;
        }

        // A heap tag is named only in its own heap's parameters, views and tag order.
        let reached: BTreeSet<HeapId> = reach.iter().map(|entry| entry.heap.id).collect();
        for heap in heaps.iter().filter(|heap| reached.contains(&heap.id)) {
            for parameter in store::parameters::list_parameters(txn, heap.id).await? {
                let Some(tags) = replaced(&parameter.tags) else {
                    continue;
                };
                let edited = domain::parameter::Parameter {
                    tags,
                    ..parameter.clone()
                };
                store::parameters::update_parameter(txn, &edited, parameter.version, now)
                    .await?
                    .ok_or_else(|| locked_row_changed("parameters", &parameter.id.to_string()))?;
            }
            for mut view in store::saved_views::list_saved_views(txn, heap.id).await? {
                let Some(tags) = replaced(&view.list_view.filter.tags) else {
                    continue;
                };
                let expected = view.version;
                view.list_view.filter.tags = tags;
                store::saved_views::update_saved_view(txn, &view, expected)
                    .await?
                    .ok_or_else(|| locked_row_changed("saved_views", &view.id.to_string()))?;
            }
            if let Some(tag_order) = merged_tag_order(&heap.tag_order, team_tag_id, &merged) {
                let edited = Heap {
                    tag_order,
                    ..heap.clone()
                };
                store::heaps::update_heap(txn, &edited, heap.version)
                    .await?
                    .ok_or_else(|| locked_row_changed("heaps", &heap.id.to_string()))?;
            }
        }

        store::tags::mark_merged(txn, &merged_ids, team_tag_id).await?;
        let listed: Vec<serde_json::Value> = reach
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "heap": entry.heap.id.to_string(),
                    "tag": entry.tag.id.to_string(),
                    "name": entry.tag.name.as_str(),
                    "description": entry.tag.description.as_ref().map(|d| d.as_str()),
                    "items": entry.items,
                })
            })
            .collect();
        let entry = self.tag_audit_entry(
            team_id,
            None,
            principal,
            AuditAction::TagMerged,
            team_tag,
            serde_json::json!({ AuditField::Merged.code(): listed }),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
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

/// A row the merge holds the lock for changed under it, which the locks rule out.
fn locked_row_changed(table: &'static str, id: &str) -> ServiceError {
    StoreError::corrupt(table, id, "a row changed under the lock of its heap").into()
}

/// The IDs of the team's heaps, archived ones included, in heap ID order (see [`Services::begin_team_tag_write`]).
async fn team_heap_ids(db: &impl ConnectionTrait, team_id: TeamId) -> Result<Vec<HeapId>, ServiceError> {
    let mut ids: Vec<HeapId> = store::heaps::list_heaps_for_teams(db, &[team_id])
        .await?
        .into_iter()
        .map(|heap| heap.id)
        .collect();
    ids.sort();
    Ok(ids)
}

/// The team's lowest heap ID when it isn't `heap_id` itself, read before a heap tag write's transaction begins (see
/// [`authorize_tag_write`]). `None` for the lowest heap itself, and for a heap that doesn't exist. Heaps are never
/// deleted and never change team, and their IDs are UUIDv7, rising with the time they were made, so the lowest one
/// stays the lowest, as long as the servers' clocks agree to within the age of the team's first heap.
async fn lower_team_heap(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Option<HeapId>, ServiceError> {
    let Some(heap) = store::heaps::find_heap(db, heap_id).await? else {
        return Ok(None);
    };
    let lowest = team_heap_ids(db, heap.team_id).await?.into_iter().next();
    Ok(lowest.filter(|lowest| *lowest != heap_id))
}

/// For a write to a team's tags: locks each heap of the team (`heap_ids`, from [`team_heap_ids`]) in heap ID order,
/// then checks the caller may edit. Returns the heaps as read under their locks.
///
/// A merge writes items, parameters, views and heaps in every heap of the team, so it waits for the item writes in
/// flight and holds the others off, each of which holds its heap row. The locks come before every other read, so
/// the plain reads after them see what a competing write committed (on MySQL a transaction's first plain read fixes
/// its snapshot). A non-member gets [`ServiceError::NotFound`], as for a missing team.
async fn authorize_team_write(
    txn: &impl ConnectionTrait,
    principal: Principal,
    team_id: TeamId,
    heap_ids: &[HeapId],
) -> Result<(Membership, Vec<Heap>), ServiceError> {
    let mut heaps = Vec::with_capacity(heap_ids.len());
    for heap_id in heap_ids {
        heaps.push(
            store::heaps::lock_heap(txn, *heap_id)
                .await?
                .ok_or(ServiceError::NotFound)?,
        );
    }
    let membership = authorize_team(txn, principal, team_id, Permission::EditItems).await?;
    Ok((membership, heaps))
}

/// For a write to a heap's tags: locks the heap row, checks the caller may edit, and refuses an archived heap on the
/// row read under the lock.
///
/// `lower` is the team's lowest heap when that isn't this heap ([`lower_team_heap`]); it is locked shared first. A
/// team tag write locks every heap it read before its transaction exclusively, the lowest among them, so a heap tag
/// write and a team tag write of the heap's team meet there even when this heap was made after the team tag write
/// read the team's heaps, and can't take one name at once. The locks stay in ascending heap ID order. The locks come
/// first, so the plain reads after them (the name checks, which take no locking reads, since on MySQL those take gap
/// locks that deadlock tag writes of two teams) see what a competing write committed.
async fn authorize_tag_write(
    txn: &impl ConnectionTrait,
    principal: Principal,
    heap_id: HeapId,
    lower: Option<HeapId>,
) -> Result<HeapAccess, ServiceError> {
    if let Some(lower) = lower {
        store::heaps::lock_heap_shared(txn, lower).await?;
    }
    let heap = store::heaps::lock_heap(txn, heap_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let access = authorize_heap(txn, principal, heap_id, Permission::EditItems).await?;
    ensure_active(&heap)?;
    Ok(HeapAccess {
        heap,
        membership: access.membership,
    })
}

/// The heap tags in use of `heaps` that `name` would merge, with their item counts, and the token that names them.
/// Empty when no heap tag has the name.
async fn reach_of(db: &impl ConnectionTrait, heaps: &[Heap], name: &TagName) -> Result<MergeReach, ServiceError> {
    let heap_ids: Vec<HeapId> = heaps.iter().map(|heap| heap.id).collect();
    let tags = store::tags::list_heap_tags_named(db, &heap_ids, name).await?;
    let tag_ids: Vec<TagId> = tags.iter().map(|tag| tag.id).collect();
    let mut counts: BTreeMap<TagId, u32> = BTreeMap::new();
    for (_, tag) in store::items::list_active_tag_links(db, &tag_ids).await? {
        *counts.entry(tag).or_default() += 1;
    }
    let mut entries = Vec::with_capacity(tags.len());
    for tag in tags {
        // The query names heap tags of `heaps` only.
        let heap = match tag.owner {
            TagOwner::Heap(heap_id) => heaps.iter().find(|heap| heap.id == heap_id),
            TagOwner::Team(_) => None,
        }
        .ok_or_else(|| StoreError::corrupt("tags", &tag.id.to_string(), "a heap tag of no heap asked for"))?;
        entries.push(ReachEntry {
            items: counts.get(&tag.id).copied().unwrap_or(0),
            heap: heap.clone(),
            tag,
        });
    }
    entries.sort_by(|a, b| (a.heap.name.as_str(), a.heap.id).cmp(&(b.heap.name.as_str(), b.heap.id)));
    let text = reach_text(entries.iter().map(|entry| (entry.tag.id, entry.items)));
    // The first 8 bytes of the SHA-256: 16 hex digits, enough to tell one reach from another.
    let token = sha256_hex(&text).chars().take(16).collect();
    Ok(MergeReach { token, entries })
}

/// Refuses a write that would merge heap tags unless `confirm` is the token of exactly this reach. An empty reach
/// merges nothing, so it needs none.
fn ensure_confirmed(name: &TagName, reach: &MergeReach, confirm: Option<&str>) -> Result<(), ServiceError> {
    if reach.entries.is_empty() || confirm == Some(reach.token.as_str()) {
        return Ok(());
    }
    Err(ServiceError::TagMergeUnconfirmed {
        name: name.as_str().to_owned(),
        reach: reach.clone(),
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

/// Refuses a name another team tag has. `except` is the tag being renamed. Heap tags with the name are not a reason to
/// refuse: the write merges them.
async fn ensure_team_tag_name_free(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    name: &TagName,
    except: Option<TagId>,
) -> Result<(), ServiceError> {
    let existing = store::tags::find_team_tag_by_name(db, team_id, name).await?;
    if existing.is_some_and(|other| Some(other.id) != except) {
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
