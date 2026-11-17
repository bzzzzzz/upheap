//! Teams whose members come from the sign-in provider's groups (architecture.md, Authentication and authorization).
//!
//! The operator maps provider groups to existing teams in the server's configuration. [`resolve_group_teams`] turns
//! the slugs into team IDs once at startup, and the resulting [`GroupTeams`] lives in [`crate::Settings`]. A team is
//! group-managed while the map has it; nothing about that is stored on the team.
//!
//! At each sign-in through the provider the ID token's groups claim is read ([`read_groups`]) and the person's
//! membership in each mapped team is brought in line with it ([`Services::sync_group_teams`]).

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::audit::{AuditAction, AuditField, EntityType};
use domain::handle::Handle;
use domain::team::{Membership, Role};
use domain::validation::Field;
use domain::{TeamId, Timestamp, UserId};
use serde_json::{Value, json};
use store::{ConnectionTrait, StoreError};

use crate::{Principal, ServiceError, Services};

/// The mapped teams and the provider groups that put people in each, in team ID order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTeams(BTreeMap<TeamId, Vec<String>>);

impl GroupTeams {
    /// The groups that put people in `team_id`, as configured, or `None` when the team isn't group-managed.
    pub fn get(&self, team_id: TeamId) -> Option<&[String]> {
        self.0.get(&team_id).map(Vec::as_slice)
    }

