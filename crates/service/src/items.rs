//! Item use cases: the ranked list, the closed list, adding and editing.

use std::collections::{BTreeSet, HashMap, HashSet};

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::item::{Item, ItemPatch, ItemSummary, NewItem, StateKind};
use domain::list_view::{Group, ListView};
use domain::permission::{ItemAction, Permission, allowed_item_actions};
use domain::ranking::{CURRENT_FORMULA, Contributor, ItemForRanking, rank};
use domain::tag::Tag;
use domain::team::Role;
use domain::{Duration, HeapId, ItemId, TagId, Timestamp, Version};
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

pub use store::items::ClosedCursor;

use crate::access::{authorize_heap, authorize_heap_write};
use crate::audit::{describe_new_item, diff_items};
use crate::{Principal, ServiceError, Services};

/// A client may keep ranking at an `as_of` for at most this long. Older values are replaced by the current time.
pub const MAX_AS_OF_AGE: Duration = Duration::from_hours(24);

/// Largest page of closed items.
pub const MAX_CLOSED_PAGE: u64 = 200;

/// An item with its tags, its blocker, and what the caller may do with it.
#[derive(Debug)]
pub struct ItemView {
    pub item: Item,
    /// The item's tags, by name.
    pub tags: Vec<Tag>,
    /// The item it waits for, when it's blocked and one was picked.
    pub blocked_by: Option<ItemSummary>,
    pub allowed_actions: Vec<ItemAction>,
}

/// An item in the ranked list.
#[derive(Debug)]
pub struct RankedEntry {
    pub view: ItemView,
    /// 1-based position within its group, in the order the group is listed.
    pub position: u32,
    pub contributors: Vec<Contributor>,
}

/// The heap's non-closed items, ranked at `as_of`, then filtered, grouped and sorted by a [`ListView`].
#[derive(Debug)]
pub struct RankedList {
    pub heap: Heap,
    pub role: Role,
    /// The time the ranking used. Clients send it back to keep the order stable.
    pub as_of: Timestamp,
    /// Items in pinned states, shown above the rest.
    pub pinned: Vec<RankedEntry>,
    /// Items in shown states, including blocked items mixed in.
    pub ranked: Vec<RankedEntry>,
    /// Shown blocked items, when they're grouped.
    pub blocked: Vec<RankedEntry>,
    /// Items the filters keep but whose state is hidden.
    pub hidden_count: u32,
}

#[derive(Debug)]
pub struct ClosedPage {
    pub heap: Heap,
    pub items: Vec<ItemView>,
    /// Where the next page starts, or `None` on the last page.
    pub next: Option<ClosedCursor>,
}

impl Services {
    /// Ranks every non-closed item of the heap, then shows them as `view` says.
    ///
    /// `requested_as_of` freezes the time input so the order stays stable within a session. It's ignored when
    /// it's in the future or more than [`MAX_AS_OF_AGE`] old, and the current time is used instead. Every tag in
    /// the view's filter must belong to the heap, or it fails with [`ServiceError::TagNotInHeap`].
    pub async fn list_ranked(
        &self,
        principal: Principal,
        heap_id: HeapId,
        requested_as_of: Option<Timestamp>,
        view: &ListView,
    ) -> Result<RankedList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let now = self.now();
        let as_of = requested_as_of
            .filter(|t| *t <= now && now.since(*t) <= MAX_AS_OF_AGE)
            .unwrap_or(now);
        let states = [StateKind::Open, StateKind::InProgress, StateKind::Blocked];
        let mut items = store::items::list_items_in_states(&self.db, heap_id, &states).await?;
        let heap_tags = store::tags::list_tags(&self.db, heap_id).await?;
        ensure_tags_in_heap(&heap_tags, &view.filter.tags)?;

