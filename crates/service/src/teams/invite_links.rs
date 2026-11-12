//! A team's invite links: admins make them, list them and revoke them, and anyone holding one can open it and a
//! signed-in user can join the team through it. A person without an account creates one through it, and joins in
//! the same act, whether registration is open or closed.
//!
//! Making and revoking lock the team row first, like every team write (see `lock_team_for`), so two admins revoking
//! the same link at once give one revocation and one no-op. A link ending by time is not an act and has no audit
//! entry. The token is stored readable and never goes into a diff or a log line (architecture.md, Data model).
//!
//! A person removed from a team can't join through a link made before the removal. The membership row is deleted on
//! removal, so the removal is remembered by the team's `member_removed` audit entry for that person: the newest one
//! is read when someone opens or joins through a link (architecture.md, Data model).
//!
//! Joining, and creating an account through a link, read the link by its token before the transaction begins, only
//! to learn which team to lock. On MySQL the first plain read of a transaction fixes its snapshot, so reading the
//! link inside the transaction before the team lock would hide a revocation that commits while the join waits for
//! the lock (see `lock_team_for`). The link is read again after the lock, and that read decides.

use std::collections::HashMap;

use domain::audit::{AuditAction, EntityType};
use domain::invite_link::{InviteLink, InviteLinkState, InviteToken, Revocation};
use domain::permission::{
    InvitationAction, InviteLinkAction, Permission, allowed_invitation_actions, allowed_invite_link_actions,
};
use domain::team::{Membership, Role, Team};
use domain::user::User;
use domain::{InviteLinkId, TeamId, Timestamp, UserId, Version};
use serde_json::json;
use store::audit::{AuditQuery, AuditScope};
use store::{ConnectionTrait, StoreError, TransactionTrait};

use super::manage::{MemberAdded, lock_team_for};
use super::{TeamDetail, load_team_detail};
use crate::access::authorize_team;
use crate::auth::{Registration, SessionGrant, hash_registration, random_token, validate_registration};
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
    /// How many people have joined the team through the link, ever, including those who have since left.
    pub joined_count: u32,
    pub allowed_actions: Vec<InviteLinkAction>,
}

