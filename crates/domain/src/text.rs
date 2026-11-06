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
