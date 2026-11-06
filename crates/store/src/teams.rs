//! Teams and memberships.

use domain::team::{Membership, Role, Team};
use domain::text::Name;
use domain::{TeamId, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{team_members, teams};

pub async fn insert_team(db: &impl ConnectionTrait, team: &Team) -> Result<(), StoreError> {
    let id = id_to_db(team.id.as_uuid());
    let row = teams::ActiveModel {
        version: Set(version_to_db(team.version, "teams", &id)?),
        id: Set(id),
        name: Set(team.name.as_str().to_owned()),
        created_at: Set(team.created_at.as_millis()),
    };
    teams::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

pub async fn insert_membership(db: &impl ConnectionTrait, membership: &Membership) -> Result<(), StoreError> {
    let row = team_members::ActiveModel {
        team_id: Set(id_to_db(membership.team_id.as_uuid())),
        user_id: Set(id_to_db(membership.user_id.as_uuid())),
        role: Set(membership.role.code().to_owned()),
        joined_at: Set(membership.joined_at.as_millis()),
    };
    team_members::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// The caller's membership in `team_id`, or `None` if they aren't a member.
pub async fn find_membership(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    user_id: UserId,
) -> Result<Option<Membership>, StoreError> {
    let row = team_members::Entity::find_by_id((id_to_db(team_id.as_uuid()), id_to_db(user_id.as_uuid())))
        .one(db)
        .await?;
    row.map(to_membership).transpose()
}

/// Every team `user_id` belongs to, with their role in it, ordered by name.
pub async fn list_teams_for_user(db: &impl ConnectionTrait, user_id: UserId) -> Result<Vec<(Team, Role)>, StoreError> {
    let memberships = team_members::Entity::find()
        .filter(team_members::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .all(db)
        .await?
        .into_iter()
        .map(to_membership)
        .collect::<Result<Vec<_>, _>>()?;
    let team_ids = memberships.iter().map(|m| id_to_db(m.team_id.as_uuid()));
    let teams = teams::Entity::find()
        .filter(teams::Column::Id.is_in(team_ids))
        .order_by_asc(teams::Column::Name)
        .order_by_asc(teams::Column::Id)
        .all(db)
        .await?;

    teams
        .into_iter()
        .map(|row| {
            let team = to_team(row)?;
            let role = memberships
                .iter()
                .find(|m| m.team_id == team.id)
                .map(|m| m.role)
                .ok_or_else(|| StoreError::corrupt("teams", &team.id.to_string(), "membership vanished"))?;
            Ok((team, role))
        })
        .collect()
}

fn to_team(row: teams::Model) -> Result<Team, StoreError> {
    let read = RowReader {
        table: "teams",
        id: &row.id,
    };
    Ok(Team {
        id: TeamId::from_uuid(read.uuid(&row.id)?),
        name: Name::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        created_at: read.timestamp(row.created_at)?,
        version: read.version(row.version)?,
    })
}

fn to_membership(row: team_members::Model) -> Result<Membership, StoreError> {
    let read = RowReader {
        table: "team_members",
        id: &row.team_id,
    };
    Ok(Membership {
        team_id: TeamId::from_uuid(read.uuid(&row.team_id)?),
        user_id: UserId::from_uuid(read.uuid(&row.user_id)?),
        role: Role::from_code(&row.role).ok_or_else(|| read.corrupt(format!("unknown role {:?}", row.role)))?,
        joined_at: read.timestamp(row.joined_at)?,
    })
}
