//! A new user with a team of their own and a first heap in it: what registering makes, and what the local build
//! makes for its one user.

use domain::audit::{AuditAction, EntityType};
use domain::handle::Handle;
use domain::heap::{Heap, HeapPolicy};
use domain::team::{Membership, Role, Team};
use domain::text::Name;
use domain::user::{DisplayName, Email, User};
use domain::validation::Field;
use domain::{HeapId, TeamId, Timestamp, UserId, Version};
use serde_json::json;
use store::audit::NewAuditEntry;
use store::{ConnectionTrait, StoreError, TransactionTrait};

use crate::error::handle_taken_on_duplicate;
use crate::{Principal, ServiceError, Services};

/// The local build's one user. Nobody signs in, so the address is never used, and `.invalid` can't be anyone's.
const LOCAL_EMAIL: &str = "me@upheap.invalid";
const LOCAL_USERNAME: &str = "me";
const LOCAL_DISPLAY_NAME: &str = "Me";
const LOCAL_TEAM_NAME: &str = "My heaps";

/// Everyone's first heap, in their personal team.
const FIRST_HEAP_NAME: &str = "Personal";
const FIRST_HEAP_SLUG: &str = "personal";

/// What [`Services::insert_personal_user`] made.
pub(crate) struct PersonalUser {
    pub user: User,
    pub team: Team,
    pub heap: Heap,
}

impl Services {
    /// The local build's one user, made the first time with their team and first heap, which every later heap
    /// joins. Later calls return the same user.
    ///
    /// Call it once, before serving requests: two first calls at once could each make a user.
    pub async fn open_local_user(&self) -> Result<Principal, ServiceError> {
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        if let Some(user) = store::users::find_first_user(&txn).await? {
            return Ok(Principal { user_id: user.id });
        }
        let made = self
            .insert_personal_user(
                &txn,
                Email::parse(LOCAL_EMAIL)?,
                Handle::parse(LOCAL_USERNAME, Field::Username)?,
                DisplayName::parse(LOCAL_DISPLAY_NAME)?,
                Name::parse(LOCAL_TEAM_NAME)?,
                self.now(),
            )
            .await?;
        txn.commit().await.map_err(StoreError::from)?;

        tracing::info!(user_id = %made.user.id, team_id = %made.team.id, heap_id = %made.heap.id, "local user created");
        Ok(Principal { user_id: made.user.id })
    }

    /// Inserts a new user, their personal team with `username` as its slug and them as its only admin, and a first
    /// heap in it, with an audit entry for each. Fails with [`ServiceError::EmailTaken`] when the email is taken and
    /// [`ServiceError::HandleTaken`] when the username is.
    pub(crate) async fn insert_personal_user(
        &self,
        txn: &impl ConnectionTrait,
        email: Email,
        username: Handle,
        display_name: DisplayName,
        team_name: Name,
        now: Timestamp,
    ) -> Result<PersonalUser, ServiceError> {
        let user = User {
            id: UserId::from_uuid(self.new_id(now)),
            username: username.clone(),
            email,
            display_name,
            created_at: now,
            version: Version::INITIAL,
        };
        let team = Team {
            id: TeamId::from_uuid(self.new_id(now)),
            slug: username,
            name: team_name,
            personal_user: Some(user.id),
            created_at: now,
            version: Version::INITIAL,
        };
        let membership = Membership {
            team_id: team.id,
            user_id: user.id,
            role: Role::Admin,
            joined_at: now,
        };
        let heap = Heap {
            id: HeapId::from_uuid(self.new_id(now)),
            team_id: team.id,
            slug: Handle::parse(FIRST_HEAP_SLUG, Field::Slug)?,
            name: Name::parse(FIRST_HEAP_NAME)?,
            policy: HeapPolicy::default(),
            context: None,
            archived_at: None,
            version: Version::INITIAL,
        };

        store::users::insert_user(txn, &user).await.map_err(|e| match e {
            StoreError::Duplicate(_) => ServiceError::EmailTaken,
            other => other.into(),
        })?;
        store::teams::insert_team(txn, &team)
            .await
            .map_err(|e| handle_taken_on_duplicate(e, Field::Username))?;
        store::teams::insert_membership(txn, &membership).await?;
        store::heaps::insert_heap(txn, &heap, now).await?;
        let entry = |action, entity_type, entity_id, heap_id, diff| NewAuditEntry {
            id: self.new_id(now),
            team_id: team.id,
            heap_id,
            actor: user.id,
            action,
            entity_type,
            entity_id,
            diff,
            created_at: now,
        };
        let entries = [
            entry(
                AuditAction::UserRegistered,
                EntityType::User,
                *user.id.as_uuid(),
                None,
                json!({ "username": user.username.as_str(), "display_name": user.display_name.as_str() }),
            ),
            entry(
                AuditAction::TeamCreated,
                EntityType::Team,
                *team.id.as_uuid(),
                None,
                json!({ "slug": team.slug.as_str(), "name": team.name.as_str() }),
            ),
            entry(
                AuditAction::MemberAdded,
                EntityType::User,
                *user.id.as_uuid(),
                None,
                json!({ "role": Role::Admin.code() }),
            ),
            entry(
                AuditAction::HeapCreated,
                EntityType::Heap,
                *heap.id.as_uuid(),
                Some(heap.id),
                json!({ "slug": heap.slug.as_str(), "name": heap.name.as_str() }),
            ),
        ];
        for entry in &entries {
            store::audit::insert_audit_entry(txn, entry).await?;
        }
        Ok(PersonalUser { user, team, heap })
    }
}
