//! The Data page: what agents spent and how items and runs came and went over a period, for a team or one heap.

use domain::data::{Buckets, DataFigures, Period};
use domain::handle::Handle;
use domain::permission::Permission;
use domain::{TeamId, ValidationError};
use jiff::tz::TimeZone;

use crate::access::authorize_team;
use crate::{Principal, ServiceError, Services};

impl Services {
    /// The figures of `period` for all the heaps of the team `team_id`, archived ones included, or for its heap
    /// `heap` only, bucketed by day or week in `tz`. An item counts in the heap it is in now, for all its past. Any
    /// member may ask. Fails with [`ServiceError::NotFound`] when `heap` isn't a heap of the team.
    pub async fn load_team_data(
        &self,
        principal: Principal,
        team_id: TeamId,
        heap: Option<&Handle>,
        period: Period,
        tz: &TimeZone,
    ) -> Result<DataFigures, ServiceError> {
        authorize_team(&self.db, principal, team_id, Permission::ViewTeam).await?;
        let mut heaps = store::heaps::list_heaps_for_teams(&self.db, &[team_id]).await?;
        if let Some(slug) = heap {
            heaps.retain(|heap| heap.slug == *slug);
            if heaps.is_empty() {
                return Err(ServiceError::NotFound);
            }
        }
        let heap_ids: Vec<_> = heaps.iter().map(|heap| heap.id).collect();

        let buckets = Buckets::new(self.now(), tz, period).ok_or(ValidationError::TimestampOutOfRange)?;
        let (from, to) = (buckets.from(), buckets.to());
        let runs = store::figures::list_runs_started(&self.db, &heap_ids, from, to).await?;
        let created = store::figures::list_items_created(&self.db, &heap_ids, from, to).await?;
        let closed = store::figures::list_items_closed(&self.db, &heap_ids, from, to).await?;
        Ok(buckets.count(&runs, &created, &closed))
    }
}