/// An invite link as someone holding it sees it: what the team and the maker are called, and where the link stands.
#[derive(Debug)]
pub struct InvitationView {
    /// Where the link is in its life, as of the moment the view was made.
    pub state: InviteLinkState,
    pub team: Team,
    /// The maker, by their name now, even if they have left the team.
    pub created_by: User,
    /// The caller's standing in the team; `None` when the caller isn't signed in.
    pub viewer: Option<InviteViewer>,
    pub allowed_actions: Vec<InvitationAction>,
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
        let now = self.now();
        let (viewer, allowed_actions) = match principal {
            Some(principal) => {
                let (viewer, removed_at) = load_viewer(&self.db, &link, principal).await?;
                let is_member = viewer == InviteViewer::Member;
                (
                    Some(viewer),
                    allowed_invitation_actions(&link, Some(is_member), now, removed_at),
                )
            }
            None => (None, allowed_invitation_actions(&link, None, now, None)),
        };
        Ok(InvitationView {
            state: link.state(now),
            team,
            created_by,
            viewer,
            allowed_actions,
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
        let (team, members, link) = lock_link_team(&txn, team_id, link_id).await?;
        if members.iter().any(|m| m.user_id == principal.user_id) {
            let detail = load_team_detail(&txn, principal, team, self.now()).await?;
            return Ok((detail, MemberAdded::AlreadyMember));
        }

        let now = self.now();
        let removed_at = last_removed_at(&txn, team_id, principal.user_id).await?;
        link.check_join(now, removed_at)?;
        self.admit_through_link(&txn, principal, &link, now).await?;
        let detail = load_team_detail(&txn, principal, team, self.now()).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(%team_id, invite_link_id = %link.id, user_id = %principal.user_id, "joined through invite link");
        Ok((detail, MemberAdded::Added))
    }

    /// Creates an account through a link, and signs it in: the user, their personal team and heap, their password,
    /// their membership in the link's team and the sign-in are made together or not at all. Works whether
    /// registration is open or closed; the link is what lets the person in.
    ///
    /// Checks come in this order: the token, the input (as [`Services::register`] does), the link at this moment,
    /// then whether the email and username are free. So a link that ended while the form was open says so, even if
    /// the username was also taken. Fails with [`ServiceError::NotFound`] for an unknown token, and with
    /// [`ServiceError::InviteLink`] when the link ended or was revoked. Returns the link's team as the new member
    /// sees it.
    pub async fn register_through_invite(
        &self,
        token: &InviteToken,
        registration: Registration<'_>,
    ) -> Result<(SessionGrant, TeamDetail), ServiceError> {
        // Only to learn which team to lock: see the module doc.
        let found = store::invite_links::find_invite_link_by_token(&self.db, token)
            .await?
            .ok_or(ServiceError::NotFound)?;
        let (team_id, link_id) = (found.team_id, found.id);
        let valid = validate_registration(registration)?;
        // Only saves a hash for a link that has already stopped working. The check under the lock below is the one
        // that counts: the link may end or be revoked while the password is hashed.
        found.check_join(self.now(), None)?;
        let account = hash_registration(valid).await?;

        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        let (team, _, link) = lock_link_team(&txn, team_id, link_id).await?;
        // A new user has never been removed from any team. Before the user is made, so that an ended link wins over
        // a taken username.
        link.check_join(now, None)?;
        let (grant, _, _) = self.insert_account(&txn, account, now).await?;
        let principal = Principal {
            user_id: grant.user.id,
            run: None,
        };
        self.admit_through_link(&txn, principal, &link, now).await?;
        let detail = load_team_detail(&txn, principal, team, self.now()).await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(
            user_id = %grant.user.id,
            %team_id,
            invite_link_id = %link.id,
            "user registered through invite link"
        );
        Ok((grant, detail))
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

/// Locks the link's team and reads its members and the link again. Not `lock_team_for`: the caller is not a member
/// yet, so its membership check would refuse them. The link is read after the lock, so it sees a revocation
/// committed before the lock was taken. Fails with [`ServiceError::NotFound`] when either is gone, and with
/// [`domain::team::TeamError::PersonalTeam`] for a personal team.
async fn lock_link_team(
    txn: &impl ConnectionTrait,
    team_id: TeamId,
    link_id: InviteLinkId,
) -> Result<(Team, Vec<Membership>, InviteLink), ServiceError> {
    let team = store::teams::lock_team(txn, team_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let members = store::teams::list_memberships(txn, team_id).await?;
    team.ensure_shared()?;
    let link = store::invite_links::find_invite_link(txn, team_id, link_id)
        .await?
        .ok_or(ServiceError::NotFound)?;
    Ok((team, members, link))
}

impl Services {
    /// Makes `principal` a member of the link's team and records that they joined through it. The caller has
    /// locked the team and checked that the link admits them.
    async fn admit_through_link(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        link: &InviteLink,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let membership = Membership {
            team_id: link.team_id,
            user_id: principal.user_id,
            role: Role::Member,
            joined_at: now,
        };
        store::teams::insert_membership(txn, &membership).await?;
        let entry = self.team_audit_entry(
            principal,
            link.team_id,
            AuditAction::MemberJoined,
            EntityType::InviteLink,
            *link.id.as_uuid(),
            json!({ "role": Role::Member.code() }),
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
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
    let link_ids: Vec<InviteLinkId> = links.iter().map(|link| link.id).collect();
    let joins = store::audit::count_joins_by_invite_link(db, &link_ids).await?;
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
                joined_count: joins.get(&link.id).copied().unwrap_or(0),
                link,
            })
        })
        .collect()
}

fn find_user(users: &HashMap<UserId, User>, id: UserId) -> Result<User, ServiceError> {
    // The foreign keys on invite_links guarantee the user rows.
    users
        .get(&id)
        .cloned()
        .ok_or_else(|| missing("invite_links", id.to_string(), "link names a user that does not exist"))
}

/// Where `principal` stands in the link's team, for the page that opens the link, and their last removal from the
/// team (`None` for a member, who has no use for it).
async fn load_viewer(
    db: &impl ConnectionTrait,
    link: &InviteLink,
    principal: Principal,
) -> Result<(InviteViewer, Option<Timestamp>), ServiceError> {
    if store::teams::find_membership(db, link.team_id, principal.user_id)
        .await?
        .is_some()
    {
        return Ok((InviteViewer::Member, None));
    }
    let removed_at = last_removed_at(db, link.team_id, principal.user_id).await?;
    let viewer = if link.admits_after_removal(removed_at) {
        InviteViewer::NotMember
    } else {
        InviteViewer::Removed
    };
    Ok((viewer, removed_at))
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
