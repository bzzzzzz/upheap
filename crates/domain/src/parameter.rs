//! Custom parameters: per-heap fields such as "Customer" or "Area", each with an ordered list of options. The order
//! of the options is their priority within the parameter, and the parameter's weight is its influence on the
//! ranking next to age, deadline and value (Req Custom parameters).
//!
//! Parameters and options are archived, never deleted (Off Nothing is deleted §5):
//! - Archiving an option needs a replacement: open items switch to it, and closed items keep the archived option.
//!   An archived option always names its replacement, so a value can always be followed to an option in use.
//! - A parameter can be archived only when no open item has a value for it, which the service checks.
//!
//! A parameter is edited as a whole: its options and tag links have no version of their own, so every change to
//! them goes through the parameter and its version.

use std::collections::BTreeSet;

use crate::text::parse_required;
use crate::validation::{Field, ValidationError};
use crate::{HeapId, OptionId, ParameterId, TagId, Timestamp, Version};

/// Most options in use one parameter can have. Real parameters have a handful; this bounds requests and the list.
pub const MAX_OPTIONS: usize = 50;

/// A parameter name: trimmed, 1 to 50 characters. Unique among a heap's parameters in use, ignoring case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterName(String);

impl ParameterName {
    pub const MAX_CHARS: usize = 50;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Name, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The form uniqueness is checked on: lowercase, so "Customer" and "customer" are the same parameter.
    pub fn to_key(&self) -> String {
        self.0.to_lowercase()
    }
}

/// An option label: trimmed, 1 to 50 characters. Unique among a parameter's options in use, ignoring case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionLabel(String);

impl OptionLabel {
    pub const MAX_CHARS: usize = 50;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Label, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn to_key(&self) -> String {
        self.0.to_lowercase()
    }
}

/// A parameter's influence on the ranking, relative to the heap's other weights. `0` turns it off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weight(u16);

impl Weight {
    pub const MAX: u16 = 100;
    /// The weight of a new parameter, the same as the heap's default weights.
    pub const DEFAULT: Self = Self(10);

    pub const fn parse(raw: u16) -> Result<Self, ValidationError> {
        if raw > Self::MAX {
            return Err(ValidationError::WeightTooLarge { max: Self::MAX });
        }
        Ok(Self(raw))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// An option in use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterOption {
    pub id: OptionId,
    pub label: OptionLabel,
}

/// An archived option. Open items that had it now have `replaced_by`; closed items keep it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedOption {
    pub id: OptionId,
    pub label: OptionLabel,
    pub archived_at: Timestamp,
    /// The option open items switched to. It may have been archived since, with a replacement of its own.
    pub replaced_by: OptionId,
}

/// Why a change to a parameter, or a value for one, was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParameterError {
    #[error("parameter {parameter_id} is archived")]
    Archived { parameter_id: ParameterId },
    #[error("a parameter has at most {max} options in use")]
    TooManyOptions { max: usize },
    #[error("this parameter already has an option called {label:?}")]
    OptionLabelTaken { label: String },
    /// A new order for the options must list every option in use exactly once, and nothing else.
    #[error("list every option of parameter {parameter_id} in use exactly once")]
    OptionsMismatch { parameter_id: ParameterId },
    /// An option is archived only with a replacement: another option of the same parameter that is in use.
    #[error("option {option_id} needs a replacement: another option of its parameter that is in use")]
    ReplacementRequired { option_id: OptionId },
    /// A value names a parameter that isn't one of the heap's parameters in use.
    #[error("parameter {parameter_id} is not a parameter in use in this heap")]
    NotInHeap { parameter_id: ParameterId },
    /// A value names an option that isn't one of the parameter's options in use.
    #[error("option {option_id} is not an option in use of parameter {parameter_id}")]
    OptionNotInParameter {
        parameter_id: ParameterId,
        option_id: OptionId,
    },
}

/// The fields of a new parameter. The caller gives each option its ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewParameter {
    pub name: ParameterName,
    pub weight: Weight,
    /// Highest priority first.
    pub options: Vec<ParameterOption>,
    /// Tags of the same heap. The service checks they belong to it.
    pub tags: BTreeSet<TagId>,
}

