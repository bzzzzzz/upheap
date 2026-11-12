//! The parameter commands: their args and flows, and what the ones that write share.

pub mod add;
pub mod archive;
pub mod edit;
pub mod list;
pub mod option;

use api_types::parameters::{ArrangeOptionsDto, OptionArrangementDto, OptionDto};
use clap::builder::RangedI64ValueParser;
use domain::parameter::Weight;
use uuid::Uuid;

/// The parser of `--weight`: 0 to 100, so a weight outside it is a usage error before any request.
fn weight_parser() -> RangedI64ValueParser<u8> {
    clap::value_parser!(u8).range(..=i64::from(Weight::MAX))
}

/// The help of `--weight`, which says what a new parameter gets when it is left out.
fn describe_weight_flag() -> String {
    format!(
        "How much it counts in the ranking, 0 to {}. Left out, {}",
        Weight::MAX,
        Weight::DEFAULT.get()
    )
}

/// An arrangement of `options`, in the order given and with their current labels, except that `relabel` gives one
/// option, by ID, a new label.
fn arrangement(options: &[&OptionDto], relabel: Option<(Uuid, &str)>) -> ArrangeOptionsDto {
    ArrangeOptionsDto {
        options: options
            .iter()
            .map(|option| OptionArrangementDto {
                id: option.id,
                label: match relabel {
                    Some((id, label)) if id == option.id => label.to_owned(),
                    _ => option.label.clone(),
                },
            })
            .collect(),
    }
}
