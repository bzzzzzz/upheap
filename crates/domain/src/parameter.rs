//! Custom parameters: per-heap fields such as "Customer" or "Area", each with an ordered list of options. The order
//! of the options is their priority within the parameter, and the parameter's weight is its influence on the
//! ranking next to age, deadline and value (product.md, Custom parameters).
//!
//! Parameters and options are archived, never deleted (product.md, Nothing is deleted §5):
//! - Archiving an option needs a replacement: open items switch to it, and closed items keep the archived option.
//!   An archived option always names its replacement, so a value can always be followed to an option in use.
//! - A parameter can be archived only when no open item has a value for it, which the service checks.
//!
//! A parameter is edited as a whole: its options and tag links have no version of their own, so every change to
//! them goes through the parameter and its version.

use std::collections::{BTreeMap, BTreeSet};

use crate::text::parse_required;
use crate::validation::{Field, ValidationError};
use crate::{HeapId, OptionId, ParameterId, TagId, TeamId, Timestamp, Version};

/// Most options in use one parameter can have. Real parameters have a handful; this bounds requests and the list.
pub const MAX_OPTIONS: usize = 50;

/// A parameter name: trimmed, 1 to 50 characters. Unique among the parameters in use of a heap and its team,
/// ignoring case.
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

    /// The form labels are compared on: lowercase, so "High" and "high" are the same option.
    pub fn to_key(&self) -> String {
        self.0.to_lowercase()
    }
}

/// A ranking factor's weight: a parameter's or one of the heap's own. It is the factor's influence on the ranking,
/// relative to the other weights. `0` turns it off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weight(u16);

impl Weight {
    pub const MAX: u16 = 100;
    /// The weight of a new parameter, the same as the heap's default weights.
    pub const DEFAULT: Self = Self(10);
    /// A weight that turns its factor off.
    pub const OFF: Self = Self(0);

    /// Takes any integer, so a negative or huge weight from a request is refused as out of range.
    pub fn parse(raw: i64) -> Result<Self, ValidationError> {
        u16::try_from(raw)
            .ok()
            .filter(|w| *w <= Self::MAX)
            .map(Self)
            .ok_or(ValidationError::WeightOutOfRange { max: Self::MAX })
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

/// Who owns a parameter: one heap, or a whole team, whose heaps each choose whether to use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterOwner {
    Heap(HeapId),
    Team(TeamId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub id: ParameterId,
    pub owner: ParameterOwner,
    pub name: ParameterName,
    /// A heap parameter's weight. A team parameter read for a heap carries that heap's weight for it.
    pub weight: Weight,
    /// The options in use, highest priority first.
    pub options: Vec<ParameterOption>,
    /// Archived options, in the order they were archived.
    pub archived_options: Vec<ArchivedOption>,
    /// The tags whose items the parameter is suggested for: the owner's links. A team parameter's are its team links.
    pub tags: BTreeSet<TagId>,
    /// For a team parameter as a heap sees it, the heap's own links; empty otherwise.
    pub heap_tags: BTreeSet<TagId>,
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

/// What merging one parameter's options into another's gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionMerge {
    /// The receiving parameter with the options it took in.
    pub merged: Parameter,
    /// Each option of the merged-in parameter, in use or archived, to the option of `merged` it became.
    pub option_map: BTreeMap<OptionId, OptionId>,
    /// The labels of the options `merged` didn't have: in use first, in the merged-in parameter's order, then archived.
    pub added: Vec<OptionLabel>,
    /// The options of the merged-in parameter in use that matched an archived option of `merged`, in its order: open
    /// items on them move to the option in use that archived option resolves to.
    pub switched: Vec<OptionSwitch>,
}

/// An option in use of a merged-in parameter that open items leave for another option of the team parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionSwitch {
    /// The label as the merged-in parameter wrote it.
    pub from: OptionLabel,
    /// The label of the option in use that open items get.
    pub to: OptionLabel,
}

impl Parameter {
    /// A parameter in use, created at `now`. Fails if the options are too many or two share a label.
    pub fn new(
        id: ParameterId,
        owner: ParameterOwner,
        fields: NewParameter,
        now: Timestamp,
    ) -> Result<Self, ParameterError> {
        let parameter = Self {
            id,
            owner,
            name: fields.name,
            weight: fields.weight,
            options: fields.options,
            archived_options: Vec::new(),
            tags: fields.tags,
            heap_tags: BTreeSet::new(),
            created_at: now,
            archived_at: None,
            version: Version::INITIAL,
        };
        ensure_labels_unique(&parameter.options)?;
        Ok(parameter)
    }