/// A partial edit of the name or weight. `None` leaves the field unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParameterPatch {
    pub name: Option<ParameterName>,
    pub weight: Option<Weight>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub id: ParameterId,
    pub heap_id: HeapId,
    pub name: ParameterName,
    pub weight: Weight,
    /// The options in use, highest priority first.
    pub options: Vec<ParameterOption>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOption>,
    /// The tags whose items the parameter is suggested for.
    pub tags: BTreeSet<TagId>,
    pub created_at: Timestamp,
    /// `None` while the parameter is in use.
    pub archived_at: Option<Timestamp>,
    pub version: Version,
}

/// An item's value for a parameter, resolved to an option in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedValue<'a> {
    pub option: &'a ParameterOption,
    /// 0 for the highest-priority option.
    pub rank: usize,
}

impl Parameter {
    /// A parameter in use, created at `now`. Fails if the options are too many or two share a label.
    pub fn new(id: ParameterId, heap_id: HeapId, fields: NewParameter, now: Timestamp) -> Result<Self, ParameterError> {
        let parameter = Self {
            id,
            heap_id,
            name: fields.name,
            weight: fields.weight,
            options: fields.options,
            archived_options: Vec::new(),
            tags: fields.tags,
            created_at: now,
            archived_at: None,
            version: Version::INITIAL,
        };
        ensure_labels_unique(&parameter.options)?;
        Ok(parameter)
    }

    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// The parameter with `patch` applied, or `None` when the patch changes nothing. Keeps the old version; the
    /// store bumps it on write.
    pub fn apply_patch(&self, patch: &ParameterPatch) -> Result<Option<Self>, ParameterError> {
        self.ensure_in_use()?;
        let edited = Self {
            name: patch.name.clone().unwrap_or_else(|| self.name.clone()),
            weight: patch.weight.unwrap_or(self.weight),
            ..self.clone()
        };
        Ok((edited != *self).then_some(edited))
    }

    /// The parameter with `option` added as its lowest-priority option.
    pub fn add_option(&self, option: ParameterOption) -> Result<Self, ParameterError> {
        self.ensure_in_use()?;
        let mut edited = self.clone();
        edited.options.push(option);
        ensure_labels_unique(&edited.options)?;
        Ok(edited)
    }

    /// The parameter with its options in use relabelled and put in the order of `options`, highest priority first,
    /// or `None` when nothing changes. `options` must name every option in use exactly once.
    pub fn arrange_options(&self, options: Vec<ParameterOption>) -> Result<Option<Self>, ParameterError> {
        self.ensure_in_use()?;
        let current: BTreeSet<OptionId> = self.options.iter().map(|o| o.id).collect();
        let listed: BTreeSet<OptionId> = options.iter().map(|o| o.id).collect();
        if listed != current || options.len() != current.len() {
            return Err(ParameterError::OptionsMismatch { parameter_id: self.id });
        }
        ensure_labels_unique(&options)?;
        let edited = Self {
            options,
            ..self.clone()
        };
        Ok((edited != *self).then_some(edited))
    }

    /// The parameter with `option_id` archived at `now` and replaced by `replacement`, or `None` when it is
    /// archived already.
    ///
    /// The replacement must be another option of this parameter in use. `option_id` must be one of its options.
    pub fn archive_option(
        &self,
        option_id: OptionId,
        replacement: Option<OptionId>,
        now: Timestamp,
    ) -> Result<Option<Self>, ParameterError> {
        if self.archived_options.iter().any(|o| o.id == option_id) {
            return Ok(None);
        }
        self.ensure_in_use()?;
        let required = ParameterError::ReplacementRequired { option_id };
        let replaced_by = replacement.ok_or_else(|| required.clone())?;
        let replacement_in_use = self.options.iter().any(|o| o.id == replaced_by);
        let Some(index) = self.options.iter().position(|o| o.id == option_id) else {
            return Err(required);
        };
        if replaced_by == option_id || !replacement_in_use {
            return Err(required);
        }
        let mut edited = self.clone();
        let option = edited.options.remove(index);
        edited.archived_options.push(ArchivedOption {
            id: option.id,
            label: option.label,
            archived_at: now,
            replaced_by,
        });
        Ok(Some(edited))
    }

