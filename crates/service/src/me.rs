//! The signed-in user and what they can reach.

use domain::heap::Heap;
use domain::team::{Role, Team};
use domain::user::User;

use crate::{Principal, ServiceError, Services};

/// The caller, their teams and each team's heaps.
#[derive(Debug)]
pub struct MeView {
    pub user: User,
    pub teams: Vec<TeamView>,
}

#[derive(Debug)]
pub struct TeamView {
    pub team: Team,
    pub role: Role,
    pub heaps: Vec<Heap>,
}

impl Services {
    pub async fn load_me(&self, principal: Principal) -> Result<MeView, ServiceError> {
        let user = store::users::find_user(&self.db, principal.user_id)
            .await?
            .ok_or(ServiceError::Unauthenticated)?;
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let team_ids: Vec<_> = teams.iter().map(|(team, _)| team.id).collect();
        let mut heaps = store::heaps::list_heaps_for_teams(&self.db, &team_ids).await?;

        let teams = teams
            .into_iter()
            .map(|(team, role)| {
                let (own, rest): (Vec<Heap>, Vec<Heap>) = heaps.drain(..).partition(|heap| heap.team_id == team.id);
                heaps = rest;
                TeamView { team, role, heaps: own }
            })
            .collect();
        Ok(MeView { user, teams })
    }
}
