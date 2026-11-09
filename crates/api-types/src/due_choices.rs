//! The rough due times a client offers in place of a date ("next week"), with the dates they stand for.

use domain::due::DueChoice;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A rough due time. Send its `date` as a loose due time (`firm: false`), and the item's due label comes back as this
/// `label`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = DueChoice)]
pub struct DueChoiceDto {
    /// "next week".
    pub label: String,
    /// `YYYY-MM-DD`, in the request's timezone.
    pub date: String,
}

impl From<DueChoice> for DueChoiceDto {
    fn from(choice: DueChoice) -> Self {
        Self {
            label: choice.label.to_owned(),
            date: choice.date.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = DueChoices)]
pub struct DueChoicesDto {
    /// Nearest first. The dates move with the day, so ask again on another day.
    pub choices: Vec<DueChoiceDto>,
}