    /// The parameter suggested for exactly `tags`, or `None` when it already is.
    pub fn link_tags(&self, tags: BTreeSet<TagId>) -> Result<Option<Self>, ParameterError> {
        self.ensure_in_use()?;
        if tags == self.tags {
            return Ok(None);
        }
        Ok(Some(Self { tags, ..self.clone() }))
    }

    /// The parameter archived at `now`, or `None` when it is archived already. The service checks first that no
    /// open item has a value for it.
    pub fn archive(&self, now: Timestamp) -> Option<Self> {
        if self.is_archived() {
            return None;
        }
        Some(Self {
            archived_at: Some(now),
            ..self.clone()
        })
    }

    /// The label of `option_id`, in use or archived, with whether it is archived.
    pub fn find_option(&self, option_id: OptionId) -> Option<(&OptionLabel, bool)> {
        if let Some(option) = self.options.iter().find(|o| o.id == option_id) {
            return Some((&option.label, false));
        }
        self.archived_options
            .iter()
            .find(|o| o.id == option_id)
            .map(|o| (&o.label, true))
    }

    /// `option_id` as an option in use: itself, or the replacement it was archived with, followed until one is in
    /// use. `None` when it isn't an option of this parameter.
    pub fn resolve(&self, option_id: OptionId) -> Option<ResolvedValue<'_>> {
        let mut current = option_id;
        // Each archived option is followed at most once, so a loop in stored data can't hang.
        for _ in 0..=self.archived_options.len() {
            if let Some(rank) = self.options.iter().position(|o| o.id == current) {
                return Some(ResolvedValue {
                    option: &self.options[rank],
                    rank,
                });
            }
            current = self.archived_options.iter().find(|o| o.id == current)?.replaced_by;
        }
        None
    }

    const fn ensure_in_use(&self) -> Result<(), ParameterError> {
        if self.is_archived() {
            return Err(ParameterError::Archived { parameter_id: self.id });
        }
        Ok(())
    }
}

