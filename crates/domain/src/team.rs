//! Teams and membership.

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
    pub name: Name,
    pub created_at: Timestamp,
    pub version: Version,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Membership {
    pub team_id: TeamId,
    pub user_id: UserId,
    pub role: Role,
    pub joined_at: Timestamp,
}
