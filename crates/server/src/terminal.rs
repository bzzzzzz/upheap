//! Text for the operator's terminal.

/// `text` with every control character shown as U+FFFD, since a name comes from another person and an escape sequence
/// in it would run in the operator's terminal.
pub fn for_terminal(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn control_characters_in_a_name_are_shown_as_replacement_characters() {
        expect_that!(for_terminal("a\u{1b}[2Jb\nc"), eq("a\u{FFFD}[2Jb\u{FFFD}c"));
    }
}
