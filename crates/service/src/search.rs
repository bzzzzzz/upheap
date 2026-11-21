//! Search across heaps: the items of every heap the caller may view, found by title or key, open and closed. The
//! mechanism is in architecture.md, Search.

use std::collections::HashMap;

use domain::handle::ItemKey;
use domain::heap::Heap;
use domain::item::ItemSummary;
use domain::team::Team;

use crate::{Principal, ServiceError, Services};

/// Most rows [`Services::search_items`] returns.
pub const MAX_SEARCH_ROWS: usize = 50;

/// An item that matched, with its heap and team.
#[derive(Debug)]
pub struct SearchHit {
    pub item: ItemSummary,
    pub heap: Heap,
    pub team: Team,
}

/// The items a search found, best first.
#[derive(Debug)]
pub struct SearchResults {
    pub rows: Vec<SearchHit>,
    /// Whether more items matched than the rows hold.
    pub truncated: bool,
}

impl Services {
    /// The items, in any state, of the active heaps of the caller's teams where their role may view the heap, whose
    /// title contains `text` (ignoring case, `%` and `_` as themselves) or whose key is `text`. An item with exactly
    /// that key comes first; then open items, newest created first; then closed items, most recently closed first.
    /// At most [`MAX_SEARCH_ROWS`] rows are returned, and `truncated` says whether more matched. Read-only; any
    /// signed-in user may ask, and nobody sees a heap outside their teams.
    /// How the match folds case and what size it holds for: [`store::items::search_items`].
    pub async fn search_items(&self, principal: Principal, text: &str) -> Result<SearchResults, ServiceError> {
        self.search_items_capped(principal, text, MAX_SEARCH_ROWS).await
    }

    /// [`search_items`](Self::search_items) with at most `max_rows` rows in place of [`MAX_SEARCH_ROWS`], so the cap
    /// can be tested without a row for each of its rows.
    pub async fn search_items_capped(
        &self,
        principal: Principal,
        text: &str,
        max_rows: usize,
    ) -> Result<SearchResults, ServiceError> {
        let viewable = self.list_viewable_heaps(principal).await?;
        let heap_ids: Vec<_> = viewable.iter().map(|viewable| viewable.heap.id).collect();
        let owners: HashMap<_, _> = viewable
            .into_iter()
            .map(|viewable| (viewable.heap.id, (viewable.heap, viewable.team)))
            .collect();
        let key = ItemKey::parse(&text.to_lowercase()).ok();
        let limit = u64::try_from(max_rows.saturating_add(1)).unwrap_or(u64::MAX);
        let mut items = store::items::search_items(&self.db, &heap_ids, key, text, limit).await?;
        let truncated = items.len() > max_rows;
        items.truncate(max_rows);
        let rows: Vec<_> = items
            .iter()
            .filter_map(|item| {
                owners.get(&item.heap_id).map(|(heap, team)| SearchHit {
                    item: ItemSummary::from(item),
                    heap: heap.clone(),
                    team: team.clone(),
                })
            })
            .collect();
        tracing::debug!(
            user_id = %principal.user_id,
            heaps = heap_ids.len(),
            rows = rows.len(),
            truncated,
            "searched items across heaps"
        );
        Ok(SearchResults { rows, truncated })
    }
}
