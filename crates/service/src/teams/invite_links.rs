//! A team's invite links: admins make them, list them and revoke them, and anyone holding one can open it and a
//! signed-in user can join the team through it.
//!
//! Making and revoking lock the team row first, like every team write (see `lock_team_for`), so two admins revoking
//! the same link at once give one revocation and one no-op. A link ending by time is not an act and has no audit
//! entry. The token is stored readable and never goes into a diff or a log line (architecture.md, Data model).
//!
//! A person removed from a team can't join through a link made before the removal. The membership row is deleted on
//! removal, so the removal is remembered by the team's `member_removed` audit entry for that person: the newest one
//! is read when someone opens or joins through a link (architecture.md, Data model).
//!
//! Joining reads the link by its token before the transaction begins, only to learn which team to lock. On MySQL the
//! first plain read of a transaction fixes its snapshot, so reading the link inside the transaction before the team
//! lock would hide a revocation that commits while the join waits for the lock (see `lock_team_for`). The link is
//! read again after the lock, and that read decides.

use std::collections::HashMap;

use domain::audit::{AuditAction, EntityType};
use domain::invite_link::{InviteLink, InviteLinkState, InviteToken, Revocation};
use domain::permission::{InviteLinkAction, Permission, allowed_invite_link_actions};
use domain::team::{Membership, Role, Team};
use domain::user::User;
use domain::{InviteLinkId, TeamId, Timestamp, UserId, Version};
use serde_json::json;
use store::audit::{AuditQuery, AuditScope};
use store::{ConnectionTrait, StoreError, TransactionTrait};

use super::manage::{MemberAdded, lock_team_for};
use super::{TeamDetail, load_team_detail};
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

/// An invite link as someone holding it sees it: what the team and the maker are called, and where the link stands.
#[derive(Debug)]
pub struct InvitationView {
    pub link: InviteLink,
    /// Where the link is in its life, as of the moment the view was made.
    pub state: InviteLinkState,
    pub team: Team,
    /// The maker, by their name now, even if they have left the team.
    pub created_by: User,
    /// The caller's standing in the team; `None` when the caller isn't signed in.
    pub viewer: Option<InviteViewer>,
}

/// A signed-in person's standing in the team a link is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InviteViewer {
    /// Not in the team, and free to join through the link if it works.
    NotMember,
    /// In the team, as member or admin.
    Member,
    /// Not in the team, and removed from it after the link was made, so the link won't admit them.
    Removed,
}

impl Services {
    /// Opens a link for whoever holds its token, signed in (`principal`) or not. There is no permission to check
    /// beyond holding the token: it is the one place a person outside a team sees its name (product.md, Invite
    /// links). Reads only: no audit entry, no lock. Fails with [`ServiceError::NotFound`] for an unknown token.
    pub async fn open_invite(
        &self,
        principal: Option<Principal>,
        token: &InviteToken,
    ) -> Result<InvitationView, ServiceError> {
        let link = store::invite_links::find_invite_link_by_token(&self.db, token)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let team = store::teams::find_team(&self.db, link.team_id).await?.ok_or_else(|| {
            missing(
                "invite_links",
                link.id.to_string(),
                "link names a team that does not exist",
            )
        })?;
        let created_by = store::users::find_user(&self.db, link.created_by)
            .await?
            .ok_or_else(|| {
                missing(
                    "invite_links",
                    link.id.to_string(),
                    "link names a user that does not exist",
                )
            })?;
        let viewer = match principal {
            Some(principal) => Some(load_viewer(&self.db, &link, principal).await?),
            None => None,
        };
        Ok(InvitationView {
            state: link.state(self.now()),
            link,
            team,
            created_by,
            viewer,
        })
    }

    /// Makes `principal` a member of the link's team. Anyone signed in may try; the link decides.
    ///
    /// Someone already in the team, member or admin, is left as they are and gets [`MemberAdded::AlreadyMember`],
    /// whatever the link's state. Otherwise the link must be working at this moment, and must not predate the
    /// person's last removal from the team. Fails with [`ServiceError::NotFound`] for an unknown token, and with
    /// [`ServiceError::InviteLink`] when the link ended, was revoked, or predates a removal.
    pub async fn join_team_by_invite(
        &self,
        principal: Principal,
        token: &InviteToken,
    ) -> Result<(TeamDetail, MemberAdded), ServiceError> {
        // Only to learn which team to lock: see the module doc.
        let found = store::invite_links::find_invite_link_by_token(&self.db, token)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let (team_id, link_id) = (found.team_id, found.id);

        let txn = self.db.begin().await.map_err(StoreError::from)?;
        // Not `lock_team_for`: the caller is not a member yet, so its membership check would refuse them.
        let team = store::teams::lock_team(&txn, team_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let members = store::teams::list_memberships(&txn, team_id).await?;
        team.ensure_shared()?;
        if members.iter().any(|m| m.user_id == principal.user_id) {
            let detail = load_team_detail(&txn, principal, team).await?;
            return Ok((detail, MemberAdded::AlreadyMember));
        }

        // After the lock, so it sees a revocation committed before the lock was taken.
        let link = store::invite_links::find_invite_link(&txn, team_id, link_id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let now = self.now();
        let removed_at = last_removed_at(&txn, team_id, principal.user_id).await?;
        link.check_join(now, removed_at)?;

        let membership = Membership {
            team_id,
            user_id: principal.user_id,
            role: Role::Member,
            joined_at: now,
        };
        store::teams::insert_membership(&txn, &membership).await?;
        let entry = self.team_audit_entry(
            principal,
            team_id,
            AuditAction::MemberJoined,
            EntityType::InviteLink,
            *link.id.as_uuid(),
            json!({ "role": Role::Member.code() }),
            now,
        );
        store::audit::insert_audit_entry(&txn, &entry).await?;
        let detail = load_team_detail(&txn, principal, team).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, invite_link_id = %link.id, user_id = %principal.user_id, "joined through invite link");
        Ok((detail, MemberAdded::Added))
    }

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
        let view = load_one(&txn, link, now).await?;
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

/// Where `principal` stands in the link's team, for the page that opens the link.
async fn load_viewer(
    db: &impl ConnectionTrait,
    link: &InviteLink,
    principal: Principal,
) -> Result<InviteViewer, ServiceError> {
    if store::teams::find_membership(db, link.team_id, principal.user_id)
        .await?
        .is_some()
    {
        return Ok(InviteViewer::Member);
    }
    let removed_at = last_removed_at(db, link.team_id, principal.user_id).await?;
    Ok(if link.admits_after_removal(removed_at) {
        InviteViewer::NotMember
    } else {
        InviteViewer::Removed
    })
}

/// When `user_id` was last removed from the team by an admin, or `None` if never. Leaving by choice is a different
/// action (`member_left`) and doesn't count. The newest entry is first, and membership entries have no heap, so the
/// team scope includes them.
async fn last_removed_at(
    db: &impl ConnectionTrait,
    team_id: TeamId,
    user_id: UserId,
) -> Result<Option<Timestamp>, ServiceError> {
    let newest = store::audit::list_audit_entries(
        db,
        AuditQuery {
            scope: AuditScope::Team(team_id),
            entity_id: Some(*user_id.as_uuid()),
            actions: &[AuditAction::MemberRemoved],
            after: None,
            limit: 1,
        },
    )
    .await?;
    Ok(newest.first().map(|entry| entry.created_at))
}

fn missing(table: &'static str, id: String, problem: &str) -> ServiceError {
    StoreError::CorruptRow {
        table,
        id,
        problem: problem.to_owned(),
    }
    .into()
}