    /// Every mapped team with its groups, by team ID.
    pub fn iter(&self) -> impl Iterator<Item = (TeamId, &[String])> {
        self.0.iter().map(|(team_id, groups)| (*team_id, groups.as_slice()))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Why the configured map can't be used.
#[derive(Debug, thiserror::Error)]
pub enum GroupTeamsError {
    #[error("no team has the slug {slug:?}")]
    NoSuchTeam { slug: String },
    #[error("the team {slug:?} is a personal team, which only its owner belongs to")]
    PersonalTeam { slug: String },
    #[error("can't read the teams")]
    Store(#[from] StoreError),
}

/// The configured map with its slugs resolved to teams. A slug no team has, which includes one that can't be a slug,
/// and a personal team are errors that name the slug.
pub async fn resolve_group_teams(
    db: &impl ConnectionTrait,
    configured: &[(String, Vec<String>)],
) -> Result<GroupTeams, GroupTeamsError> {
    let mut resolved = BTreeMap::new();
    for (slug, groups) in configured {
        let no_such_team = || GroupTeamsError::NoSuchTeam { slug: slug.clone() };
        let handle = Handle::parse(slug, Field::Slug).map_err(|_| no_such_team())?;
        let team = store::teams::find_team_by_slug(db, &handle)
            .await?
            .ok_or_else(no_such_team)?;
        if team.is_personal() {
            return Err(GroupTeamsError::PersonalTeam { slug: slug.clone() });
        }
        resolved.insert(team.id, groups.clone());
    }
    Ok(GroupTeams(resolved))
}

/// The groups a person was in at sign-in, or why that isn't known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Groups {
    Known(BTreeSet<String>),
    /// The token didn't say: no membership changes. An empty list is [`Groups::Known`], not this.
    Unknown(&'static str),
}

/// The groups in the payload of a verified ID token, given as its compact form, under the claim `claim`.
///
/// The token's signature was checked before this is called, so its payload can be read as it is. A claim that is a
/// list of strings, or one string, is known. A missing claim beside `_claim_names` naming it is Entra ID's overage:
/// the groups are too many for the token and live behind another request that isn't made.
pub fn read_groups(id_token: &str, claim: &str) -> Groups {
    let payload = id_token
        .split('.')
        .nth(1)
        .and_then(|part| URL_SAFE_NO_PAD.decode(part).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let Some(Value::Object(payload)) = payload else {
        return Groups::Unknown("missing");
    };
    match payload.get(claim) {
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect::<Option<BTreeSet<_>>>()
            .map_or(Groups::Unknown("not a list of strings"), Groups::Known),
        Some(Value::String(group)) => Groups::Known(BTreeSet::from([group.clone()])),
        Some(_) => Groups::Unknown("not a list of strings"),
        None if payload
            .get("_claim_names")
            .and_then(Value::as_object)
            .is_some_and(|names| names.contains_key(claim)) =>
        {
            Groups::Unknown("overflowed")
        }
        None => Groups::Unknown("missing"),
    }
}

impl Services {
    /// Brings `user_id`'s memberships in the group-managed teams in line with `groups`, in the caller's transaction.
    /// Teams are locked in ID order, so two sign-ins never wait for each other in a cycle. Nothing happens for
    /// [`Groups::Unknown`], and nothing is read when no team is mapped.
    ///
    /// A person in one of the team's groups who isn't a member joins as a member. A member in none of them leaves,
    /// whatever their role, even as the last admin: the team then shows it has no admin. Each change has an audit
    /// entry with the person as its actor.
    pub(crate) async fn sync_group_teams(
        &self,
        txn: &impl ConnectionTrait,
        user_id: UserId,
        groups: &Groups,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let Groups::Known(groups) = groups else {
            return Ok(());
        };
        let principal = Principal { user_id, run: None };
        // Every team is locked, in ID order, before anything is read. On MySQL the first plain read of a transaction
        // fixes the snapshot that later plain reads see (see `lock_team_for`), so a membership read between two locks
        // would miss what committed while the next lock waited. The IDs were resolved at startup and teams are never
        // deleted, so a team the lock doesn't find is a broken database.
        for (team_id, _) in self.settings.group_teams.iter() {
            if store::teams::lock_team(txn, team_id).await?.is_none() {
                return Err(
                    StoreError::corrupt("teams", &team_id.to_string(), "a group-managed team is missing").into(),
                );
            }
        }
        for (team_id, mapped) in self.settings.group_teams.iter() {
            let current = store::teams::find_membership(txn, team_id, user_id).await?;
            let matching: Vec<&String> = groups.iter().filter(|group| mapped.contains(group)).collect();
            match (matching.is_empty(), current) {
                (false, None) => {
                    let membership = Membership {
                        team_id,
                        user_id,
                        role: Role::Member,
                        joined_at: now,
                    };
                    store::teams::insert_membership(txn, &membership).await?;
                    let diff = json!({ AuditField::Groups.code(): matching, AuditField::By.code(): PROVIDER_GROUPS });
                    self.insert_group_audit(txn, principal, team_id, AuditAction::GroupMemberAdded, diff, now)
                        .await?;
                    tracing::info!(%team_id, %user_id, "joined a team through provider groups");
                }
                (true, Some(member)) => {
                    store::teams::delete_membership(txn, team_id, user_id).await?;
                    let mut mapped: Vec<&String> = mapped.iter().collect();
                    mapped.sort();
                    let diff = json!({
                        AuditField::Role.code(): member.role.code(),
                        AuditField::Groups.code(): mapped,
                        AuditField::By.code(): PROVIDER_GROUPS,
                    });
                    self.insert_group_audit(txn, principal, team_id, AuditAction::GroupMemberRemoved, diff, now)
                        .await?;
                    tracing::info!(%team_id, %user_id, "left a team: no longer in its provider groups");
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn insert_group_audit(
        &self,
        txn: &impl ConnectionTrait,
        principal: Principal,
        team_id: TeamId,
        action: AuditAction,
        diff: Value,
        now: Timestamp,
    ) -> Result<(), ServiceError> {
        let entry = self.team_audit_entry(
            principal,
            team_id,
            action,
            EntityType::User,
            *principal.user_id.as_uuid(),
            diff,
            now,
        );
        store::audit::insert_audit_entry(txn, &entry).await?;
        Ok(())
    }
}

/// What `by` says in the audit entries of a sync: the only thing that makes them.
const PROVIDER_GROUPS: &str = "provider_groups";

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    /// A compact token whose payload is `payload`. The header and signature are not read.
    fn token(payload: &Value) -> String {
        format!("e30.{}.c2ln", URL_SAFE_NO_PAD.encode(payload.to_string()))
    }

    fn known(groups: &[&str]) -> Groups {
        Groups::Known(groups.iter().map(|group| (*group).to_owned()).collect())
    }

    #[gtest]
    fn a_list_of_strings_is_known() {
        expect_that!(
            read_groups(&token(&json!({ "groups": ["sre", "engineering"] })), "groups"),
            eq(&known(&["engineering", "sre"]))
        );
    }

    #[gtest]
    fn a_single_string_is_one_group() {
        expect_that!(
            read_groups(&token(&json!({ "roles": "engineering" })), "roles"),
            eq(&known(&["engineering"]))
        );
    }

    #[gtest]
    fn an_empty_list_is_known_and_empty() {
        expect_that!(read_groups(&token(&json!({ "groups": [] })), "groups"), eq(&known(&[])));
    }

    #[gtest]
    fn a_missing_claim_is_unknown() {
        expect_that!(
            read_groups(&token(&json!({ "sub": "a" })), "groups"),
            eq(&Groups::Unknown("missing"))
        );
    }

    #[gtest]
    fn a_claim_named_as_overflowed_is_unknown() {
        let payload = json!({ "_claim_names": { "groups": "src1" }, "_claim_sources": { "src1": {} } });

        expect_that!(
            read_groups(&token(&payload), "groups"),
            eq(&Groups::Unknown("overflowed"))
        );
        expect_that!(read_groups(&token(&payload), "roles"), eq(&Groups::Unknown("missing")));
    }

    #[gtest]
    fn anything_else_is_unknown() {
        for claim in [json!(["a", 1]), json!(3), json!({ "a": "b" }), json!(null), json!(true)] {
            expect_that!(
                read_groups(&token(&json!({ "groups": claim })), "groups"),
                eq(&Groups::Unknown("not a list of strings")),
                "{claim}"
            );
        }
    }

    #[gtest]
    #[tokio::test]
    async fn a_slug_no_team_has_and_a_personal_team_stop_the_map() -> Result<(), Box<dyn std::error::Error>> {
        use crate::auth::Registration;
        use crate::{ManualClock, Settings};
        use std::sync::Arc;

        // Arrange
        let db = store::connect("sqlite::memory:", 1).await?;
        store::run_migrations(&db).await?;
        let services = Services::new(
            db.clone(),
            Arc::new(ManualClock::new(Timestamp::EPOCH)),
            Settings {
                allow_registration: true,
                ..Settings::default()
            },
        );
        let sam = services
            .register(Registration {
                email: "sam@example.com",
                username: "sam",
                display_name: "Sam",
                password: "correct horse battery",
            })
            .await?;
        let team = services
            .create_team(
                Principal {
                    user_id: sam.user.id,
                    run: None,
                },
                domain::text::Name::parse("Ops")?,
                Some(Handle::parse("ops", Field::Slug)?),
            )
            .await?;
        let map = |slug: &str| vec![(slug.to_owned(), vec!["engineering".to_owned()])];

        // Act
        let resolved = resolve_group_teams(&db, &map("ops")).await?;
        let unknown = resolve_group_teams(&db, &map("nope")).await;
        let not_a_slug = resolve_group_teams(&db, &map("Not A Slug")).await;
        let personal = resolve_group_teams(&db, &map("sam")).await;

        // Assert
        expect_that!(resolved.get(team.team.id), some(eq(&["engineering".to_owned()][..])));
        expect_that!(
            unknown,
            err(matches_pattern!(GroupTeamsError::NoSuchTeam { slug: eq("nope") }))
        );
        expect_that!(
            not_a_slug,
            err(matches_pattern!(GroupTeamsError::NoSuchTeam { slug: eq("Not A Slug") }))
        );
        expect_that!(
            personal,
            err(matches_pattern!(GroupTeamsError::PersonalTeam { slug: eq("sam") }))
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn being_taken_out_by_the_groups_is_not_a_removal_that_voids_older_invite_links()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::auth::Registration;
        use crate::teams::InviteViewer;
        use crate::{ManualClock, Settings};
        use domain::permission::InvitationAction;
        use std::sync::Arc;
        use store::TransactionTrait;

        // Arrange: Alex joined Ops through Sam's link, and Ops is then managed by the group `engineering`.
        let db = store::connect("sqlite::memory:", 1).await?;
        store::run_migrations(&db).await?;
        let clock = Arc::new(ManualClock::new(Timestamp::EPOCH));
        let open = Services::new(
            db.clone(),
            clock.clone(),
            Settings {
                allow_registration: true,
                ..Settings::default()
            },
        );
        let mut people = Vec::new();
        for name in ["sam", "alex"] {
            let grant = open
                .register(Registration {
                    email: &format!("{name}@example.com"),
                    username: name,
                    display_name: name,
                    password: "correct horse battery",
                })
                .await?;
            people.push(Principal {
                user_id: grant.user.id,
                run: None,
            });
        }
        let (sam, alex) = (people[0], people[1]);
        let team = open
            .create_team(
                sam,
                domain::text::Name::parse("Ops")?,
                Some(Handle::parse("ops", Field::Slug)?),
            )
            .await?;
        let link = open.create_invite_link(sam, team.team.id).await?;
        open.join_team_by_invite(alex, &link.link.token).await?;
        let configured = [("ops".to_owned(), vec!["engineering".to_owned()])];
        let mapped = Services::new(
            db.clone(),
            clock,
            Settings {
                group_teams: resolve_group_teams(&db, &configured).await?,
                ..Settings::default()
            },
        );

        // Act: Alex's groups no longer name Ops.
        let txn = db.begin().await?;
        mapped
            .sync_group_teams(&txn, alex.user_id, &Groups::Known(BTreeSet::new()), mapped.now())
            .await?;
        txn.commit().await?;

        // Assert: with Ops unmapped again, the older link admits Alex as it admits anyone never removed by an admin.
        let opened = open.open_invite(Some(alex), &link.link.token).await?;
        expect_that!(opened.viewer, some(eq(InviteViewer::NotMember)));
        expect_that!(opened.allowed_actions, elements_are![eq(&InvitationAction::Join)]);
        Ok(())
    }
}
