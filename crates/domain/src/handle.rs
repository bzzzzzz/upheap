//! Readable names for URLs and the CLI (design/readable-ids.md): usernames, team and heap slugs, and item keys.
//! UUIDs stay the keys everywhere else.

use std::fmt;

use crate::validation::{Field, ValidationError};

/// A username, team slug or heap slug: lowercase ASCII letters, digits and single dashes, starting with a letter and
/// not ending with a dash, [`Handle::MIN_CHARS`] to [`Handle::MAX_CHARS`] long.
///
/// Usernames and team slugs share one namespace, because a username is the slug of its user's personal team.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(String);

impl Handle {
    pub const MIN_CHARS: usize = 2;
    pub const MAX_CHARS: usize = 39;

    /// Checks `raw` as it is. Nothing is fixed silently: `My-Team` is refused, with `my-team` as the hint.
    pub fn parse(raw: &str, field: Field) -> Result<Self, ValidationError> {
        if is_valid(raw) {
            return Ok(Self(raw.to_owned()));
        }
        Err(ValidationError::HandleInvalid {
            field,
            hint: Self::derive(raw).map(|handle| handle.0),
        })
    }

    /// A handle made from a display name: "My Team" gives `my-team`. ASCII letters and digits are kept and lowercased,
    /// every run of anything else becomes one dash, and leading digits are dropped, since a handle starts with a
    /// letter. `None` when too little is left, as for "日本" or "2027".
    pub fn derive(name: &str) -> Option<Self> {
        let mut slug = String::new();
        for c in name.chars() {
            if c.is_ascii_alphanumeric() {
                if slug.is_empty() && c.is_ascii_digit() {
                    continue;
                }
                slug.push(c.to_ascii_lowercase());
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug = truncate(&slug, Self::MAX_CHARS);
        is_valid(slug).then(|| Self(slug.to_owned()))
    }

    /// This handle with `-{n}` on the end, shortened first if it wouldn't fit: `my-team-2`. For finding a free
    /// handle when the derived one is taken.
    pub fn with_suffix(&self, n: u32) -> Self {
        let suffix = format!("-{n}");
        let base = truncate(&self.0, Self::MAX_CHARS.saturating_sub(suffix.len()));
        Self(format!("{base}{suffix}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_valid(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let allowed = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-';
    (Handle::MIN_CHARS..=Handle::MAX_CHARS).contains(&bytes.len())
        && bytes.iter().all(allowed)
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && !raw.ends_with('-')
        && !raw.contains("--")
}

/// The first `max` bytes of an ASCII slug, without a dash left on the end.
fn truncate(slug: &str, max: usize) -> &str {
    slug.get(..max).unwrap_or(slug).trim_end_matches('-')
}

/// Crockford's base32 alphabet in lowercase: digits and letters without `i`, `l`, `o` and `u`.
const KEY_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// An item's key, unique within its heap: [`ItemKey::LEN`] characters of Crockford's base32 in lowercase, at least
/// one of them a digit. Random, so it says nothing about when the item was made, and never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemKey([u8; ItemKey::LEN]);

impl ItemKey {
    pub const LEN: usize = 5;

    /// The key spelled by `random`, five bytes from a random source, of which the low five bits of each pick a
    /// character. `None` when no character is a digit: about one draw in six, which the caller replaces with
    /// another.
    pub fn from_random(random: [u8; Self::LEN]) -> Option<Self> {
        let key = random.map(|b| KEY_ALPHABET[usize::from(b & 31)]);
        key.iter().any(u8::is_ascii_digit).then_some(Self(key))
    }

    /// Checks a key as typed. Keys are lowercase; anything else isn't a key.
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let key: [u8; Self::LEN] = raw.as_bytes().try_into().map_err(|_| ValidationError::ItemKeyInvalid)?;
        let well_formed = key.iter().all(|b| KEY_ALPHABET.contains(b)) && key.iter().any(u8::is_ascii_digit);
        if !well_formed {
            return Err(ValidationError::ItemKeyInvalid);
        }
        Ok(Self(key))
    }

    pub fn as_str(&self) -> &str {
        // Every byte comes from the ASCII alphabet.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl fmt::Display for ItemKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use proptest::prelude::*;

    use super::*;

    fn parse(raw: &str) -> Result<Handle, ValidationError> {
        Handle::parse(raw, Field::Slug)
    }

    #[gtest]
    fn handles_are_lowercase_words_with_single_dashes() {
        for good in ["my-team", "infra-2027", "ab", "a1", "a-b-c"] {
            expect_that!(parse(good).map(|h| h.to_string()), ok(eq(good)), "{good}");
        }
        for bad in [
            "",
            "a",
            "1team",
            "-team",
            "team-",
            "my--team",
            "my_team",
            "my team",
            "ÿes",
            &"a".repeat(40),
        ] {
            expect_that!(parse(bad), err(anything()), "{bad}");
        }
    }

    #[gtest]
    fn a_refused_handle_hints_at_one_that_works() {
        expect_that!(
            parse("My-Team"),
            err(eq(&ValidationError::HandleInvalid {
                field: Field::Slug,
                hint: Some("my-team".to_owned())
            }))
        );
        expect_that!(
            parse("--"),
            err(eq(&ValidationError::HandleInvalid {
                field: Field::Slug,
                hint: None
            }))
        );
    }

    #[gtest]
    fn names_turn_into_handles() {
        let derive = |name: &str| Handle::derive(name).map(|h| h.to_string());
        expect_that!(derive("My Team"), some(eq("my-team")));
        expect_that!(derive("  Infra / Ops: 2027! "), some(eq("infra-ops-2027")));
        expect_that!(derive("2027 plans"), some(eq("plans")));
        expect_that!(derive("Ölaf's heap"), some(eq("laf-s-heap")));
        expect_that!(derive("日本"), none());
        expect_that!(derive("x"), none());
        expect_that!(
            derive(&"long ".repeat(20)).map(|h| h.len()),
            some(le(Handle::MAX_CHARS))
        );
    }

    #[gtest]
    fn suffixes_keep_handles_in_bounds() {
        let team = parse("my-team").expect("valid");
        expect_that!(team.with_suffix(2).as_str(), eq("my-team-2"));
        let long = parse(&format!("{}-b", "a".repeat(36))).expect("valid");
        let suffixed = long.with_suffix(12);
        expect_that!(parse(suffixed.as_str()), ok(eq(&suffixed)));
    }

    #[gtest]
    fn keys_need_a_digit() {
        expect_that!(
            ItemKey::from_random([0, 1, 2, 3, 4]).map(|k| k.to_string()),
            some(eq("01234"))
        );
        expect_that!(
            ItemKey::from_random([10, 11, 12, 13, 20]).map(|k| k.to_string()),
            none()
        );
        expect_that!(
            ItemKey::from_random([10, 11, 12, 13, 36]).map(|k| k.to_string()),
            some(eq("abcd4"))
        );
    }

    #[gtest]
    fn typed_keys_must_be_well_formed() {
        expect_that!(ItemKey::parse("k3x9a").map(|k| k.to_string()), ok(eq("k3x9a")));
        for bad in ["K3X9A", "k3x9", "k3x9ab", "kaxya", "k3o9a", "k3u9a", "ab-12"] {
            expect_that!(ItemKey::parse(bad), err(eq(&ValidationError::ItemKeyInvalid)), "{bad}");
        }
    }

    proptest! {
        #[test]
        fn every_generated_key_parses(random: [u8; 5]) {
            if let Some(key) = ItemKey::from_random(random) {
                prop_assert_eq!(ItemKey::parse(key.as_str()), Ok(key));
            }
        }

        #[test]
        fn every_derived_handle_parses(name in "\\PC{0,60}") {
            if let Some(handle) = Handle::derive(&name) {
                prop_assert_eq!(parse(handle.as_str()), Ok(handle.clone()));
                prop_assert_eq!(parse(handle.with_suffix(999).as_str()).is_ok(), true);
            }
        }
    }
}
