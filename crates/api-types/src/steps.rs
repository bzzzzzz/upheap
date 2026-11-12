//! Request types of the step endpoints. They answer with the whole item (`ItemDto`).

use domain::ValidationError;
use domain::item::Title;
use domain::step::{NewStep, StepNumber, StepPatch, StepState};
use domain::text::Context;
use domain::validation::Field;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common::{StepStateDto, double_option};

/// A step to add to a task.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AddStepRequest)]
pub struct AddStepDto {
    /// 1 to 300 characters.
    pub title: String,
    /// Markdown, at most 20,000 characters; blank means none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// The number of the step the new one goes right after. Left out, the step goes at the end.
    #[schema(minimum = 1)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u32>,
}

impl AddStepDto {
    /// The new step's fields. Fails on the first invalid field. Step numbers start at 1, so an `after` of zero is
    /// `ValueUnknown`.
    pub fn to_new_step(&self) -> Result<NewStep, ValidationError> {
        Ok(NewStep {
            title: Title::parse(&self.title)?,
            context: self.context.as_deref().map(Context::parse).transpose()?.flatten(),
            after: self
                .after
                .map(|number| StepNumber::new(number).ok_or(ValidationError::ValueUnknown { field: Field::After }))
                .transpose()?,
        })
    }
}

/// A partial edit of a step. Leave a field out to keep it; send `null` to clear the context.
#[derive(Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = StepPatch)]
pub struct StepPatchDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Markdown; `null` or blank clears it.
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, nullable)]
    pub context: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<StepStateDto>,
}

impl StepPatchDto {
    /// The edit. Fails on the first invalid field; a state this server doesn't know is `ValueUnknown`.
    pub fn to_patch(&self) -> Result<StepPatch, ValidationError> {
        Ok(StepPatch {
            title: self.title.as_deref().map(Title::parse).transpose()?,
            context: match &self.context {
                None => None,
                Some(None) => Some(None),
                Some(Some(raw)) => Some(Context::parse(raw)?),
            },
            state: self.state.map(StepState::try_from).transpose()?,
        })
    }
}
