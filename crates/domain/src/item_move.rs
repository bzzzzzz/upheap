//! Moving an item to another heap of its team: which of its tags and parameter values the target heap has too.
//!
//! Tags and parameters belong to one heap, so an item that moves keeps only what the target heap has under the same
//! name: a tag by name, a value by parameter name and option label, each ignoring case. The rest is dropped, and the
//! caller records what was dropped (product.md, Items).

use std::collections::{BTreeMap, BTreeSet};

use crate::parameter::Parameter;
use crate::tag::Tag;
use crate::{OptionId, ParameterId, TagId};

/// What an item carries to the heap it moves to, and how many tags and values it leaves behind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CarriedOver {
    /// The target heap's tags that match the item's tags by name.
    pub tags: BTreeSet<TagId>,
    /// The target heap's options that match the item's values by parameter name and option label.
    pub parameters: BTreeMap<ParameterId, OptionId>,
    /// How many of the item's tags the target heap has no tag of the same name for.
    pub tags_dropped: usize,
    /// How many of the item's values the target heap has no parameter in use and option in use of the same names for.
    pub values_dropped: usize,
}

/// A heap's tags and its parameters, archived ones included.
pub type HeapVocabulary<'a> = (&'a [Tag], &'a [Parameter]);

/// What an item with `tags` and `values` in the heap `source` takes to the heap `target`. Only parameters and options
/// in use of the target match. An item's unset values carry nothing and so don't count as dropped.
pub fn carry_over(
    tags: &BTreeSet<TagId>,
    values: &BTreeMap<ParameterId, OptionId>,
    source: HeapVocabulary<'_>,
    target: HeapVocabulary<'_>,
) -> CarriedOver {
    let mut carried = CarriedOver::default();
    for tag_id in tags {
        let matching = source
            .0
            .iter()
            .find(|tag| tag.id == *tag_id)
            .and_then(|tag| target.0.iter().find(|other| other.name.to_key() == tag.name.to_key()));
        match matching {
            Some(target) => {
                carried.tags.insert(target.id);
            }
            None => carried.tags_dropped += 1,
        }
    }
    for (parameter_id, option_id) in values {
        match matching_option(source.1, target.1, *parameter_id, *option_id) {
            Some((parameter, option)) => {
                carried.parameters.insert(parameter, option);
            }
            None => carried.values_dropped += 1,
        }
    }
    carried
}

/// The target's parameter and option in use named like the source's `parameter_id` and `option_id`.
fn matching_option(
    source: &[Parameter],
    target: &[Parameter],
    parameter_id: ParameterId,
    option_id: OptionId,
) -> Option<(ParameterId, OptionId)> {
    let source = source.iter().find(|p| p.id == parameter_id)?;
    let (label, _) = source.find_option(option_id)?;
    let target = target
        .iter()
        .find(|p| !p.is_archived() && p.name.to_key() == source.name.to_key())?;
    let option = target.options.iter().find(|o| o.label.to_key() == label.to_key())?;
    Some((target.id, option.id))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::parameter::{NewParameter, OptionLabel, ParameterName, ParameterOption, Weight};
    use crate::tag::{NewTag, TagName};
    use crate::{HeapId, Timestamp};

    fn tag(n: u128, name: &str) -> Tag {
        Tag::new(
            TagId::from_uuid(Uuid::from_u128(n)),
            HeapId::from_uuid(Uuid::nil()),
            NewTag {
                name: TagName::parse(name).expect("valid"),
                description: None,
            },
            Timestamp::EPOCH,
        )
    }

    fn parameter(n: u128, name: &str, options: &[(u128, &str)]) -> Parameter {
        Parameter::new(
            ParameterId::from_uuid(Uuid::from_u128(n)),
            HeapId::from_uuid(Uuid::nil()),
            NewParameter {
                name: ParameterName::parse(name).expect("valid"),
                weight: Weight::DEFAULT,
                options: options
                    .iter()
                    .map(|(id, label)| ParameterOption {
                        id: OptionId::from_uuid(Uuid::from_u128(*id)),
                        label: OptionLabel::parse(label).expect("valid"),
                    })
                    .collect(),
                tags: BTreeSet::new(),
            },
            Timestamp::EPOCH,
        )
        .expect("valid")
    }

    fn tag_id(n: u128) -> TagId {
        TagId::from_uuid(Uuid::from_u128(n))
    }

    fn parameter_id(n: u128) -> ParameterId {
        ParameterId::from_uuid(Uuid::from_u128(n))
    }

    fn option_id(n: u128) -> OptionId {
        OptionId::from_uuid(Uuid::from_u128(n))
    }

    #[gtest]
    fn tags_match_by_name_ignoring_case_and_the_rest_are_dropped() {
        let source = [tag(1, "Infra"), tag(2, "later")];
        let target = [tag(11, "infra"), tag(12, "other")];

        let carried = carry_over(
            &BTreeSet::from([tag_id(1), tag_id(2)]),
            &BTreeMap::new(),
            (&source, &[]),
            (&target, &[]),
        );

        expect_that!(carried.tags, elements_are![eq(&tag_id(11))]);
        expect_that!(carried.tags_dropped, eq(1));
    }

    #[gtest]
    fn values_match_by_parameter_name_and_option_label_ignoring_case() {
        let source = [
            parameter(1, "Area", &[(101, "Backend"), (102, "Web")]),
            parameter(2, "Customer", &[(201, "Acme")]),
            parameter(3, "Region", &[(301, "EU")]),
        ];
        let target = [
            parameter(11, "area", &[(111, "backend"), (112, "Mobile")]),
            parameter(12, "Customer", &[(211, "Globex")]),
        ];
        let values = BTreeMap::from([
            (parameter_id(1), option_id(101)),
            (parameter_id(2), option_id(201)),
            (parameter_id(3), option_id(301)),
        ]);

        let carried = carry_over(&BTreeSet::new(), &values, (&[], &source), (&[], &target));

        expect_that!(
            carried.parameters,
            unordered_elements_are![(eq(&parameter_id(11)), eq(&option_id(111)))]
        );
        expect_that!(carried.values_dropped, eq(2));
    }

    #[gtest]
    fn an_archived_target_parameter_does_not_match() {
        let source = [parameter(1, "Area", &[(101, "Backend")])];
        let archived = parameter(11, "Area", &[(111, "Backend")])
            .archive(Timestamp::EPOCH)
            .expect("archives");
        let values = BTreeMap::from([(parameter_id(1), option_id(101))]);

        let carried = carry_over(&BTreeSet::new(), &values, (&[], &source), (&[], &[archived]));

        expect_that!(carried.parameters, is_empty());
        expect_that!(carried.values_dropped, eq(1));
    }
}
