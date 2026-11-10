//! A team's invite links (architecture.md, Data model).

use domain::invite_link::{InviteLink, InviteToken, Revocation};
use domain::{InviteLinkId, TeamId, Timestamp, UserId};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::invite_links;

/// Inserts a new link. Fails with [`StoreError::Duplicate`] on a token collision, which 256 random bits make a
/// bug rather than bad luck.
pub async fn insert_invite_link(db: &impl ConnectionTrait, link: &InviteLink) -> Result<(), StoreError> {
    let id = id_to_db(link.id.as_uuid());
    let row = invite_links::ActiveModel {
        version: Set(version_to_db(link.version, "invite_links", &id)?),
        id: Set(id),
        team_id: Set(id_to_db(link.team_id.as_uuid())),
        token: Set(link.token.expose().to_owned()),
        created_by: Set(id_to_db(link.created_by.as_uuid())),
        created_at: Set(link.created_at.as_millis()),
        revoked_at: Set(link.revocation.map(|r| r.at.as_millis())),
        revoked_by: Set(link.revocation.map(|r| id_to_db(r.by.as_uuid()))),
    };
    invite_links::Entity::insert(row).exec_without_returning(db).await?;
    Ok(())
}

/// Every link of `team_id`, ended and revoked ones too, newest first.
///
/// Size assumption: a team makes at most a few hundred links over its life (one a week is about 50 a year), so the
/// whole collection is read. If that stops holding, page the past links and keep the working ones whole.
pub async fn list_invite_links(db: &impl ConnectionTrait, team_id: TeamId) -> Result<Vec<InviteLink>, StoreError> {
    invite_links::Entity::find()
        .filter(invite_links::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .order_by_desc(invite_links::Column::CreatedAt)
        .order_by_desc(invite_links::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(to_invite_link)
        .collect()
}

/// The link `id` if it belongs to `team_id`, or `None`. A link of another team is not found.
pub async fn find_invite_link(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    id: InviteLinkId,
) -> Result<Option<InviteLink>, StoreError> {
    invite_links::Entity::find_by_id(id_to_db(id.as_uuid()))
        .filter(invite_links::Column::TeamId.eq(id_to_db(team_id.as_uuid())))
        .one(db)
        .await?
        .map(to_invite_link)
        .transpose()
}

/// Marks `link` revoked by `by` at `at` and bumps its version, if it is still unrevoked at the version read.
/// Returns whether the update applied.
pub async fn revoke_invite_link(
    db: &impl ConnectionTrait,
    link: &InviteLink,
    at: Timestamp,
    by: UserId,
) -> Result<bool, StoreError> {
    let id = id_to_db(link.id.as_uuid());
    let row = invite_links::ActiveModel {
        revoked_at: Set(Some(at.as_millis())),
        revoked_by: Set(Some(id_to_db(by.as_uuid()))),
        version: Set(version_to_db(link.version.next(), "invite_links", &id)?),
        ..Default::default()
    };
    let result = invite_links::Entity::update_many()
        .set(row)
        .filter(invite_links::Column::Id.eq(id.as_str()))
        .filter(invite_links::Column::RevokedAt.is_null())
        .filter(invite_links::Column::Version.eq(version_to_db(link.version, "invite_links", &id)?))
        .exec(db)
        .await?;
    Ok(result.rows_affected == 1)
}

fn to_invite_link(row: invite_links::Model) -> Result<InviteLink, StoreError> {
    let read = RowReader {
        table: "invite_links",
        id: &row.id,
    };
    let revocation = match (row.revoked_at, row.revoked_by.as_deref()) {
        (None, None) => None,
        (Some(at), Some(by)) => Some(Revocation {
            at: read.timestamp(at)?,
            by: UserId::from_uuid(read.uuid(by)?),
        }),
        _ => return Err(read.corrupt("revoked_at and revoked_by are not set together".to_owned())),
    };
    Ok(InviteLink {
        id: InviteLinkId::from_uuid(read.uuid(&row.id)?),
        team_id: TeamId::from_uuid(read.uuid(&row.team_id)?),
        token: InviteToken::new(row.token),
        created_by: UserId::from_uuid(read.uuid(&row.created_by)?),
        created_at: read.timestamp(row.created_at)?,
        revocation,
        version: read.version(row.version)?,
    })
}