        // Every item is ranked, because a blocker inherits from what it blocks even when that is left out below.
        let ranking = {
            let inputs: Vec<ItemForRanking<'_>> = items.iter().map(ItemForRanking::from).collect();
            rank(CURRENT_FORMULA, &access.heap.policy, &inputs, as_of)
        };
        let item_count = items.len();
        let blockers = blocker_summaries(&self.db, heap_id, &items).await?;

        let role = access.membership.role;
        let (mut pinned, mut ranked, mut blocked) = (Vec::new(), Vec::new(), Vec::new());
        let mut hidden_count = 0_u32;
        for entry in ranking {
            let Some(index) = items.iter().position(|item| item.id == entry.item_id) else {
                continue;
            };
            let item = items.swap_remove(index);
            if !view.filter.matches(&item) {
                continue;
            }
            let group = match view.group_of(item.state.kind()) {
                Some(Group::Pinned) => &mut pinned,
                Some(Group::Ranked) => &mut ranked,
                Some(Group::Blocked) => &mut blocked,
                None => {
                    hidden_count = hidden_count.saturating_add(1);
                    continue;
                }
            };
            let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
            let tags = select_tags(&heap_tags, &item.tags);
            let blocked_by = item.blocked_by().and_then(|id| blockers.get(&id).cloned());
            group.push(RankedEntry {
                view: ItemView {
                    item,
                    tags,
                    blocked_by,
                    allowed_actions,
                },
                position: 0,
                contributors: entry.contributors,
            });
        }
        for (group, entries) in [
            (Group::Pinned, &mut pinned),
            (Group::Ranked, &mut ranked),
            (Group::Blocked, &mut blocked),
        ] {
            view.sort(group, entries, |entry| &entry.view.item);
            for (index, entry) in entries.iter_mut().enumerate() {
                entry.position = u32::try_from(index + 1).unwrap_or(u32::MAX);
            }
        }

