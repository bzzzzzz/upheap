//! A team's invite links: admins make them, list them and revoke them. Joining through a link is not here yet.
//!
//! Making and revoking lock the team row first, like every team write (see `lock_team_for`), so two admins revoking
//! the same link at once give one revocation and one no-op. A link ending by time is not an act and has no audit
//! entry. The token is stored readable and never goes into a diff or a log line (architecture.md, Data model).

use std::collections::HashMap;

use domain::audit::{AuditAction, EntityType};
use domain::invite_link::{InviteLink, InviteLinkState, InviteToken, Revocation};
use domain::permission::{InviteLinkAction, Permission, allowed_invite_link_actions};
use domain::team::Role;
use domain::user::User;
use domain::{InviteLinkId, TeamId, Timestamp, UserId, Version};
use serde_json::json;
use store::{ConnectionTrait, StoreError, TransactionTrait};

use super::manage::lock_team_for;
use crate::access::authorize_team;
use crate::auth::random_token;
use crate::{Principal, ServiceError, Services};

/// An invite link as an admin of its team sees it.
#[derive(Debug)]
pub struct InviteLinkView {
    pub link: InviteLink,
    /// Where the link is in its life, as of the moment the view was made.
    pub state: InviteLinkState,
    /// The maker, by their name now, even if they have left the team.
    pub created_by: User,
    pub revoked_by: Option<User>,
    pub allowed_actions: Vec<InviteLinkAction>,
}

impl Services {
    /// Makes a link that works for a week. Admins only. Fails with [`domain::team::TeamError::PersonalTeam`] on a
    /// personal team.
    pub async fn create_invite_link(
        &self,
        principal: Principal,
        team_id: TeamId,
    ) -> Result<InviteLinkView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let locked = lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        locked.team.ensure_shared()?;

        let now = self.now();
        let link = InviteLink {
            id: InviteLinkId::from_uuid(self.new_id(now)),
            team_id,
            token: InviteToken::new(random_token()),
            created_by: principal.user_id,
            created_at: now,
            revocation: None,
            version: Version::INITIAL,
        };
        store::invite_links::insert_invite_link(&txn, &link).await?;
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::InviteLinkCreated,
            EntityType::InviteLink,
            *link.id.as_uuid(),
            json!({ "ends_at": link.ends_at().as_millis() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let view = load_views(&txn, vec![link], Role::Admin, now)
            .await?
            .pop()
            .ok_or(ServiceError::NotFound)?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, invite_link_id = %view.link.id, user_id = %principal.user_id, "invite link made");
        Ok(view)
    }

    /// Every link of the team, made and past, newest first. Admins only. A personal team has none.
    pub async fn list_invite_links(
        &self,
        principal: Principal,
        team_id: TeamId,
    ) -> Result<Vec<InviteLinkView>, ServiceError> {
        let membership = authorize_team(&self.db, principal, team_id, Permission::ManageTeam).await?;
        let links = store::invite_links::list_invite_links(&self.db, team_id).await?;
        load_views(&self.db, links, membership.role, self.now()).await
    }

    /// Stops a working link at once. Admins of the team only, whoever made the link. A link that is revoked or has
    /// ended already is returned as it is, without an audit entry. Fails with [`ServiceError::NotFound`] when the
    /// team has no such link.
    pub async fn revoke_invite_link(
        &self,
        principal: Principal,
        team_id: TeamId,
        link_id: InviteLinkId,
    ) -> Result<InviteLinkView, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        lock_team_for(&txn, principal, team_id, Permission::ManageTeam).await?;
        let link = store::invite_links::find_invite_link(&txn, team_id, link_id)
            .await?
            .ok_or(ServiceError::NotFound)?;

        let now = self.now();
        if !link.is_working(now) {
            return load_one(&txn, link, now).await;
        }
        if !store::invite_links::revoke_invite_link(&txn, &link, now, principal.user_id).await? {
            // The link changed after it was read, which the team lock rules out; report it as it is now.
            let current = store::invite_links::find_invite_link(&txn, team_id, link_id)
                .await?
                .ok_or(ServiceError::NotFound)?;
            return load_one(&txn, current, now).await;
        }
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::InviteLinkRevoked,
            EntityType::InviteLink,
            *link.id.as_uuid(),
            json!({ "state": { "before": InviteLinkState::Working.code(), "after": InviteLinkState::Revoked.code() } }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let revoked = InviteLink {
            revocation: Some(Revocation {
                at: now,
                by: principal.user_id,
            }),
            version: link.version.next(),
            ..link
        };
        let view = load_one(&txn, revoked, now).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, invite_link_id = %link_id, user_id = %principal.user_id, "invite link revoked");
        Ok(view)
    }
}

async fn load_one(db: &impl ConnectionTrait, link: InviteLink, now: Timestamp) -> Result<InviteLinkView, ServiceError> {
    // Only admins reach here, so the viewer's role is admin.
    load_views(db, vec![link], Role::Admin, now)
        .await?
        .pop()
        .ok_or(ServiceError::NotFound)
}

/// `links` in the same order as `role` sees them at `now`, with their makers and revokers read in one query.
async fn load_views(
    db: &impl ConnectionTrait,
    links: Vec<InviteLink>,
    role: Role,
    now: Timestamp,
) -> Result<Vec<InviteLinkView>, ServiceError> {
    let mut ids: Vec<UserId> = links
        .iter()
        .flat_map(|link| [Some(link.created_by), link.revocation.map(|r| r.by)])
        .flatten()
        .collect();
    ids.sort();
    ids.dedup();
    let users: HashMap<UserId, User> = store::users::list_users(db, &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user))
        .collect();
    links
        .into_iter()
        .map(|link| {
            let created_by = find_user(&users, link.created_by)?;
            let revoked_by = link.revocation.map(|r| find_user(&users, r.by)).transpose()?;
            Ok(InviteLinkView {
                state: link.state(now),
                allowed_actions: allowed_invite_link_actions(&link, role, now),
                created_by,
                revoked_by,
                link,
            })
        })
        .collect()
}

fn find_user(users: &HashMap<UserId, User>, id: UserId) -> Result<User, ServiceError> {
    // The foreign keys on invite_links guarantee the user rows.
    users.get(&id).cloned().ok_or_else(|| {
        StoreError::CorruptRow {
            table: "invite_links",
            id: id.to_string(),
            problem: "link names a user that does not exist".to_owned(),
        }
        .into()
    })
}
