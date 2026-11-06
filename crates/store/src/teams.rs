//! Teams and memberships.

use domain::team::{Membership, Role, Team};
use domain::text::Name;
use domain::{TeamId, UserId, Version};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::{team_members, teams};

/// Inserts a new team. Its members are inserted separately ([`insert_membership`]).
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

/// The team with `id`, or `None` if there is none. Doesn't check who is asking; the service does.
pub async fn find_team(db: &impl ConnectionTrait, id: TeamId) -> Result<Option<Team>, StoreError> {
    let row = teams::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_team).transpose()
}

/// Reads the team and locks its row until the transaction ends, so membership changes in one team run one at a
/// time and the last-admin check sees every change that committed before it. See [`crate::heaps::lock_heap`] for
/// how this works on SQLite.
pub async fn lock_team(db: &impl ConnectionTrait, id: TeamId) -> Result<Option<Team>, StoreError> {
    let row = teams::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    row.map(to_team).transpose()
}

/// Writes the name if the stored row still has version `expected`, and returns the team with its new version.
/// Returns `None` when the stored version differs.
pub async fn update_team(
    db: &impl ConnectionTrait,
    team: &Team,
    expected: Version,
) -> Result<Option<Team>, StoreError> {
    let id = id_to_db(team.id.as_uuid());
    let saved = Team {
        version: expected.next(),
        ..team.clone()
    };
    let row = teams::ActiveModel {
        name: Set(saved.name.as_str().to_owned()),
        version: Set(version_to_db(saved.version, "teams", &id)?),
        ..Default::default()
    };
    let result = teams::Entity::update_many()
        .set(row)
        .filter(teams::Column::Id.eq(id.as_str()))
        .filter(teams::Column::Version.eq(version_to_db(expected, "teams", &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

/// Inserts a membership. Fails with [`StoreError::Duplicate`] when the user is already a member.
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

/// Every membership of `team_id`, in the order members joined.
pub async fn list_memberships(db: &impl ConnectionTrait, team_id: TeamId) -> Result<Vec<Membership>, StoreError> {
    team_members::Entity::find()
        .filter(team_members::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .order_by_asc(team_members::Column::JoinedAt)
        .order_by_asc(team_members::Column::UserId)
        .all(db)
        .await?
        .into_iter()
        .map(to_membership)
        .collect()
}

/// Sets the member's role. Returns `false` when `user_id` isn't a member of `team_id`.
pub async fn update_role(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    user_id: UserId,
    role: Role,
) -> Result<bool, StoreError> {
    let row = team_members::ActiveModel {
        role: Set(role.code().to_owned()),
        ..Default::default()
    };
    let result = team_members::Entity::update_many()
        .set(row)
        .filter(team_members::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .filter(team_members::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

/// Removes the membership. Returns `false` when there was none.
///
/// The one `DELETE` of the application: a membership is a relationship row, and its history lives in the audit
/// log (`member_removed`, `member_left`).
pub async fn delete_membership(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    user_id: UserId,
) -> Result<bool, StoreError> {
    let result = team_members::Entity::delete_many()
        .filter(team_members::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .filter(team_members::Column::UserId.eq(id_to_db(user_id.as_uuid())))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
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
