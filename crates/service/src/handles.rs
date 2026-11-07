//! Finding teams, heaps, items and users by their handles (design/readable-ids.md), and picking free handles.
//!
//! Each lookup checks the caller is a member of the team first, and answers [`ServiceError::NotFound`] otherwise, as
//! for a handle nobody has, so handles can't be probed. The use case the caller then runs by ID checks its own
//! permission as usual.

use domain::handle::{Handle, ItemKey};
use domain::text::Name;
use domain::validation::Field;
use domain::{HeapId, ItemId, TeamId, UserId};
use store::ConnectionTrait;

use crate::{Principal, ServiceError, Services};

/// How many `-2`, `-3`… suffixes are tried before a derived handle counts as taken.
const MAX_SUFFIX: u32 = 100;

/// A heap found by its team's and its own slug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapRef {
    pub team_id: TeamId,
    pub heap_id: HeapId,
}

impl Services {
    /// The team with `slug`. Fails with [`ServiceError::NotFound`] unless the caller is a member.
    pub async fn resolve_team(&self, principal: Principal, slug: &Handle) -> Result<TeamId, ServiceError> {
        resolve_team(&self.db, principal, slug).await
    }

    /// The heap `heap` of the team `team`. Fails with [`ServiceError::NotFound`] unless the caller is a member.
    pub async fn resolve_heap(
        &self,
        principal: Principal,
        team: &Handle,
        heap: &Handle,
    ) -> Result<HeapRef, ServiceError> {
        let team_id = resolve_team(&self.db, principal, team).await?;
        let heap = store::heaps::find_heap_by_slug(&self.db, team_id, heap)
            .await?
            .ok_or(ServiceError::NotFound)?;
        Ok(HeapRef {
            team_id,
            heap_id: heap.id,
        })
    }

    /// The item with `key` in the heap `heap` of the team `team`. Fails with [`ServiceError::NotFound`] unless the
    /// caller is a member.
    pub async fn resolve_item(
        &self,
        principal: Principal,
        team: &Handle,
        heap: &Handle,
        key: ItemKey,
    ) -> Result<ItemId, ServiceError> {
        let heap = self.resolve_heap(principal, team, heap).await?;
        store::items::find_item_id_by_key(&self.db, heap.heap_id, key)
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// The team `team` and the user with `username`, who needn't be a member. Fails with
    /// [`ServiceError::NotFound`] unless the caller is a member of the team, or when there is no such user.
    pub async fn resolve_member(
        &self,
        principal: Principal,
        team: &Handle,
        username: &Handle,
    ) -> Result<(TeamId, UserId), ServiceError> {
        let team_id = resolve_team(&self.db, principal, team).await?;
        let user = store::users::find_user_by_username(&self.db, username)
            .await?
            .ok_or(ServiceError::NotFound)?;
        Ok((team_id, user.id))
    }
}

async fn resolve_team(db: &impl ConnectionTrait, principal: Principal, slug: &Handle) -> Result<TeamId, ServiceError> {
    let team = store::teams::find_team_by_slug(db, slug)
        .await?
        .ok_or(ServiceError::NotFound)?;
    store::teams::find_membership(db, team.id, principal.user_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok(team.id)
}

/// A free slug for a new team called `name`: made from the name, with the first of `-2`, `-3`… that no team or
/// username has when the plain one is taken.
#[cfg(feature = "accounts")]
pub(crate) async fn free_team_slug(db: &impl ConnectionTrait, name: &Name) -> Result<Handle, ServiceError> {
    for candidate in candidates(name, "team")? {
        if store::teams::find_team_by_slug(db, &candidate).await?.is_none() {
            return Ok(candidate);
        }
    }
    Err(ServiceError::HandleTaken { field: Field::Slug })
}

/// A free slug for a new heap of `team_id` called `name`, as [`free_team_slug`] makes one among the team's heaps.
pub(crate) async fn free_heap_slug(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    name: &Name,
) -> Result<Handle, ServiceError> {
    for candidate in candidates(name, "heap")? {
        if store::heaps::find_heap_by_slug(db, team_id, &candidate)
            .await?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(ServiceError::HandleTaken { field: Field::Slug })
}

/// How many times a create with a derived slug starts again after losing that slug to a create running alongside.
const RACE_RETRIES: u32 = 3;

/// Runs `create` and, when it made its own slug (`derived`) and fails with [`ServiceError::HandleTaken`], runs it
/// again. Finding a free slug and inserting it aren't atomic: two creates of "Backlog" at once can both find
/// `backlog` free, and the unique index refuses the second. Run again, it sees the first one's slug and takes
/// `backlog-2`. A slug the caller chose isn't retried, as it really is taken.
pub(crate) async fn retry_derived_slug<T, F, Fut>(derived: bool, mut create: F) -> Result<T, ServiceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, ServiceError>>,
{
    let mut retries = 0;
    loop {
        match create().await {
            Err(ServiceError::HandleTaken { .. }) if derived && retries < RACE_RETRIES => {
                retries += 1;
                tracing::debug!(retries, "derived slug taken alongside, trying again");
            }
            result => return result,
        }
    }
}

/// The handle made from `name`, or from `fallback` when the name gives none, then the same with `-2` up to
/// `-{MAX_SUFFIX}`.
fn candidates(name: &Name, fallback: &str) -> Result<impl Iterator<Item = Handle>, ServiceError> {
    let base = Handle::derive(name.as_str())
        .or_else(|| Handle::derive(fallback))
        .ok_or(ServiceError::HandleTaken { field: Field::Slug })?;
    Ok(std::iter::once(base.clone()).chain((2..=MAX_SUFFIX).map(move |n| base.with_suffix(n))))
}