    /// Every tag that suggests the parameter where it is read: the owner's links and the heap's own.
    pub fn suggesting_tags(&self) -> BTreeSet<TagId> {
        self.tags.union(&self.heap_tags).copied().collect()
    }

    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Whether the team owns the parameter.
    pub const fn is_team_owned(&self) -> bool {
        matches!(self.owner, ParameterOwner::Team(_))
    }

    /// The heap parameter as its team's: the same parameter, options and history, with no weight of its own, since each
    /// heap that uses a team parameter weights it for itself. Fails if it is archived or already a team's.
    pub fn into_team(&self, team_id: TeamId) -> Result<Self, ParameterError> {
        self.ensure_in_use()?;
        if self.is_team_owned() {
            return Err(ParameterError::NotInHeap { parameter_id: self.id });
        }
        Ok(Self {
            owner: ParameterOwner::Team(team_id),
            weight: Weight::OFF,
            ..self.clone()
        })
    }

    /// This parameter with the options of `from` taken in, which is merged into it.
    ///
    /// Options match by label, ignoring case, among this parameter's options in use first, then its archived ones,
    /// including those taken in so far. An option of `from` in use with no match is added as this parameter's
    /// lowest-priority option, in `from`'s order. An archived one with no match is added archived, keeping its
    /// `archived_at`, with the option its replacement became as its replacement. `new_id` makes the ID of each option
    /// added. Fails when the result has too many options in use ([`ParameterError::TooManyOptions`]).
    pub fn merge_options(
        &self,
        from: &Self,
        mut new_id: impl FnMut() -> OptionId,
    ) -> Result<OptionMerge, ParameterError> {
        let mut merged = self.clone();
        let mut option_map = BTreeMap::new();
        let mut added = Vec::new();
        let mut switched = Vec::new();
        let mut in_use: BTreeMap<String, OptionId> = self.options.iter().map(|o| (o.label.to_key(), o.id)).collect();
        let mut archived: BTreeMap<String, OptionId> =
            self.archived_options.iter().map(|o| (o.label.to_key(), o.id)).collect();

        for option in &from.options {
            let key = option.label.to_key();
            if let Some(&id) = in_use.get(&key) {
                option_map.insert(option.id, id);
                continue;
            }
            if let Some(&id) = archived.get(&key) {
                let to = merged
                    .resolve(id)
                    .ok_or(ParameterError::ReplacementRequired { option_id: id })?
                    .option
                    .label
                    .clone();
                switched.push(OptionSwitch {
                    from: option.label.clone(),
                    to,
                });
                option_map.insert(option.id, id);
                continue;
            }
            let id = new_id();
            merged.options.push(ParameterOption {
                id,
                label: option.label.clone(),
            });
            in_use.insert(key, id);
            option_map.insert(option.id, id);
            added.push(option.label.clone());
        }

        // An archived option is added once the option that replaced it is mapped, so a replacement is always there
        // before the option that names it.
        let mut pending: Vec<&ArchivedOption> = from.archived_options.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|option| {
                let key = option.label.to_key();
                if let Some(&id) = in_use.get(&key).or_else(|| archived.get(&key)) {
                    option_map.insert(option.id, id);
                    return false;
                }
                let Some(&replaced_by) = option_map.get(&option.replaced_by) else {
                    return true;
                };
                let id = new_id();
                merged.archived_options.push(ArchivedOption {
                    id,
                    label: option.label.clone(),
                    archived_at: option.archived_at,
                    replaced_by,
                });
                archived.insert(key, id);
                option_map.insert(option.id, id);
                added.push(option.label.clone());
                false
            });
            if pending.len() == before {
                return Err(ParameterError::ReplacementRequired {
                    option_id: pending[0].id,
                });
            }
        }
        ensure_labels_unique(&merged.options)?;
        Ok(OptionMerge {
            merged,
            option_map,
            added,
            switched,
        })
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

/// The text a parameter merge's reach is hashed from: for each parameter in the reach, by parameter ID,
/// `"<parameter id>:<items>:<labels added, joined by |>:<switches as from>to, joined by |>\n"`. It changes when a
/// parameter joins or leaves the reach, a count changes or the options it adds or switches change.
pub fn reach_text<'a>(
    entries: impl IntoIterator<Item = (ParameterId, u32, &'a [OptionLabel], &'a [OptionSwitch])>,
) -> String {
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by_key(|(parameter, _, _, _)| *parameter);
    entries
        .into_iter()
        .map(|(parameter, items, added, switched)| {
            let added: Vec<&str> = added.iter().map(OptionLabel::as_str).collect();
            let switched: Vec<String> = switched
                .iter()
                .map(|switch| format!("{}>{}", switch.from.as_str(), switch.to.as_str()))
                .collect();
            format!("{parameter}:{items}:{}:{}\n", added.join("|"), switched.join("|"))
        })
        .collect()
}

