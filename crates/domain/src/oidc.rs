//! Signing in through an OpenID Connect provider: the rules that need no IO. The mechanism is in architecture.md,
//! Authentication and authorization.

use crate::user::DisplayName;
use crate::{Duration, Timestamp};

/// How long a sign-in lasts from the start to the callback, and then how long the username step may take.
pub const PENDING_SIGN_IN_LIFETIME: Duration = Duration::from_minutes(15);

/// When a sign-in in flight, started or verified at `now`, ends. `None` past the supported range.
pub fn pending_sign_in_ends_at(now: Timestamp) -> Option<Timestamp> {
    now.checked_add(PENDING_SIGN_IN_LIFETIME)
}

/// The most characters a sign-in's `redirect` has, which is what its column holds.
pub const REDIRECT_MAX_CHARS: usize = 2048;

/// Whether `path` may be a sign-in's `redirect`: a path of this app, so a sign-in can't be used to send someone to
/// another site. It starts with `/`, the second character is not `/` or `\` (browsers read both as a host), and it
/// holds no ASCII control character or space, and is at most
/// [`REDIRECT_MAX_CHARS`] long.
pub fn is_app_path(path: &str) -> bool {
    let mut chars = path.chars();
    chars.next() == Some('/')
        && !matches!(chars.next(), Some('/' | '\\'))
        && path.chars().count() <= REDIRECT_MAX_CHARS
        && !path.chars().any(|c| c.is_ascii_control() || c == ' ')
}

/// The display name of a user who signs in through a provider for the first time: the provider's `name` claim if it
/// is a valid display name, else the part of the email before the `@`.
pub fn choose_display_name(name: Option<&str>, email: &str) -> Option<DisplayName> {
    name.and_then(|name| DisplayName::parse(name).ok())
        .or_else(|| DisplayName::parse(email.split('@').next().unwrap_or_default()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_app_paths() {
        for path in [
            "/",
            "/teams/a?x=1#y",
            "/oauth/authorize?response_type=code&client_id=upheap-cli",
        ] {
            assert!(is_app_path(path), "{path}");
        }
    }

    #[test]
    fn rejects_other_redirects() {
        for path in ["//evil", "/\\evil", "https://x", "evil", "", "/a b", "/a\tb", "/a\nb"] {
            assert!(!is_app_path(path), "{path:?}");
        }
    }

    #[test]
    fn rejects_a_redirect_longer_than_its_column() {
        assert!(is_app_path(&format!("/{}", "a".repeat(REDIRECT_MAX_CHARS - 1))));
        assert!(!is_app_path(&format!("/{}", "a".repeat(REDIRECT_MAX_CHARS))));
    }

    #[test]
    fn prefers_the_name_claim() {
        let name = choose_display_name(Some("  Sam Doe "), "sam@example.com");
        assert_eq!(name.as_ref().map(DisplayName::as_str), Some("Sam Doe"));
    }

    #[test]
    fn falls_back_to_the_email_local_part() {
        let too_long = "x".repeat(DisplayName::MAX_CHARS + 1);
        for name in [None, Some(""), Some("   "), Some(too_long.as_str())] {
            let chosen = choose_display_name(name, "sam@example.com");
            assert_eq!(chosen.as_ref().map(DisplayName::as_str), Some("sam"));
        }
    }
}
