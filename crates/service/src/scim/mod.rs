//! SCIM 2.0 provisioning: the identity provider pushes its users, groups and deactivations, and the group-managed
//! teams follow at once.
//!
//! The pushes are stored as the provider sent them ([`store::scim`]). Each write runs in one transaction that locks
//! the mapped teams first, then the SCIM row being written, then (a write to users) the subject a record is created
//! with or moved onto ([`store::scim::lock_external_id`]), then for each linked person it touches the person's
//! personal team, found before the transaction so the lock comes before any read of the person's state,
//! writes the SCIM rows, then runs the group-teams sync for every touched person who is linked: a SCIM user is linked
//! to an account when `user_identities` has the configured issuer and the SCIM user's `externalId`, the person's
//! subject. Nothing stores the link.
//!
//! Records may share an `externalId`, so a person's state is read from all of them at once ([`ScimPerson`]): inactive
//! only when every record is, else in the groups of the active ones. A linked person who is left inactive, or whose
//! last record is deleted, also loses every session and token, and [`Services::open_session`] refuses an inactive
//! person until the provider turns them back on. A sign-in of a known person locks their personal team before it
//! opens the session, and approving a CLI sign-in and exchanging its code lock it before they store or use the
//! code, so each comes before such a write, which ends it, or after, and is refused. A write to users that
//! turns a linked person off or back on records it in their personal team, where their own account history reads it.
//!
//! Resources are JSON as in RFC 7643, built and read here; the HTTP side is in `api`.

mod filter;
mod patch;

use std::collections::BTreeSet;

use domain::audit::{AuditAction, AuditField, EntityType};
use domain::{TeamId, Timestamp, UserId};
use serde_json::{Map, Value, json};
use store::scim::{GroupField, ScimGroup, ScimUser, UserField};
use store::{ConnectionTrait, StoreError, TransactionTrait};
use uuid::Uuid;

use crate::auth::sha256_hex;
use crate::group_teams::Groups;
use crate::{Principal, ServiceError, Services};

const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";

/// The most resources a list returns, and what it returns when `count` isn't given.
pub const MAX_PAGE: i64 = 100;

/// The longest `userName`, `displayName` and `externalId`, which the columns hold.
const MAX_TEXT_CHARS: usize = 255;

/// The provider's access to the SCIM endpoints, from the operator's configuration (README, Configuration).
#[derive(Clone)]
pub struct ScimSettings {
    /// SHA-256 of the token in lowercase hex: the token itself isn't kept.
    token_hash: String,
    /// `BASE_URL`, without a trailing slash: what resource locations start with.
    base_url: String,
}

impl ScimSettings {
    pub fn new(token: &str, base_url: &str) -> Self {
        Self {
            token_hash: sha256_hex(token),
            base_url: base_url.to_owned(),
        }
    }
}

