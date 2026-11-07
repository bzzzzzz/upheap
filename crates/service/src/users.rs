//! Finding users to add to a team. The local build has no one else, and leaves this out (the `accounts` feature).

use domain::handle::Handle;
use domain::permission::Permission;
use domain::user::{User, UserQuery};
use domain::validation::Field;

use crate::{Principal, ServiceError, Services};

/// The most users a lookup returns. A lookup is for picking one person, not for browsing.
pub const USER_LOOKUP_LIMIT: u64 = 10;

impl Services {
    /// Users for an admin picking someone to add to a team: the one whose username is `query`, those with a word of
    /// their display name starting with `query`, and, when `query` has an email's local part, `@` and part of its domain, those whose email starts
    /// with it. Up to [`USER_LOOKUP_LIMIT`], by display name.
    ///
    /// A match shows the email only when the caller typed that much of it, so a lookup by name can't be used to
    /// collect addresses.
    ///
    /// Only admins of some shared team may look users up ([`Permission::ManageTeam`]), since nobody joins a personal
    /// team; everyone else gets [`ServiceError::Forbidden`].
    pub async fn find_users(&self, principal: Principal, query: UserQuery) -> Result<Vec<UserMatch>, ServiceError> {
        let teams = store::teams::list_teams_for_user(&self.db, principal.user_id).await?;
        let is_admin = teams
            .iter()
            .any(|(team, role)| !team.is_personal() && Permission::ManageTeam.is_granted_to(*role));
        if !is_admin {
            return Err(ServiceError::Forbidden(Permission::ManageTeam));
        }
        let by_email = match query.as_email_prefix() {
            Some(prefix) => store::users::find_users_by_email_prefix(&self.db, prefix, USER_LOOKUP_LIMIT).await?,
            None => Vec::new(),
        };
        let by_name = if query.matches_names() {
            store::users::find_users_by_name(&self.db, &query, USER_LOOKUP_LIMIT).await?
        } else {
            Vec::new()
        };
        let by_username = match Handle::parse(query.as_str(), Field::Username) {
            Ok(username) => store::users::find_user_by_username(&self.db, &username).await?,
            Err(_) => None,
        };

        let mut matches: Vec<UserMatch> = by_email
            .into_iter()
            .map(|user| UserMatch {
                user,
                email_matched: true,
            })
            .collect();
        for user in by_username.into_iter().chain(by_name) {
            if !matches.iter().any(|m| m.user.id == user.id) {
                matches.push(UserMatch {
                    user,
                    email_matched: false,
                });
            }
        }
        matches
            .sort_by(|a, b| (a.user.display_name.as_str(), a.user.id).cmp(&(b.user.display_name.as_str(), b.user.id)));
        matches.truncate(usize::try_from(USER_LOOKUP_LIMIT).unwrap_or(usize::MAX));
        Ok(matches)
    }
}

/// A user found by [`Services::find_users`].
#[derive(Debug)]
pub struct UserMatch {
    pub user: User,
    /// Whether the query matched the start of their email, which the caller may then be shown.
    pub email_matched: bool,
}
