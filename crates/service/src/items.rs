//! Item use cases: the ranked list, the closed list, adding and editing.

use domain::audit::{AuditAction, EntityType};
use domain::heap::Heap;
use domain::item::{Item, ItemPatch, NewItem, StateKind};
use domain::permission::{ItemAction, Permission, allowed_item_actions};
use domain::ranking::{CURRENT_FORMULA, Contributor, ItemForRanking, rank};
use domain::team::Role;
use domain::{Duration, HeapId, ItemId, Timestamp, Version};
use store::audit::NewAuditEntry;
use store::{StoreError, TransactionTrait};

pub use store::items::ClosedCursor;

use crate::access::{authorize_heap, ensure_active};
use crate::audit::{describe_new_item, diff_items};
use crate::{Principal, ServiceError, Services};

/// A client may keep ranking at an `as_of` for at most this long. Older values are replaced by the current time.
pub const MAX_AS_OF_AGE: Duration = Duration::from_hours(24);

/// Largest page of closed items.
pub const MAX_CLOSED_PAGE: u64 = 200;

/// An item with what the caller may do with it.
#[derive(Debug)]
pub struct ItemView {
    pub item: Item,
    pub allowed_actions: Vec<ItemAction>,
}

/// An item in the ranked list.
#[derive(Debug)]
pub struct RankedEntry {
    pub view: ItemView,
    /// 1-based position within its group.
    pub position: u32,
    pub contributors: Vec<Contributor>,
}

/// The heap's non-closed items, ranked at `as_of` and grouped by state.
#[derive(Debug)]
pub struct RankedList {
    pub heap: Heap,
    pub role: Role,
    /// The time the ranking used. Clients send it back to keep the order stable.
    pub as_of: Timestamp,
    /// In-progress items, pinned above the rest.
    pub pinned: Vec<RankedEntry>,
    /// Open items.
    pub ranked: Vec<RankedEntry>,
    /// Blocked items, which clients hide by default.
    pub blocked: Vec<RankedEntry>,
}

#[derive(Debug)]
pub struct ClosedPage {
    pub heap: Heap,
    pub items: Vec<ItemView>,
    /// Where the next page starts, or `None` on the last page.
    pub next: Option<ClosedCursor>,
}

impl Services {
    /// Ranks every non-closed item of the heap.
    ///
    /// `requested_as_of` freezes the time input so the order stays stable within a session. It's ignored when
    /// it's in the future or more than [`MAX_AS_OF_AGE`] old, and the current time is used instead.
    pub async fn list_ranked(
        &self,
        principal: Principal,
        heap_id: HeapId,
        requested_as_of: Option<Timestamp>,
    ) -> Result<RankedList, ServiceError> {
        let access = authorize_heap(&self.db, principal, heap_id, Permission::ViewHeap).await?;
        let now = self.now();
        let as_of = requested_as_of
            .filter(|t| *t <= now && now.since(*t) <= MAX_AS_OF_AGE)
            .unwrap_or(now);
        let states = [StateKind::Open, StateKind::InProgress, StateKind::Blocked];
        let mut items = store::items::list_items_in_states(&self.db, heap_id, &states).await?;

        let inputs: Vec<ItemForRanking> = items.iter().map(ItemForRanking::from).collect();
        let ranking = rank(CURRENT_FORMULA, &access.heap.policy, &inputs, as_of);

        let role = access.membership.role;
        let (mut pinned, mut ranked, mut blocked) = (Vec::new(), Vec::new(), Vec::new());
        for entry in ranking {
            let Some(index) = items.iter().position(|item| item.id == entry.item_id) else {
                continue;
            };
            let item = items.swap_remove(index);
            let group = match item.state.kind() {
                StateKind::InProgress => &mut pinned,
                StateKind::Blocked => &mut blocked,
                StateKind::Open | StateKind::Done | StateKind::WontDo => &mut ranked,
            };
            let position = u32::try_from(group.len() + 1).unwrap_or(u32::MAX);
            let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
            group.push(RankedEntry {
                view: ItemView { item, allowed_actions },
                position,
                contributors: entry.contributors,
            });
        }

        tracing::debug!(%heap_id, items = inputs.len(), as_of = as_of.as_millis(), "ranked heap");
        Ok(RankedList {
            heap: access.heap,
            role,
            as_of,
            pinned,
            ranked,
            blocked,
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

        let now = self.now();
        let role = access.membership.role;
        let items = items
            .into_iter()
            .map(|item| {
                let allowed_actions = allowed_item_actions(&item, &access.heap, role, now);
                ItemView { item, allowed_actions }
            })
            .collect();
        Ok(ClosedPage {
            heap: access.heap,
            items,
            next,
        })
    }

    /// Adds an open item to the heap.
    pub async fn create_item(
        &self,
        principal: Principal,
        heap_id: HeapId,
        fields: NewItem,
    ) -> Result<ItemView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let access = authorize_heap(&txn, principal, heap_id, Permission::EditItems).await?;
        ensure_active(&access.heap)?;

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
        Ok(ItemView { item, allowed_actions })
    }

    pub async fn find_item(&self, principal: Principal, item_id: ItemId) -> Result<ItemView, ServiceError> {
        let item = store::items::find_item(&self.db, item_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let access = authorize_heap(&self.db, principal, item.heap_id, Permission::ViewHeap).await?;
        let allowed_actions = allowed_item_actions(&item, &access.heap, access.membership.role, self.now());
        Ok(ItemView { item, allowed_actions })
    }

    /// Applies a partial edit if the item is still at version `expected`.
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
        let access = authorize_heap(&txn, principal, current.heap_id, Permission::EditItems).await?;
        ensure_active(&access.heap)?;
        if current.version != expected {
            return Err(ServiceError::VersionConflict {
                expected,
                current: current.version,
            });
        }

        let now = self.now();
        let role = access.membership.role;
        let Some(edited) = current.apply_patch(&patch, now)? else {
            let allowed_actions = allowed_item_actions(&current, &access.heap, role, now);
            return Ok(ItemView {
                item: current,
                allowed_actions,
            });
        };
        let saved = store::items::update_item(&txn, &edited, expected)
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
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(heap_id = %saved.heap_id, item_id = %saved.id, user_id = %principal.user_id, version = saved.version.get(), "item updated");
        let allowed_actions = allowed_item_actions(&saved, &access.heap, role, now);
        Ok(ItemView {
            item: saved,
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
