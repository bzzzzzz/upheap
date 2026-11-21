//! The tag commands: their args and flows.

use clap::builder::{PossibleValuesParser, TypedValueParser};
use domain::tag::TagColor;

pub mod add;
pub mod edit;
pub mod list;
pub mod promote;
mod refusal;

/// The value parser of `--color`: one of the palette's codes, which `--help` lists, read into the colour.
fn color_parser() -> impl TypedValueParser<Value = TagColor> {
    PossibleValuesParser::new(TagColor::ALL.map(TagColor::code)).try_map(|code| TagColor::from_code(&code).ok_or(code))
}