fn ensure_labels_unique(options: &[ParameterOption]) -> Result<(), ParameterError> {
    if options.len() > MAX_OPTIONS {
        return Err(ParameterError::TooManyOptions { max: MAX_OPTIONS });
    }
    let mut keys = BTreeSet::new();
    for option in options {
        if !keys.insert(option.label.to_key()) {
            return Err(ParameterError::OptionLabelTaken {
                label: option.label.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

/// Checks the values an item edit sets against the heap's parameters in use: a value names a parameter in use and
/// one of its options in use. A cleared value (`None`) may name any parameter of the heap, archived ones included,
/// so a value of an archived parameter can still be taken off.
pub fn check_values(
    parameters: &[Parameter],
    values: impl IntoIterator<Item = (ParameterId, Option<OptionId>)>,
) -> Result<(), ParameterError> {
    for (parameter_id, value) in values {
        let parameter = parameters.iter().find(|p| p.id == parameter_id);
        let Some(option_id) = value else {
            if parameter.is_none() {
                return Err(ParameterError::NotInHeap { parameter_id });
            }
            continue;
        };
        let parameter = parameter
            .filter(|p| !p.is_archived())
            .ok_or(ParameterError::NotInHeap { parameter_id })?;
        if !parameter.options.iter().any(|o| o.id == option_id) {
            return Err(ParameterError::OptionNotInParameter {
                parameter_id,
                option_id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn option(n: u128, label: &str) -> ParameterOption {
        ParameterOption {
            id: OptionId::from_uuid(Uuid::from_u128(n)),
            label: OptionLabel::parse(label).expect("valid"),
        }
    }

    fn opt(n: u128) -> OptionId {
        OptionId::from_uuid(Uuid::from_u128(n))
    }

    fn at(millis: i64) -> Timestamp {
        Timestamp::from_millis(millis).expect("in range")
    }

    fn customer() -> Parameter {
        let fields = NewParameter {
            name: ParameterName::parse("Customer").expect("valid"),
            weight: Weight::DEFAULT,
            options: vec![option(1, "Enterprise"), option(2, "Pro"), option(3, "Free")],
            tags: BTreeSet::new(),
        };
        Parameter::new(
            ParameterId::from_uuid(Uuid::from_u128(100)),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            at(0),
        )
        .expect("valid")
    }

    #[gtest]
    fn option_labels_are_unique_ignoring_case() {
        let fields = NewParameter {
            name: ParameterName::parse("Area").expect("valid"),
            weight: Weight::DEFAULT,
            options: vec![option(1, "Infra"), option(2, "infra")],
            tags: BTreeSet::new(),
        };

        let created = Parameter::new(
            ParameterId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            at(0),
        );

        expect_that!(
            created,
            err(eq(&ParameterError::OptionLabelTaken {
                label: "infra".to_owned()
            }))
        );
    }

    #[gtest]
    fn weights_above_the_maximum_are_refused() {
        expect_that!(Weight::parse(Weight::MAX).map(Weight::get), ok(eq(&100)));
        expect_that!(Weight::parse(101), err(anything()));
    }

    #[gtest]
    fn options_are_rearranged_only_when_every_option_in_use_is_listed() {
        let parameter = customer();
        let reversed = vec![option(3, "Free"), option(2, "Pro"), option(1, "Enterprise")];
        let missing = vec![option(3, "Free"), option(2, "Pro")];
        let doubled = vec![option(3, "Free"), option(3, "Free"), option(1, "Enterprise")];

        let arranged = parameter
            .arrange_options(reversed.clone())
            .expect("allowed")
            .expect("changed");

        expect_that!(arranged.options, eq(&reversed));
        expect_that!(parameter.arrange_options(missing), err(anything()));
        expect_that!(parameter.arrange_options(doubled), err(anything()));
        expect_that!(parameter.arrange_options(parameter.options.clone()), ok(none()));
    }

    #[gtest]
    fn an_archived_option_needs_another_option_in_use_as_its_replacement() {
        let parameter = customer();

        expect_that!(
            parameter.archive_option(opt(2), None, at(1)),
            err(eq(&ParameterError::ReplacementRequired { option_id: opt(2) }))
        );
        expect_that!(parameter.archive_option(opt(2), Some(opt(2)), at(1)), err(anything()));
        expect_that!(parameter.archive_option(opt(2), Some(opt(99)), at(1)), err(anything()));
        let archived = parameter
            .archive_option(opt(2), Some(opt(3)), at(1))
            .expect("allowed")
            .expect("changed");
        expect_that!(archived.options.len(), eq(2));
        expect_that!(archived.archive_option(opt(2), None, at(2)), ok(none()));
    }

    #[gtest]
    fn values_follow_replacements_to_an_option_in_use() {
        let parameter = customer()
            .archive_option(opt(1), Some(opt(2)), at(1))
            .expect("allowed")
            .expect("changed")
            .archive_option(opt(2), Some(opt(3)), at(2))
            .expect("allowed")
            .expect("changed");

        let resolved = parameter.resolve(opt(1));

        expect_that!(resolved.map(|v| v.option.id), some(eq(opt(3))));
        expect_that!(resolved.map(|v| v.rank), some(eq(0)));
        expect_that!(parameter.resolve(opt(99)), none());
    }

    #[gtest]
    fn an_archived_parameter_refuses_edits() {
        let archived = customer().archive(at(1)).expect("changed");
        let rename = ParameterPatch {
            name: Some(ParameterName::parse("Client").expect("valid")),
            ..ParameterPatch::default()
        };

        expect_that!(
            archived.apply_patch(&rename),
            err(eq(&ParameterError::Archived {
                parameter_id: archived.id
            }))
        );
        expect_that!(archived.archive(at(2)), none());
    }

    #[gtest]
    fn values_must_name_an_option_in_use_of_a_parameter_in_use() {
        let parameter = customer();
        let archived_parameter = Parameter {
            id: ParameterId::from_uuid(Uuid::from_u128(200)),
            ..customer().archive(at(1)).expect("changed")
        };
        let parameters = [parameter.clone(), archived_parameter.clone()];
        let check = |p: ParameterId, o: Option<OptionId>| check_values(&parameters, [(p, o)]);

        expect_that!(check(parameter.id, Some(opt(1))), ok(anything()));
        expect_that!(check(parameter.id, Some(opt(99))), err(anything()));
        expect_that!(check(archived_parameter.id, Some(opt(1))), err(anything()));
        expect_that!(check(archived_parameter.id, None), ok(anything()));
        expect_that!(
            check(ParameterId::from_uuid(Uuid::from_u128(300)), None),
            err(anything())
        );
    }
}
