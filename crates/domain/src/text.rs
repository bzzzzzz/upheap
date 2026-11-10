//! Bounded, trimmed text values used for titles and names.

use crate::validation::{Field, ValidationError};

/// Trims `raw` and checks it is non-empty and at most `max` characters long.
pub(crate) fn parse_required(raw: &str, field: Field, max: usize) -> Result<String, ValidationError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::Empty { field });
    }
    if trimmed.chars().count() > max {
        return Err(ValidationError::TooLong { field, max });
    }
    Ok(trimmed.to_owned())
}

/// Trims `raw` for a long text that may be left out: blank gives `None`, and more than `max` characters is too long.
pub(crate) fn parse_optional(raw: &str, field: Field, max: usize) -> Result<Option<String>, ValidationError> {
    match parse_required(raw, field, max) {
        Ok(text) => Ok(Some(text)),
        Err(ValidationError::Empty { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

/// A team or heap name: trimmed, 1 to 100 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(String);

impl Name {
    pub const MAX_CHARS: usize = 100;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Name, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Background text for a heap or a step, plain for now: blank means none, at most 20,000 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context(String);

impl Context {
    pub const MAX_CHARS: usize = 20_000;

    /// Blank input means "no context" and gives `None`. Surrounding whitespace is kept off.
    pub fn parse(raw: &str) -> Result<Option<Self>, ValidationError> {
        Self::parse_as(raw, Field::Context)
    }

    /// [`Self::parse`] for a text with the same rules that goes by another name, such as a question's details.
    pub fn parse_as(raw: &str, field: Field) -> Result<Option<Self>, ValidationError> {
        parse_optional(raw, field, Self::MAX_CHARS).map(|text| text.map(Self))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn context_is_trimmed_and_blank_means_none() {
        expect_that!(Context::parse("  \n "), ok(none()));
        expect_that!(
            Context::parse(" hi ").map(|context| context.map(|context| context.as_str().to_owned())),
            ok(some(eq("hi")))
        );
        expect_that!(Context::parse(&"x".repeat(20_000)), ok(some(anything())));
        expect_that!(
            Context::parse(&"x".repeat(20_001)),
            err(eq(&ValidationError::TooLong {
                field: Field::Context,
                max: 20_000
            }))
        );
    }
}
