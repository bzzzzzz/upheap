//! Teams and membership. A team always keeps at least one admin, and a personal team keeps exactly its owner.

use crate::handle::Handle;
use crate::text::Name;
use crate::{TeamId, Timestamp, UserId, Version};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    /// Stable code used in storage and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Admin, Self::Member].into_iter().find(|role| role.code() == code)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Team {
    pub id: TeamId,
    /// Unique among teams. On a personal team it is the owner's username, and never changes.
    pub slug: Handle,
    pub name: Name,
    /// The user whose personal team this is. `None` for a shared team.
    pub personal_user: Option<UserId>,
    pub created_at: Timestamp,
    pub version: Version,
}

impl Team {
    /// A personal team is private to its owner: nobody joins it, its owner can't leave it, and its slug is fixed.
    pub const fn is_personal(&self) -> bool {
        self.personal_user.is_some()
    }

    /// Refuses a change only a shared team can take: adding or removing a member, leaving, or a new slug.
    pub const fn ensure_shared(&self) -> Result<(), TeamError> {
        if self.is_personal() {
            return Err(TeamError::PersonalTeam { team_id: self.id });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Membership {
    pub team_id: TeamId,
    pub user_id: UserId,
    pub role: Role,
    pub joined_at: Timestamp,
}

/// Why a membership change was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeamError {
    #[error("team {team_id} would have no admin left; make another member an admin first")]
    LastAdmin { team_id: TeamId },
    #[error("team {team_id} is a personal team, which only its owner belongs to")]
    PersonalTeam { team_id: TeamId },
}

/// Whether the team whose memberships are `members` still has an admin once `user_id` gets `new_role`, or leaves
/// when it is `None`.
pub fn admin_remains(members: &[Membership], user_id: UserId, new_role: Option<Role>) -> bool {
    let others = members.iter().any(|m| m.user_id != user_id && m.role == Role::Admin);
    others || new_role == Some(Role::Admin)
}

/// Refuses a change that would leave `team_id` without an admin. A change to a member who isn't an admin always
/// passes, so a team that somehow has no admin can still take members in and out.
pub fn ensure_admin_remains(
    team_id: TeamId,
    members: &[Membership],
    user_id: UserId,
    new_role: Option<Role>,
) -> Result<(), TeamError> {
    let is_admin = members.iter().any(|m| m.user_id == user_id && m.role == Role::Admin);
    if is_admin && !admin_remains(members, user_id, new_role) {
        return Err(TeamError::LastAdmin { team_id });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn member(n: u128, role: Role) -> Membership {
        Membership {
            team_id: TeamId::from_uuid(Uuid::from_u128(1)),
            user_id: UserId::from_uuid(Uuid::from_u128(n)),
            role,
            joined_at: Timestamp::EPOCH,
        }
    }

    #[gtest]
    fn the_last_admin_cannot_leave_or_be_demoted() {
        let team = TeamId::from_uuid(Uuid::from_u128(1));
        let admin = member(10, Role::Admin);
        let members = [admin, member(11, Role::Member)];

        expect_that!(
            ensure_admin_remains(team, &members, admin.user_id, None),
            err(eq(&TeamError::LastAdmin { team_id: team }))
        );
        expect_that!(
            ensure_admin_remains(team, &members, admin.user_id, Some(Role::Member)),
            err(eq(&TeamError::LastAdmin { team_id: team }))
        );
        expect_that!(
            ensure_admin_remains(team, &members, admin.user_id, Some(Role::Admin)),
            ok(eq(&()))
        );
    }

    #[gtest]
    fn only_shared_teams_change_members_or_slug() {
        let team_id = TeamId::from_uuid(Uuid::from_u128(1));
        let shared = Team {
            id: team_id,
            slug: Handle::parse("ops", crate::validation::Field::Slug).expect("valid"),
            name: Name::parse("Ops").expect("valid"),
            personal_user: None,
            created_at: Timestamp::EPOCH,
            version: Version::INITIAL,
        };
        let personal = Team {
            personal_user: Some(UserId::from_uuid(Uuid::from_u128(10))),
            ..shared.clone()
        };

        expect_that!(shared.ensure_shared(), ok(eq(&())));
        expect_that!(personal.ensure_shared(), err(eq(&TeamError::PersonalTeam { team_id })));
    }

    #[gtest]
    fn another_admin_or_a_plain_member_lets_the_change_through() {
        let team = TeamId::from_uuid(Uuid::from_u128(1));
        let members = [
            member(10, Role::Admin),
            member(11, Role::Admin),
            member(12, Role::Member),
        ];

        expect_that!(
            ensure_admin_remains(team, &members, members[0].user_id, None),
            ok(eq(&()))
        );
        expect_that!(
            ensure_admin_remains(team, &members, members[2].user_id, None),
            ok(eq(&()))
        );
    }
}