/// `values` as they stand for an open item: each value followed to its option in use, and values of archived
/// parameters dropped. A closed item keeps the values it had; this brings them up to date when it reopens, as
/// archiving would have if it had been open then.
pub fn current_values(
    parameters: &[Parameter],
    values: &BTreeMap<ParameterId, OptionId>,
) -> BTreeMap<ParameterId, OptionId> {
    values
        .iter()
        .filter_map(|(&parameter_id, &option_id)| {
            let parameter = parameters.iter().find(|p| p.id == parameter_id && !p.is_archived())?;
            Some((parameter_id, parameter.resolve(option_id)?.option.id))
        })
        .collect()
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
            ParameterOwner::Heap(HeapId::from_uuid(Uuid::nil())),
            fields,
            at(0),
        )
        .expect("valid")
    }

    #[gtest]
    fn a_new_parameter_has_the_owner_it_was_made_for() {
        let team = ParameterOwner::Team(TeamId::from_uuid(Uuid::from_u128(7)));
        let fields = NewParameter {
            name: ParameterName::parse("Customer").expect("valid"),
            weight: Weight::OFF,
            options: vec![option(1, "Enterprise")],
            tags: BTreeSet::new(),
        };

        let created = Parameter::new(ParameterId::from_uuid(Uuid::nil()), team, fields, at(0)).expect("valid");

        expect_that!(created.owner, eq(team));
        expect_that!(created.is_team_owned(), is_true());
        expect_that!(customer().is_team_owned(), is_false());
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
            ParameterOwner::Heap(HeapId::from_uuid(Uuid::nil())),
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
        expect_that!(Weight::parse(Weight::MAX.into()).map(Weight::get), ok(eq(&100)));
        expect_that!(Weight::parse(101), err(anything()));
        expect_that!(Weight::parse(-1), err(anything()));
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

    fn team_parameter(options: Vec<ParameterOption>) -> Parameter {
        Parameter {
            options,
            ..customer()
                .into_team(TeamId::from_uuid(Uuid::from_u128(7)))
                .expect("heap parameter")
        }
    }

    fn heap_parameter(options: Vec<ParameterOption>) -> Parameter {
        Parameter {
            id: ParameterId::from_uuid(Uuid::from_u128(101)),
            options,
            ..customer()
        }
    }

    fn id_source() -> impl FnMut() -> OptionId {
        let mut next = 1000;
        move || {
            next += 1;
            opt(next)
        }
    }

    #[gtest]
    fn a_heap_parameter_becomes_a_team_one_with_the_same_id_and_options_and_no_weight() {
        let team = TeamId::from_uuid(Uuid::from_u128(7));

        let made = customer().into_team(team).expect("heap parameter");

        expect_that!(made.owner, eq(ParameterOwner::Team(team)));
        expect_that!(made.weight, eq(Weight::OFF));
        expect_that!(made.id, eq(customer().id));
        expect_that!(made.options, eq(&customer().options));
        expect_that!(made.into_team(team), err(anything()));
        expect_that!(
            customer().archive(at(1)).expect("changed").into_team(team),
            err(anything())
        );
    }

    #[gtest]
    fn merged_options_match_by_label_ignoring_case() {
        let team = team_parameter(vec![option(1, "Gold"), option(2, "Silver")]);
        let heap = heap_parameter(vec![option(11, "silver"), option(12, "GOLD")]);

        let merge = team.merge_options(&heap, id_source()).expect("fits");

        expect_that!(merge.merged.options, eq(&team.options));
        expect_that!(
            merge.option_map,
            eq(&BTreeMap::from([(opt(11), opt(2)), (opt(12), opt(1))]))
        );
        expect_that!(merge.added, is_empty());
    }

    #[gtest]
    fn merged_options_without_a_match_are_appended_in_their_order() {
        let team = team_parameter(vec![option(1, "Gold")]);
        let heap = heap_parameter(vec![option(11, "Platinum"), option(12, "gold"), option(13, "Bronze")]);

        let merge = team.merge_options(&heap, id_source()).expect("fits");

        let labels: Vec<&str> = merge.merged.options.iter().map(|o| o.label.as_str()).collect();
        expect_that!(labels, elements_are![eq(&"Gold"), eq(&"Platinum"), eq(&"Bronze")]);
        expect_that!(merge.option_map.get(&opt(11)), some(eq(&opt(1001))));
        expect_that!(merge.option_map.get(&opt(12)), some(eq(&opt(1))));
        expect_that!(merge.option_map.get(&opt(13)), some(eq(&opt(1002))));
        let added: Vec<&str> = merge.added.iter().map(OptionLabel::as_str).collect();
        expect_that!(added, elements_are![eq(&"Platinum"), eq(&"Bronze")]);
    }

    #[gtest]
    fn a_live_option_matching_an_archived_one_maps_to_the_archived_one() {
        let team = team_parameter(vec![option(1, "Gold"), option(2, "Silver")])
            .archive_option(opt(2), Some(opt(1)), at(5))
            .expect("allowed")
            .expect("changed");
        let heap = heap_parameter(vec![option(11, "silver")]);

        let merge = team.merge_options(&heap, id_source()).expect("fits");

        expect_that!(merge.option_map.get(&opt(11)), some(eq(&opt(2))));
        expect_that!(merge.merged.options.len(), eq(1));
        expect_that!(merge.added, is_empty());
        expect_that!(
            merge.switched,
            elements_are![eq(&OptionSwitch {
                from: OptionLabel::parse("silver").expect("valid"),
                to: OptionLabel::parse("Gold").expect("valid"),
            })]
        );
    }

    #[gtest]
    fn an_archived_option_without_a_match_is_added_archived_with_its_replacement_mapped() {
        let team = team_parameter(vec![option(1, "Gold")]);
        // 12 Old was replaced by 13 Older (itself archived later), which was replaced by 11 Platinum.
        let heap = Parameter {
            archived_options: vec![
                ArchivedOption {
                    id: opt(12),
                    label: OptionLabel::parse("Old").expect("valid"),
                    archived_at: at(3),
                    replaced_by: opt(13),
                },
                ArchivedOption {
                    id: opt(13),
                    label: OptionLabel::parse("Older").expect("valid"),
                    archived_at: at(4),
                    replaced_by: opt(11),
                },
            ],
            ..heap_parameter(vec![option(11, "Platinum")])
        };

        let merge = team.merge_options(&heap, id_source()).expect("fits");

        let platinum = merge.option_map[&opt(11)];
        let older = merge.option_map[&opt(13)];
        let old = merge.option_map[&opt(12)];
        let archived = &merge.merged.archived_options;
        expect_that!(archived.len(), eq(2));
        // A replacement comes before the option that names it.
        expect_that!(archived[0].id, eq(older));
        expect_that!(archived[0].replaced_by, eq(platinum));
        expect_that!(archived[0].archived_at, eq(at(4)));
        expect_that!(archived[1].id, eq(old));
        expect_that!(archived[1].replaced_by, eq(older));
        expect_that!(merge.merged.resolve(old).map(|v| v.option.id), some(eq(platinum)));
    }

    #[gtest]
    fn two_merges_adding_the_same_label_add_it_once() {
        let team = team_parameter(vec![option(1, "Gold")]);
        let first = heap_parameter(vec![option(11, "Platinum")]);
        let second = heap_parameter(vec![option(21, "platinum")]);

        let after_first = team.merge_options(&first, id_source()).expect("fits");
        let after_second = after_first.merged.merge_options(&second, id_source()).expect("fits");

        expect_that!(after_second.merged.options.len(), eq(2));
        expect_that!(after_second.added, is_empty());
        expect_that!(
            after_second.option_map.get(&opt(21)),
            some(eq(&after_first.option_map[&opt(11)]))
        );
    }

    #[gtest]
    fn a_merge_that_leaves_too_many_options_is_refused() {
        let many: Vec<ParameterOption> = (1..=MAX_OPTIONS as u128).map(|n| option(n, &format!("A{n}"))).collect();
        let team = team_parameter(many);
        let heap = heap_parameter(vec![option(500, "Extra")]);

        expect_that!(
            team.merge_options(&heap, id_source()),
            err(eq(&ParameterError::TooManyOptions { max: MAX_OPTIONS }))
        );
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

    #[gtest]
    fn current_values_follow_replacements_and_drop_archived_parameters() {
        let parameter = customer()
            .archive_option(opt(1), Some(opt(2)), at(1))
            .expect("valid")
            .expect("changed")
            .archive_option(opt(2), Some(opt(3)), at(2))
            .expect("valid")
            .expect("changed");
        let archived_parameter = Parameter {
            id: ParameterId::from_uuid(Uuid::from_u128(200)),
            ..customer().archive(at(1)).expect("changed")
        };
        let values = BTreeMap::from([(parameter.id, opt(1)), (archived_parameter.id, opt(1))]);

        let current = current_values(&[parameter.clone(), archived_parameter], &values);

        expect_that!(current, eq(&BTreeMap::from([(parameter.id, opt(3))])));
    }
}