impl std::fmt::Debug for ScimSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScimSettings")
            .field("token_hash", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Why a SCIM request failed, which `api` says in SCIM's error format.
#[derive(Debug, thiserror::Error)]
pub enum ScimError {
    #[error("the bearer token is missing or wrong")]
    Unauthorized,
    #[error("no such resource")]
    NotFound,
    #[error("{0}")]
    Uniqueness(&'static str),
    #[error("only `<attribute> eq \"<value>\"` on a supported attribute is understood")]
    InvalidFilter,
    #[error("{0}")]
    InvalidValue(&'static str),
    #[error("{0}")]
    InvalidSyntax(&'static str),
    #[error("{0}")]
    NoTarget(&'static str),
    #[error(transparent)]
    Service(#[from] ServiceError),
}

impl From<StoreError> for ScimError {
    fn from(error: StoreError) -> Self {
        Self::Service(error.into())
    }
}

impl From<filter::InvalidFilter> for ScimError {
    fn from(_: filter::InvalidFilter) -> Self {
        Self::InvalidFilter
    }
}

impl From<patch::PatchError> for ScimError {
    fn from(error: patch::PatchError) -> Self {
        match error {
            patch::PatchError::Syntax(detail) => Self::InvalidSyntax(detail),
            patch::PatchError::Value(detail) => Self::InvalidValue(detail),
            patch::PatchError::Filter => Self::InvalidFilter,
            patch::PatchError::NoTarget => Self::NoTarget("remove needs a path"),
        }
    }
}

/// A duplicate-key failure on a write means another resource has the unique attribute.
fn taken_on_duplicate(error: StoreError, detail: &'static str) -> ScimError {
    match error {
        StoreError::Duplicate(_) => ScimError::Uniqueness(detail),
        other => other.into(),
    }
}

/// How a list is paged and filtered, as the query string says.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListQuery<'a> {
    pub filter: Option<&'a str>,
    pub start_index: Option<i64>,
    pub count: Option<i64>,
}

impl ListQuery<'_> {
    /// The 1-based start (below 1 counts as 1) and how many to return (default and most 100, below 0 is 0).
    fn page(&self) -> (u64, u64) {
        let start = self.start_index.unwrap_or(1).max(1);
        let count = self.count.unwrap_or(MAX_PAGE).clamp(0, MAX_PAGE);
        (start.unsigned_abs(), count.unsigned_abs())
    }
}

/// A user or group body that passed the checks.
#[derive(Clone)]
struct UserInput {
    user_name: String,
    external_id: Option<String>,
    active: bool,
    /// What the provider sent besides the attributes the columns hold.
    body: Value,
}

struct GroupInput {
    display_name: String,
    external_id: Option<String>,
    members: BTreeSet<Uuid>,
    body: Value,
}

fn object(body: &Value) -> Result<&Map<String, Value>, ScimError> {
    body.as_object()
        .ok_or(ScimError::InvalidSyntax("the body must be a JSON object"))
}

fn required_text(map: &Map<String, Value>, key: &'static str, detail: &'static str) -> Result<String, ScimError> {
    match map.get(key) {
        Some(Value::String(text)) if !text.is_empty() && text.chars().count() <= MAX_TEXT_CHARS => Ok(text.clone()),
        _ => Err(ScimError::InvalidValue(detail)),
    }
}

fn external_id(map: &Map<String, Value>) -> Result<Option<String>, ScimError> {
    match map.get("externalId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.chars().count() <= MAX_TEXT_CHARS => Ok(Some(text.clone())),
        Some(_) => Err(ScimError::InvalidValue(
            "externalId must be a text of at most 255 characters",
        )),
    }
}

/// The body less the keys Upheap sets or holds in columns.
fn rest_of(map: &Map<String, Value>, dropped: &[&str]) -> Value {
    Value::Object(
        map.iter()
            .filter(|(key, _)| !dropped.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn parse_user(body: &Value) -> Result<UserInput, ScimError> {
    let map = object(body)?;
    let active = match map.get("active") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(active)) => *active,
        Some(_) => return Err(ScimError::InvalidValue("active must be true or false")),
    };
    Ok(UserInput {
        user_name: required_text(map, "userName", "userName is required, at most 255 characters")?,
        external_id: external_id(map)?,
        active,
        body: rest_of(
            map,
            &["schemas", "id", "meta", "password", "userName", "externalId", "active"],
        ),
    })
}

fn parse_group(body: &Value) -> Result<GroupInput, ScimError> {
    const MEMBERS: &str = "members must be a list of objects whose value is a user's id";
    let map = object(body)?;
    let members = match map.get("members") {
        None | Some(Value::Null) => BTreeSet::new(),
        Some(Value::Array(members)) => members
            .iter()
            .map(|member| {
                member
                    .get("value")
                    .and_then(Value::as_str)
                    .and_then(|id| Uuid::parse_str(id).ok())
                    .ok_or(ScimError::InvalidValue(MEMBERS))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(ScimError::InvalidValue(MEMBERS)),
    };
    Ok(GroupInput {
        display_name: required_text(map, "displayName", "displayName is required, at most 255 characters")?,
        external_id: external_id(map)?,
        members,
        body: rest_of(map, &["schemas", "id", "meta", "displayName", "externalId", "members"]),
    })
}

fn parse_id(id: &str) -> Result<Uuid, ScimError> {
    Uuid::parse_str(id).map_err(|_| ScimError::NotFound)
}

impl Services {
    /// Whether the server takes SCIM requests: the operator set a token.
    pub fn scim_enabled(&self) -> bool {
        self.settings.scim.is_some()
    }

    fn scim(&self) -> Result<&ScimSettings, ScimError> {
        self.settings.scim.as_ref().ok_or(ScimError::NotFound)
    }

    /// Checks the bearer token of a request: [`ScimError::Unauthorized`] unless it is the operator's. The SHA-256
    /// digests are compared, not the tokens.
    pub fn scim_authenticate(&self, presented: Option<&str>) -> Result<(), ScimError> {
        let settings = self.scim().map_err(|_| ScimError::Unauthorized)?;
        match presented {
            Some(token) if sha256_hex(token) == settings.token_hash => Ok(()),
            _ => Err(ScimError::Unauthorized),
        }
    }

    // Reads.

    pub async fn scim_list_users(&self, query: ListQuery<'_>) -> Result<Value, ScimError> {
        let filter = match query.filter {
            Some(text) => {
                let parsed = filter::parse(text, &["userName", "externalId"])?;
                let field = if parsed.attribute == "userName" {
                    UserField::UserName
                } else {
                    UserField::ExternalId
                };
                Some((field, parsed.value))
            }
            None => None,
        };
        let (start, count) = query.page();
        let page = store::scim::list_users(
            &self.db,
            filter.as_ref().map(|(field, value)| (*field, value.as_str())),
            start - 1,
            count,
        )
        .await?;
        let resources = page
            .rows
            .iter()
            .map(|user| self.render_user(user))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(list_response(page.total, start, resources))
    }

    pub async fn scim_get_user(&self, id: &str) -> Result<Value, ScimError> {
        let user = store::scim::find_user(&self.db, parse_id(id)?)
            .await?
            .ok_or(ScimError::NotFound)?;
        self.render_user(&user)
    }

    pub async fn scim_list_groups(&self, query: ListQuery<'_>) -> Result<Value, ScimError> {
        let filter = match query.filter {
            Some(text) => {
                let parsed = filter::parse(text, &["displayName", "externalId"])?;
                let field = if parsed.attribute == "displayName" {
                    GroupField::DisplayName
                } else {
                    GroupField::ExternalId
                };
                Some((field, parsed.value))
            }
            None => None,
        };
        let (start, count) = query.page();
        let page = store::scim::list_groups(
            &self.db,
            filter.as_ref().map(|(field, value)| (*field, value.as_str())),
            start - 1,
            count,
        )
        .await?;
        let mut resources = Vec::new();
        for group in &page.rows {
            let members = store::scim::list_group_members(&self.db, group.id).await?;
            resources.push(self.render_group(group, &members)?);
        }
        Ok(list_response(page.total, start, resources))
    }

    pub async fn scim_get_group(&self, id: &str) -> Result<Value, ScimError> {
        let id = parse_id(id)?;
        let group = store::scim::find_group(&self.db, id)
            .await?
            .ok_or(ScimError::NotFound)?;
        let members = store::scim::list_group_members(&self.db, id).await?;
        self.render_group(&group, &members)
    }

    // Writes to users.

    pub async fn scim_create_user(&self, body: &Value) -> Result<Value, ScimError> {
        let input = parse_user(body)?;
        let now = self.now();
        let user = ScimUser {
            id: self.new_id(now),
            user_name: input.user_name,
            external_id: input.external_id,
            active: input.active,
            body: input.body,
            created_at: now,
            updated_at: now,
        };
        let subjects: BTreeSet<String> = user.external_id.iter().cloned().collect();
        let personal_teams = self.personal_teams(&subjects).await?;
        let txn = store::scim::begin_user_write(&self.db).await?;
        self.lock_group_teams(&txn).await?;
        // No row has the subject yet, so a first sign-in of the person can't lock this one ahead.
        if let Some(subject) = &user.external_id {
            store::scim::lock_external_id(&txn, subject).await?;
        }
        lock_teams(&txn, &personal_teams).await?;
        let was_off = self.off_subjects(&txn, &subjects).await?;
        store::scim::insert_user(&txn, &user)
            .await
            .map_err(|e| taken_on_duplicate(e, "userName is taken"))?;
        let pushed = self.pushed_teams(&txn).await?;
        self.apply_to_linked(&txn, subjects, false, Some(&was_off), &pushed, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_user_id = %user.id, active = user.active, "SCIM user created");
        self.render_user(&user)
    }

    pub async fn scim_replace_user(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let id = parse_id(id)?;
        let input = parse_user(body)?;
        self.save_user(id, |_| Ok(input.clone())).await
    }

    pub async fn scim_patch_user(&self, id: &str, patch: &Value) -> Result<Value, ScimError> {
        let id = parse_id(id)?;
        self.save_user(id, |current| {
            let Value::Object(mut resource) = self.render_user(current)? else {
                return Err(ScimError::InvalidSyntax("the stored user isn't an object"));
            };
            patch::apply(&mut resource, patch::Resource::User, patch)?;
            parse_user(&Value::Object(resource))
        })
        .await
    }

    /// Replaces the stored user with what `change` makes of it, and applies the change to the teams.
    async fn save_user(
        &self,
        id: Uuid,
        change: impl Fn(&ScimUser) -> Result<UserInput, ScimError>,
    ) -> Result<Value, ScimError> {
        let now = self.now();
        // The person the record belongs to and the one the write moves it to, as the stored record tells before the
        // locks. `change` runs again on the record read under its lock, which decides.
        let personal_teams = match store::scim::find_user(&self.db, id).await? {
            Some(stored) => {
                let moved_to = change(&stored).ok().and_then(|input| input.external_id);
                let subjects = [stored.external_id, moved_to].into_iter().flatten().collect();
                self.personal_teams(&subjects).await?
            }
            None => BTreeSet::new(),
        };
        let txn = store::scim::begin_user_write(&self.db).await?;
        self.lock_group_teams(&txn).await?;
        let current = store::scim::lock_user(&txn, id).await?.ok_or(ScimError::NotFound)?;
        let input = change(&current)?;
        // A first sign-in of the person the record moves to can't lock this row ahead: it doesn't have their subject
        // yet. The person it moves from is covered by the row's lock.
        if let Some(subject) = input
            .external_id
            .as_ref()
            .filter(|&moved_to| current.external_id.as_ref() != Some(moved_to))
        {
            store::scim::lock_external_id(&txn, subject).await?;
        }
        lock_teams(&txn, &personal_teams).await?;
        // The person the record was linked to before, if the write moves it to another.
        let subjects: BTreeSet<String> = [input.external_id.clone(), current.external_id.clone()]
            .into_iter()
            .flatten()
            .collect();
        let was_off = self.off_subjects(&txn, &subjects).await?;
        let user = ScimUser {
            id,
            user_name: input.user_name,
            external_id: input.external_id,
            active: input.active,
            body: input.body,
            created_at: current.created_at,
            updated_at: now,
        };
        store::scim::update_user(&txn, &user)
            .await
            .map_err(|e| taken_on_duplicate(e, "userName is taken"))?;
        let pushed = self.pushed_teams(&txn).await?;
        self.apply_to_linked(&txn, subjects, false, Some(&was_off), &pushed, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_user_id = %user.id, active = user.active, "SCIM user changed");
        self.render_user(&user)
    }

    pub async fn scim_delete_user(&self, id: &str) -> Result<(), ScimError> {
        let id = parse_id(id)?;
        let now = self.now();
        let stored_subject = store::scim::find_user(&self.db, id)
            .await?
            .and_then(|user| user.external_id);
        let personal_teams = self.personal_teams(&stored_subject.into_iter().collect()).await?;
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        self.lock_group_teams(&txn).await?;
        let current = store::scim::lock_user(&txn, id).await?.ok_or(ScimError::NotFound)?;
        lock_teams(&txn, &personal_teams).await?;
        let subjects: BTreeSet<String> = current.external_id.into_iter().collect();
        let was_off = self.off_subjects(&txn, &subjects).await?;
        store::scim::delete_user_memberships(&txn, id).await?;
        store::scim::delete_user(&txn, id).await?;
        // A person left with no record is inactive: no groups and no sign-ins.
        let pushed = self.pushed_teams(&txn).await?;
        self.apply_to_linked(&txn, subjects, true, Some(&was_off), &pushed, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_user_id = %id, "SCIM user deleted");
        Ok(())
    }

    // Writes to groups.

    pub async fn scim_create_group(&self, body: &Value) -> Result<Value, ScimError> {
        let input = parse_group(body)?;
        let now = self.now();
        let group = ScimGroup {
            id: self.new_id(now),
            display_name: input.display_name,
            external_id: input.external_id,
            body: input.body,
            created_at: now,
            updated_at: now,
        };
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        self.lock_group_teams(&txn).await?;
        check_members_exist(&txn, &input.members).await?;
        let pushed_before = self.pushed_teams(&txn).await?;
        store::scim::insert_group(&txn, &group)
            .await
            .map_err(|e| taken_on_duplicate(e, "displayName is taken"))?;
        store::scim::replace_group_members(&txn, group.id, &input.members).await?;
        self.after_group_write(&txn, &input.members, &pushed_before, now)
            .await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_group_id = %group.id, members = input.members.len(), "SCIM group created");
        self.render_group(&group, &input.members)
    }

    pub async fn scim_replace_group(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let id = parse_id(id)?;
        let input = parse_group(body)?;
        self.save_group(id, |_, _| Ok(input)).await
    }

    pub async fn scim_patch_group(&self, id: &str, patch: &Value) -> Result<Value, ScimError> {
        let id = parse_id(id)?;
        self.save_group(id, |current, members| {
            let Value::Object(mut resource) = self.render_group(current, members)? else {
                return Err(ScimError::InvalidSyntax("the stored group isn't an object"));
            };
            patch::apply(&mut resource, patch::Resource::Group, patch)?;
            parse_group(&Value::Object(resource))
        })
        .await
    }

    /// Replaces the stored group and its members with what `change` makes of them, and applies the change to the
    /// teams of everyone who was or is a member.
    async fn save_group(
        &self,
        id: Uuid,
        change: impl FnOnce(&ScimGroup, &BTreeSet<Uuid>) -> Result<GroupInput, ScimError>,
    ) -> Result<Value, ScimError> {
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        self.lock_group_teams(&txn).await?;
        let current = store::scim::lock_group(&txn, id).await?.ok_or(ScimError::NotFound)?;
        let before = store::scim::list_group_members(&txn, id).await?;
        let pushed_before = self.pushed_teams(&txn).await?;
        let input = change(&current, &before)?;
        check_members_exist(&txn, &input.members).await?;
        let group = ScimGroup {
            id,
            display_name: input.display_name,
            external_id: input.external_id,
            body: input.body,
            created_at: current.created_at,
            updated_at: now,
        };
        store::scim::update_group(&txn, &group)
            .await
            .map_err(|e| taken_on_duplicate(e, "displayName is taken"))?;
        store::scim::replace_group_members(&txn, id, &input.members).await?;
        let touched: BTreeSet<Uuid> = before.union(&input.members).copied().collect();
        self.after_group_write(&txn, &touched, &pushed_before, now).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_group_id = %id, members = input.members.len(), "SCIM group changed");
        self.render_group(&group, &input.members)
    }

    pub async fn scim_delete_group(&self, id: &str) -> Result<(), ScimError> {
        let id = parse_id(id)?;
        let now = self.now();
        let txn = self.db.begin().await.map_err(StoreError::from)?;
        self.lock_group_teams(&txn).await?;
        store::scim::lock_group(&txn, id).await?.ok_or(ScimError::NotFound)?;
        let members = store::scim::list_group_members(&txn, id).await?;
        let pushed_before = self.pushed_teams(&txn).await?;
        store::scim::delete_group_members(&txn, id).await?;
        store::scim::delete_group(&txn, id).await?;
        self.after_group_write(&txn, &members, &pushed_before, now).await?;
        txn.commit().await.map_err(StoreError::from)?;
        tracing::info!(scim_group_id = %id, "SCIM group deleted");
        Ok(())
    }

    // Applying a change.

    /// After a group's members or names changed: syncs the teams of the people of the users `touched`, for every team
    /// that was pushed before the write or is after it. A team the write stopped being pushed for (one of its groups
    /// was deleted or renamed away) is synced with the people's remaining groups as known, so those in none of its
    /// groups leave it, and it follows the token from then on. A team the write made pushed for the first
    /// time is synced for every person the provider pushed, and for that team only.
    async fn after_group_write(
        &self,
        txn: &impl ConnectionTrait,
        touched: &BTreeSet<Uuid>,
        pushed_before: &BTreeSet<TeamId>,
        now: Timestamp,
    ) -> Result<(), ScimError> {
        let pushed_after = self.pushed_teams(txn).await?;
        let mut subjects = BTreeSet::new();
        for id in touched {
            subjects.extend(
                store::scim::find_user(txn, *id)
                    .await?
                    .and_then(|user| user.external_id),
            );
        }
        let teams: BTreeSet<TeamId> = pushed_before.union(&pushed_after).copied().collect();
        self.apply_to_linked(txn, subjects, false, None, &teams, now).await?;
        let started: BTreeSet<TeamId> = pushed_after.difference(pushed_before).copied().collect();
        if !started.is_empty() {
            let everyone = store::scim::list_external_ids(txn).await?;
            self.apply_to_linked(txn, everyone, false, None, &started, now).await?;
        }
        Ok(())
    }

    /// The people among `subjects` who are off: they have records and every one is inactive. Read before a write to
    /// users changes the rows, and after the locks.
    async fn off_subjects(
        &self,
        db: &impl ConnectionTrait,
        subjects: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>, ServiceError> {
        let mut off = BTreeSet::new();
        for subject in subjects {
            if self
                .scim_person(db, subject)
                .await?
                .is_some_and(|person| !person.active)
            {
                off.insert(subject.clone());
            }
        }
        Ok(off)
    }

    /// For each person among `subjects` who is linked to an account: for a write to users (`was_off` is `Some`, the
    /// people who were off before it) ends the sign-ins of an inactive person, and records `account_deactivated` in
    /// their personal team when they weren't off before, or `account_reactivated` when they were and are active now.
    /// Syncs their teams from their SCIM state: an active person's membership in `teams`, an
    /// inactive person's in every mapped team. A person left with no record is inactive when
    /// `deleted`, else unchanged. Logs once how many aren't linked, and changes nothing for them.
    async fn apply_to_linked(
        &self,
        txn: &impl ConnectionTrait,
        subjects: BTreeSet<String>,
        deleted: bool,
        was_off: Option<&BTreeSet<String>>,
        teams: &BTreeSet<TeamId>,
        now: Timestamp,
    ) -> Result<(), ScimError> {
        let mut unlinked = 0_u32;
        for subject in &subjects {
            let Some(user_id) = self.linked_user(txn, subject).await? else {
                unlinked += 1;
                continue;
            };
            let person = match self.scim_person(txn, subject).await? {
                Some(person) => person,
                None if deleted => ScimPerson::inactive(),
                None => continue,
            };
            if let Some(was_off) = was_off {
                let off_before = was_off.contains(subject);
                // Before the sign-ins: password changes and resets lock the personal team, then end sign-ins, and sign-ins
                // open a session under it.
                let personal_team = self.lock_personal_team(txn, user_id).await?;
                if !person.active {
                    let ended_sign_ins = self.end_sign_ins(txn, user_id, None, None, now).await?;
                    tracing::info!(%user_id, ended_sign_ins, "ended the sign-ins of a person the provider deactivated");
                    if !off_before {
                        let diff = json!({ AuditField::EndedSignIns.code(): ended_sign_ins });
                        self.record_account_change(
                            txn,
                            Principal { user_id, run: None },
                            user_id,
                            personal_team,
                            AuditAction::AccountDeactivated,
                            diff,
                            now,
                        )
                        .await?;
                    }
                } else if off_before {
                    self.record_account_change(
                        txn,
                        Principal { user_id, run: None },
                        user_id,
                        personal_team,
                        AuditAction::AccountReactivated,
                        json!({}),
                        now,
                    )
                    .await?;
                }
            }
            if !self.settings.group_teams.is_empty() {
                let teams = person.active.then_some(teams);
                self.sync_group_teams(txn, user_id, &Groups::Known(person.groups), teams, now)
                    .await?;
            }
        }
        if unlinked > 0 {
            tracing::info!(
                unlinked,
                "SCIM push touched people with no linked account; no team changes for them"
            );
        }
        Ok(())
    }

    /// What a push does to a linked person who is inactive, for a person a sign-in has just linked to their SCIM
    /// records by moving their identity onto the subject: their sign-ins end, the turn-off is recorded, and their
    /// mapped teams follow the SCIM state. The person wasn't off before, so the entry is recorded. The caller holds
    /// the locks a push takes, in the same order.
    pub(crate) async fn apply_scim_state_to_moved(
        &self,
        txn: &impl ConnectionTrait,
        subject: &str,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let pushed = self.pushed_teams(txn).await?;
        self.apply_to_linked(
            txn,
            BTreeSet::from([subject.to_owned()]),
            false,
            Some(&BTreeSet::new()),
            &pushed,
            now,
        )
        .await
        .map_err(|error| match error {
            ScimError::Service(error) => error,
            // Applying a state to a linked person has no other failure.
            _ => StoreError::corrupt("scim_users", subject, "applying the state to a moved identity failed").into(),
        })
    }

    /// The personal teams of the people among `subjects` who are linked to an account, read before a write to users
    /// begins its transaction. The write locks them right after the SCIM row and before it reads any person's state:
    /// on MySQL a transaction's first plain read fixes its snapshot, so reading them inside it first would hide a
    /// write to another record of the same person that committed while this one waited for the lock. Two writes to
    /// records of one person then run one after the other, also when no mapped team's lock orders them.
    async fn personal_teams(&self, subjects: &BTreeSet<String>) -> Result<BTreeSet<TeamId>, ServiceError> {
        let mut teams = BTreeSet::new();
        for subject in subjects {
            if let Some(user_id) = self.linked_user(&self.db, subject).await? {
                teams.insert(self.personal_team(&self.db, user_id).await?);
            }
        }
        Ok(teams)
    }

    /// The personal team of `user_id`, where their account entries go.
    pub(crate) async fn personal_team(
        &self,
        db: &impl ConnectionTrait,
        user_id: UserId,
    ) -> Result<TeamId, ServiceError> {
        let user = store::users::find_user(db, user_id)
            .await?
            .ok_or_else(|| StoreError::corrupt("users", &user_id.to_string(), "a linked identity has no user"))?;
        self.load_personal_team(db, &user).await
    }

    /// Locks the personal team of `user_id` and returns it. Usually the write locked it already, before reading the
    /// person's state ([`Self::personal_teams`]); this covers a person the early read couldn't name: one linked, or
    /// one the record was moved to, by a write that committed in between.
    pub(crate) async fn lock_personal_team(
        &self,
        txn: &impl ConnectionTrait,
        user_id: UserId,
    ) -> Result<TeamId, ServiceError> {
        let personal_team = self.personal_team(txn, user_id).await?;
        store::teams::lock_team(txn, personal_team).await?;
        Ok(personal_team)
    }

    /// Writes an account entry about `user_id` in their personal team, with `actor` as its actor. A change the
    /// provider made has the person as the actor: no account did it.
    // One more than clippy likes: the actor is a different account from the person for an operator's entries, and
    // the personal team is already locked and known to each caller, so none of them is worth a type of its own.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_account_change(
        &self,
        txn: &impl ConnectionTrait,
        actor: Principal,
        user_id: UserId,
        personal_team: TeamId,
        action: AuditAction,
        diff: Value,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let entry = self.team_audit_entry(
            actor,
            personal_team,
            action,
            EntityType::User,
            *user_id.as_uuid(),
            diff,
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
    }

    /// The account whose identity at the configured provider has `subject`.
    async fn linked_user(&self, db: &impl ConnectionTrait, subject: &str) -> Result<Option<UserId>, ServiceError> {
        let Some(oidc) = &self.settings.oidc else {
            return Ok(None);
        };
        Ok(store::identities::find_identity_user(db, oidc.issuer(), subject).await?)
    }

    /// The mapped teams that are pushed: every one of the team's groups is the display name or external ID of a
    /// stored SCIM group. Until the last of them arrives, the token decides the team, so a team mapped to several
    /// groups loses no one while they are pushed one at a time. Read it once per write or sign-in, after locking the
    /// mapped teams.
    pub(crate) async fn pushed_teams(&self, db: &impl ConnectionTrait) -> Result<BTreeSet<TeamId>, ServiceError> {
        let mut pushed = BTreeSet::new();
        'teams: for (team_id, groups) in self.settings.group_teams.iter() {
            for group in groups {
                if !store::scim::any_group_named(db, &BTreeSet::from([group.clone()])).await? {
                    continue 'teams;
                }
            }
            pushed.insert(team_id);
        }
        Ok(pushed)
    }

    /// What the pushes say about the person who is `subject` at the provider, from every record with that
    /// `externalId`, or `None` when there is no record.
    ///
    /// The person is inactive only when every record is, and then in no group. Otherwise their groups are the display
    /// name and external ID of every group the active records are in.
    async fn scim_person(&self, db: &impl ConnectionTrait, subject: &str) -> Result<Option<ScimPerson>, ServiceError> {
        let records = store::scim::list_users_by_external_id(db, subject).await?;
        if records.is_empty() {
            return Ok(None);
        }
        let active: Vec<&ScimUser> = records.iter().filter(|record| record.active).collect();
        if active.is_empty() {
            return Ok(Some(ScimPerson::inactive()));
        }
        let mut groups = BTreeSet::new();
        for record in active {
            for (display_name, external_id) in store::scim::list_user_group_names(db, record.id).await? {
                groups.insert(display_name);
                groups.extend(external_id);
            }
        }
        Ok(Some(ScimPerson { active: true, groups }))
    }

    // Sign-in.

    /// Syncs the teams of the person who signed in, whose subject at the provider is `subject`, in the caller's
    /// transaction, after it locked the mapped teams. With SCIM set up and a SCIM record for the person, the pushes
    /// decide the teams that are pushed (every mapped team when the person is inactive) so a token minted before a
    /// push can't undo it, and `token_groups` decide the others. Without either, `token_groups` decide every team.
    pub(crate) async fn sync_sign_in_groups(
        &self,
        txn: &impl ConnectionTrait,
        user_id: UserId,
        subject: &str,
        token_groups: Option<Groups>,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let mut by_token = None;
        if self.settings.scim.is_some()
            && !self.settings.group_teams.is_empty()
            && let Some(person) = self.scim_person(txn, subject).await?
        {
            let teams = if person.active {
                self.pushed_teams(txn).await?
            } else {
                self.settings.group_teams.iter().map(|(team_id, _)| team_id).collect()
            };
            self.sync_group_teams(txn, user_id, &Groups::Known(person.groups), Some(&teams), now)
                .await?;
            by_token = Some(teams);
        }
        let Some(groups) = token_groups else {
            return Ok(());
        };
        match by_token {
            None => self.sync_group_teams(txn, user_id, &groups, None, now).await,
            Some(pushed) => {
                let others = self
                    .settings
                    .group_teams
                    .iter()
                    .map(|(team_id, _)| team_id)
                    .filter(|team_id| !pushed.contains(team_id))
                    .collect();
                self.sync_group_teams(txn, user_id, &groups, Some(&others), now).await
            }
        }
    }

    /// Refuses with [`ServiceError::AccountDeactivated`] when SCIM is set up and the person is inactive: they have
    /// SCIM records and every one is inactive.
    pub(crate) async fn refuse_deactivated(
        &self,
        db: &impl ConnectionTrait,
        user_id: UserId,
    ) -> Result<(), ServiceError> {
        if store::users::is_turned_off(db, user_id).await? {
            tracing::info!(%user_id, "sign-in refused: an operator turned the account off");
            return Err(ServiceError::AccountTurnedOff);
        }
        let (Some(_), Some(oidc)) = (&self.settings.scim, &self.settings.oidc) else {
            return Ok(());
        };
        let Some(subject) = store::identities::find_identity_subject(db, oidc.issuer(), user_id).await? else {
            return Ok(());
        };
        if self.is_scim_inactive(db, &subject).await? {
            tracing::info!(%user_id, "sign-in refused: the provider deactivated the account");
            return Err(ServiceError::AccountDeactivated);
        }
        Ok(())
    }

    /// Whether the pushes make the person who is `subject` at the provider inactive.
    pub(crate) async fn is_scim_inactive(
        &self,
        db: &impl ConnectionTrait,
        subject: &str,
    ) -> Result<bool, ServiceError> {
        Ok(matches!(self.scim_person(db, subject).await?, Some(person) if !person.active))
    }

    // Resources.

    fn render_user(&self, user: &ScimUser) -> Result<Value, ScimError> {
        let mut resource = match &user.body {
            Value::Object(body) => body.clone(),
            _ => Map::new(),
        };
        resource.insert("schemas".to_owned(), json!([USER_SCHEMA]));
        resource.insert("id".to_owned(), json!(user.id));
        resource.insert("userName".to_owned(), json!(user.user_name));
        if let Some(external_id) = &user.external_id {
            resource.insert("externalId".to_owned(), json!(external_id));
        }
        resource.insert("active".to_owned(), json!(user.active));
        resource.insert(
            "meta".to_owned(),
            self.meta("User", "Users", user.id, user.created_at, user.updated_at)?,
        );
        Ok(Value::Object(resource))
    }

    fn render_group(&self, group: &ScimGroup, members: &BTreeSet<Uuid>) -> Result<Value, ScimError> {
        let mut resource = match &group.body {
            Value::Object(body) => body.clone(),
            _ => Map::new(),
        };
        resource.insert("schemas".to_owned(), json!([GROUP_SCHEMA]));
        resource.insert("id".to_owned(), json!(group.id));
        resource.insert("displayName".to_owned(), json!(group.display_name));
        if let Some(external_id) = &group.external_id {
            resource.insert("externalId".to_owned(), json!(external_id));
        }
        resource.insert(
            "members".to_owned(),
            Value::Array(members.iter().map(|id| json!({ "value": id })).collect()),
        );
        resource.insert(
            "meta".to_owned(),
            self.meta("Group", "Groups", group.id, group.created_at, group.updated_at)?,
        );
        Ok(Value::Object(resource))
    }

    fn meta(
        &self,
        resource_type: &str,
        collection: &str,
        id: Uuid,
        created: Timestamp,
        modified: Timestamp,
    ) -> Result<Value, ScimError> {
        Ok(json!({
            "resourceType": resource_type,
            "created": created.to_string(),
            "lastModified": modified.to_string(),
            "location": format!("{}/scim/v2/{collection}/{id}", self.scim()?.base_url),
        }))
    }
}

/// What the pushes say about one person, read from all their records.
struct ScimPerson {
    active: bool,
    groups: BTreeSet<String>,
}

impl ScimPerson {
    /// Inactive, or left with no record: no sign-ins, and in no group.
    fn inactive() -> Self {
        Self {
            active: false,
            groups: BTreeSet::new(),
        }
    }
}

fn list_response(total: u64, start: u64, resources: Vec<Value>) -> Value {
    json!({
        "schemas": [LIST_SCHEMA],
        "totalResults": total,
        "itemsPerPage": resources.len(),
        "startIndex": start,
        "Resources": resources,
    })
}

/// Locks `teams` in ID order, so two writes that lock the same ones take them in the same order.
async fn lock_teams(txn: &impl ConnectionTrait, teams: &BTreeSet<TeamId>) -> Result<(), ServiceError> {
    for team in teams {
        store::teams::lock_team(txn, *team).await?;
    }
    Ok(())
}

/// A member that names no stored user is refused.
async fn check_members_exist(db: &impl ConnectionTrait, members: &BTreeSet<Uuid>) -> Result<(), ScimError> {
    if store::scim::existing_user_ids(db, members).await?.len() == members.len() {
        Ok(())
    } else {
        Err(ScimError::InvalidValue("a member's value names no user"))
    }
}
