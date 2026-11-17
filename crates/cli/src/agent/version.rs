//! The version of an agent tool, as `<tool> --version` prints it, and the oldest one `work run` starts.

use super::Tool;

/// A tool's version, `major.minor.patch`, compared part by part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

impl Tool {
    /// The oldest version of the tool `work run` starts: the flags and output the loop relies on are there from it on.
    /// Raised by hand when the loop starts using something newer.
    pub const fn min_version(self) -> Version {
        match self {
            Self::Claude => Version(2, 1, 326),
            Self::Copilot => Version(1, 0, 112),
        }
    }
}

/// The first `major.minor.patch` in `output`, such as `2.1.326` in `2.1.326 (Claude Code)` or `1.0.112` in
/// `GitHub Copilot CLI 1.0.112.`. Anything after the patch, a fourth part or a suffix, is ignored. `None` when there
/// is none.
pub fn parse(output: &str) -> Option<Version> {
    output
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .find_map(|word| {
            let mut parts = word.trim_matches('.').split('.').map(|part| part.parse::<u64>().ok());
            match (parts.next(), parts.next(), parts.next()) {
                (Some(Some(major)), Some(Some(minor)), Some(Some(patch))) => Some(Version(major, minor, patch)),
                _ => None,
            }
        })
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn the_version_is_read_from_what_each_tool_prints() {
        expect_that!(parse("2.1.326 (Claude Code)\n"), some(eq(Version(2, 1, 326))));
        expect_that!(
            parse("GitHub Copilot CLI 1.0.112.\nRun 'copilot update' to check for updates.\n"),
            some(eq(Version(1, 0, 112)))
        );
        expect_that!(parse("v3.0.0-beta.2"), some(eq(Version(3, 0, 0))));
        expect_that!(parse("tool 1.2.3.4"), some(eq(Version(1, 2, 3))));
    }

    #[gtest]
    fn output_without_a_version_has_none() {
        expect_that!(parse(""), none());
        expect_that!(parse("Claude Code"), none());
        expect_that!(parse("version 2.1"), none());
        expect_that!(parse("1..2.3"), none());
    }

    #[gtest]
    fn versions_compare_part_by_part_as_numbers() {
        expect_that!(Version(2, 1, 326), gt(Version(2, 1, 99)));
        expect_that!(Version(2, 10, 0), gt(Version(2, 9, 999)));
    }
}