        tracing::debug!(%heap_id, items = item_count, as_of = as_of.as_millis(), "ranked heap");
        Ok(RankedList {
            heap: access.heap,
            role,
            as_of,
            pinned,
            ranked,
            blocked,
            hidden_count,
        })
    }

    /// Closed items, most recently closed first.
    pub async fn list_closed(
        &self,
        principal: Principal,
        heap_id: HeapId,
        after: Option<ClosedCursor>,
        limit: u64,
    ) -> Result<ClosedPage, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let limit = limit.clamp(1, MAX_CLOSED_PAGE);
        // One extra row tells whether another page follows.
        let mut items = store::items::list_closed_items(&self.db, heap_id, after, limit + 1).await?;
        let has_more = items.len() as u64 > limit;
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = has_more
            .then(|| {
                items.last().and_then(|last| {
                    last.state
                        .closed_at()
                        .map(|closed_at| ClosedCursor { closed_at, id: last.id })
                })
            })
            .flatten();

        let heap_tags = store::tags::list_tags(&self.db, heap_id).await?;
        let now = self.now();
        let role = access.membership.role;
        let items = items
            .into_iter()
            .map(|item| {
                let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
                let tags = select_tags(&heap_tags, &item.tags);
                // Only blocked items have a blocker, and closed items aren't blocked.
                ItemView {
                    item,
                    tags,
                    blocked_by: None,
                    allowed_actions,
                }
            })
            .collect();
        Ok(ClosedPage {
            heap: access.heap,
            items,
            next,
        })
    }

    /// Adds an open item to the heap. Every tag must belong to the heap, or it fails with
    /// [`ServiceError::TagNotInHeap`].
    pub async fn create_item(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewItem,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap_write(&txn, principal, heap_id, Permission::EditItems).await?;
        let tags = load_heap_tags(&txn, heap_id, &fields.tags).await?;

        let now = self.now();
        let item = Item::new(ItemId::from_uuid(self.new_id(now)), heap_id, fields, now);
        store::items::insert_item(&txn, &item).await?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::ItemCreated,
            &item,
            describe_new_item(&item),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%heap_id, item_id = %item.id, user_id = %principal.user_id, "item created");
        let allowed_actions = allowed_item_actions(&item, &access.heap, access.membership.role, now);
        // New items are open, so they have no blocker.
        Ok(ItemView {
            item,
            tags,
            blocked_by: None,
            allowed_actions,
        })
    }

    pub async fn find_item(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let item = store::items::find_item(&self.db, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap(&self.db, principal, item.heap_id, Permission::ViewHeap).await?;
        let allowed_actions = allowed_item_actions(&item, &access.heap, access.membership.role, self.now());
        let tags = load_heap_tags(&self.db, item.heap_id, &item.tags).await?;
        let blocked_by = find_blocker(&self.db, &item).await?;
        Ok(ItemView {
            item,
            tags,
            blocked_by,
            allowed_actions,
        })
    }

    /// Applies a partial edit if the item is still at version `expected`. New tags must belong to the item's
    /// heap, or it fails with [`ServiceError::TagNotInHeap`]. A new blocker must be a non-closed item of the same
    /// heap that doesn't already wait for this one ([`ServiceError::BlockerNotInHeap`],
    /// [`ServiceError::BlockerClosed`], [`ServiceError::BlockingCycle`]).
    ///
    /// A patch that changes nothing succeeds and returns the item unchanged, without an audit entry.
    pub async fn update_item(
        &self,
        principal: Principal,
        item_id: ItemId,
        patch: ItemPatch,
        expected: Version,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let current = store::items::find_item(&txn, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap_write(&txn, principal, current.heap_id, Permission::EditItems).await?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        if let Some(tags) = &patch.tags {
            load_heap_tags(&txn, current.heap_id, tags).await?;
        }

        let now = self.now();
        let role = access.membership.role;
        let edited = current.apply_patch(&patch, now)?;
        // Checked after the patch, so an item that doesn't end up blocked gets `item_not_blocked` first.
        if let Some(blocker) = edited.as_ref().and_then(Item::blocked_by)
            && current.blocked_by() != Some(blocker)
        {
            check_blocker(&txn, &current, blocker).await?;
        }
        let Some(edited) = edited else {
            let allowed_actions = allowed_item_actions(&current, &access.heap, role, now);
            let tags = load_heap_tags(&txn, current.heap_id, &current.tags).await?;
            let blocked_by = find_blocker(&txn, &current).await?;
            return Ok(ItemView {
                item: current,
                tags,
                blocked_by,
                allowed_actions,
            });
        };
        let saved =
            store::items::update_item(&txn, &edited, expected, now)
                .await?
                .ok_or(ServiceError::VersionConflict {
                    expected,
                    current: expected.next(),
                })?;
        let entry = self.item_audit_entry(
            &access.heap,
            principal,
            AuditAction::ItemUpdated,
            &saved,
            diff_items(&current, &saved),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let tags = load_heap_tags(&txn, saved.heap_id, &saved.tags).await?;
        let blocked_by = find_blocker(&txn, &saved).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(heap_id = %saved.heap_id, item_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), "item updated");
        let allowed_actions = allowed_item_actions(&saved, &access.heap, role, now);
        Ok(ItemView {
            item: saved,
            tags,
            blocked_by,
            allowed_actions,
        })
    }

    fn item_audit_entry(
        &self,
        heap: &Heap,
        principal: Principal,
        action: AuditAction,
        item: &Item,
        diff: serde_json::Value,
        now: Timestamp,
    ) -> NewAuditEntry {
        NewAuditEntry {
            id: self.new_id(now),
            team_id: heap.team_id,
            heap_id: Some(heap.id),
            actor: principal.user_id,
            action,
            entity_type: EntityType::Item,
            entity_id: *item.id.as_uuid(),
            diff,
            created_at: now,
        }
    }
}

/// Checks that `item` may wait for `blocker`: a non-closed item of the same heap that doesn't wait, directly or
/// through others, for `item`.
///
/// The caller must hold the heap lock ([`store::heaps::lock_heap`]), so two writes that would close a cycle between
/// them can't both pass the check.
async fn check_blocker(db: &impl ConnectionTrait, item: &Item, blocker: ItemId) -> Result<(), ServiceError> {
    let mut link = store::items::find_item_summary_locking(db, item.heap_id, blocker)
        .await?
        .ok_or(ServiceError::BlockerNotInHeap { item_id: blocker })?;
    if link.state.kind().is_closed() {
        return Err(ServiceError::BlockerClosed { item_id: blocker });
    }
    let mut seen = HashSet::new();
    loop {
        if link.id == item.id {
            return Err(ServiceError::BlockingCycle { item_id: blocker });
        }
        // A cycle already in stored data that doesn't pass through `item` isn't this write's doing.
        if !seen.insert(link.id) {
            return Ok(());
        }
        let Some(next) = link.state.blocked_by() else {
            return Ok(());
        };
        match store::items::find_item_summary_locking(db, item.heap_id, next).await? {
            Some(found) => link = found,
            None => return Ok(()),
        }
    }
}

/// The item `item` waits for, if any.
async fn find_blocker(db: &impl ConnectionTrait, item: &Item) -> Result<Option<ItemSummary>, ServiceError> {
    let Some(id) = item.blocked_by() else {
        return Ok(None);
    };
    Ok(store::items::list_item_summaries(db, item.heap_id, [id]).await?.pop())
}

/// Every blocker of `items`, by ID. Blockers among `items` are taken from there; closed ones are loaded.
async fn blocker_summaries(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    items: &[Item],
) -> Result<HashMap<ItemId, ItemSummary>, ServiceError> {
    let loaded: HashMap<ItemId, &Item> = items.iter().map(|item| (item.id, item)).collect();
    let mut found = HashMap::new();
    let mut missing = BTreeSet::new();
    for id in items.iter().filter_map(Item::blocked_by) {
        match loaded.get(&id) {
            Some(item) => {
                found.insert(id, ItemSummary::from(*item));
            }
            None => {
                missing.insert(id);
            }
        }
    }
    for summary in store::items::list_item_summaries(db, heap_id, missing).await? {
        found.insert(summary.id, summary);
    }
    Ok(found)
}

/// The tags `ids` names, by name. Fails with [`ServiceError::TagNotInHeap`] if any of them isn't a tag of
/// `heap_id`, which also covers IDs that don't exist at all.
async fn load_heap_tags(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: &BTreeSet<TagId>,
) -> Result<Vec<Tag>, ServiceError> {
    let tags = store::tags::list_tags_in_heap(db, heap_id, ids).await?;
    let found: BTreeSet<TagId> = tags.iter().map(|t| t.id).collect();
    if let Some(&tag_id) = ids.difference(&found).next() {
        return Err(ServiceError::TagNotInHeap { tag_id });
    }
    Ok(tags)
}

/// Fails with [`ServiceError::TagNotInHeap`] unless every tag in `ids` is one of `heap_tags`, the heap's tags.
///
/// Checked in memory, so a request with many IDs never becomes a query with as many parameters.
pub(crate) fn ensure_tags_in_heap(heap_tags: &[Tag], ids: &BTreeSet<TagId>) -> Result<(), ServiceError> {
    match ids.iter().find(|id| !heap_tags.iter().any(|tag| tag.id == **id)) {
        Some(&tag_id) => Err(ServiceError::TagNotInHeap { tag_id }),
        None => Ok(()),
    }
}

/// The tags among `heap_tags` (already sorted by name) that are in `ids`, in the same order.
fn select_tags(heap_tags: &[Tag], ids: &BTreeSet<TagId>) -> Vec<Tag> {
    heap_tags.iter().filter(|tag| ids.contains(&tag.id)).cloned().collect()
}
