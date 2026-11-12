//! How the ranked list is shown: which states are pinned, shown or hidden, which items the filters keep, and what
//! the list is sorted by.
//!
//! Ranking always runs over every non-closed item, because a blocker inherits the priority of what it blocks even
//! when that item is hidden or filtered out. These rules then pick and order what the caller sees, so filtering
//! keeps the relative priority order of the items that match.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::item::{Item, Need, Size, StateKind};
use crate::parameter::Parameter;
use crate::{OptionId, ParameterId, TagId};

/// Where items in one state go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateMode {
    /// In the pinned group above the rest.
    Pinned,
    /// In the main list (blocked items may have their own group, see [`BlockedLayout`]).
    Shown,
    /// Left out. The response counts them, so clients can say that something is hidden.
    Hidden,
}

impl StateMode {
    pub const ALL: [Self; 3] = [Self::Pinned, Self::Shown, Self::Hidden];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Shown => "shown",
            Self::Hidden => "hidden",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.code() == code)
    }
}

/// Where shown blocked items go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedLayout {
    /// Their own group, by priority.
    Grouped,
    /// Mixed in with the open items, in the list's sort order.
    Mixed,
}

impl BlockedLayout {
    pub const ALL: [Self; 2] = [Self::Grouped, Self::Mixed];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Grouped => "grouped",
            Self::Mixed => "mixed",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|layout| layout.code() == code)
    }
}

/// Whether the heap's reminders to come are listed, in a group of their own after the others. They have no
/// position: they are not ranked until their date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RemindersMode {
    /// Left out. A search shows them anyway.
    #[default]
    Hidden,
    Shown,
}

impl RemindersMode {
    pub const ALL: [Self; 2] = [Self::Hidden, Self::Shown];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Shown => "shown",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.code() == code)
    }
}

/// What a group of the list is sorted by. Ties keep priority order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    /// Combined priority, highest first.
    Priority,
    /// Soonest due first; items without a due time last.
    Due,
    /// Smallest effort first; items without an effort last.
    Effort,
    /// Largest impact first; items without an impact last.
    Impact,
    /// The custom parameter's highest-priority option first; items without a value last.
    Parameter(ParameterId),
}

impl SortKey {
    /// The keys that need no parameter.
    pub const FIXED: [Self; 4] = [Self::Priority, Self::Due, Self::Effort, Self::Impact];

    /// Stable code used in storage, audit diffs and the API. Never renamed. A parameter sort also stores the
    /// parameter, see [`Self::parameter`].
    pub const fn code(self) -> &'static str {
        match self {
            Self::Priority => "priority",
            Self::Due => "due",
            Self::Effort => "effort",
            Self::Impact => "impact",
            Self::Parameter(_) => "parameter",
        }
    }

    /// The parameter a parameter sort sorts by.
    pub const fn parameter(self) -> Option<ParameterId> {
        match self {
            Self::Parameter(parameter) => Some(parameter),
            Self::Priority | Self::Due | Self::Effort | Self::Impact => None,
        }
    }

    /// The key with `code`, given a parameter exactly when the code is `parameter`. `None` for an unknown code or
    /// a parameter that doesn't fit it.
    pub fn from_parts(code: &str, parameter: Option<ParameterId>) -> Option<Self> {
        match parameter {
            Some(parameter) => (code == "parameter").then_some(Self::Parameter(parameter)),
            None => Self::FIXED.into_iter().find(|key| key.code() == code),
        }
    }
}

/// The groups of the ranked list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Pinned,
    Ranked,
    Blocked,
}

/// Which items to keep. An empty set doesn't filter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// Keep items with at least one of these tags.
    pub tags: BTreeSet<TagId>,
    /// Keep items with one of these needs.
    pub need: BTreeSet<Need>,
    /// Keep items with one of these efforts.
    pub effort: BTreeSet<Size>,
    /// Keep items with one of these impacts.
    pub impact: BTreeSet<Size>,
    /// Options of custom parameters. Keep items with one of the options listed for each parameter they belong to.
    /// An archived option matches the items that switched to its replacement. Options of archived parameters
    /// don't filter, as those parameters no longer count anywhere else.
    pub options: BTreeSet<OptionId>,
}

impl ListFilter {
    /// Whether `item` passes the filter. `parameters` are the heap's parameters; filter options that belong to
    /// none of them, or to an archived one, are ignored.
    pub fn matches(&self, item: &Item, parameters: &[Parameter]) -> bool {
        let tags = self.tags.is_empty() || !self.tags.is_disjoint(&item.tags);
        tags && (self.need.is_empty() || self.need.contains(&item.need))
            && within(&self.effort, item.effort)
            && within(&self.impact, item.impact)
            && self.matches_options(item, parameters)
    }

    fn matches_options(&self, item: &Item, parameters: &[Parameter]) -> bool {
        if self.options.is_empty() {
            return true;
        }
        parameters.iter().filter(|p| !p.is_archived()).all(|parameter| {
            let wanted: BTreeSet<OptionId> = self
                .options
                .iter()
                .filter_map(|option| parameter.resolve(*option).map(|v| v.option.id))
                .collect();
            if wanted.is_empty() {
                return true;
            }
            item.parameters
                .get(&parameter.id)
                .and_then(|option| parameter.resolve(*option))
                .is_some_and(|value| wanted.contains(&value.option.id))
        })
    }
}

fn within(allowed: &BTreeSet<Size>, size: Option<Size>) -> bool {
    allowed.is_empty() || size.is_some_and(|size| allowed.contains(&size))
}

/// The caller's view of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListView {
    pub open: StateMode,
    pub in_progress: StateMode,
    pub blocked: StateMode,
    pub blocked_layout: BlockedLayout,
    pub reminders: RemindersMode,
    pub filter: ListFilter,
    pub sort: SortKey,
}

impl Default for ListView {
    /// In-progress items pinned, open items shown, blocked items hidden, by priority (product.md, List items §2).
    fn default() -> Self {
        Self {
            open: StateMode::Shown,
            in_progress: StateMode::Pinned,
            blocked: StateMode::Hidden,
            blocked_layout: BlockedLayout::Grouped,
            reminders: RemindersMode::Hidden,
            filter: ListFilter::default(),
            sort: SortKey::Priority,
        }
    }
}

impl ListView {
    /// Whether the view lists items in `state`, pinned or shown, before its filters.
    pub fn lists(&self, state: StateKind) -> bool {
        self.group_of(state).is_some()
    }

    /// The states the view lists. For the default view these are the items a heap counts as open: its list's
    /// `unfiltered_count`, and the heap's `open_count`.
    pub fn listed_states(&self) -> Vec<StateKind> {
        StateKind::ALL.into_iter().filter(|state| self.lists(*state)).collect()
    }

    /// The group an item in `state` goes to, or `None` when its state is hidden or closed.
    pub fn group_of(&self, state: StateKind) -> Option<Group> {
        let (mode, shown_group) = match state {
            StateKind::Open => (self.open, Group::Ranked),
            StateKind::InProgress => (self.in_progress, Group::Ranked),
            StateKind::Blocked => (
                self.blocked,
                match self.blocked_layout {
                    BlockedLayout::Grouped => Group::Blocked,
                    BlockedLayout::Mixed => Group::Ranked,
                },
            ),
            StateKind::Done | StateKind::WontDo => return None,
        };
        match mode {
            StateMode::Pinned => Some(Group::Pinned),
            StateMode::Shown => Some(shown_group),
            StateMode::Hidden => None,
        }
    }

    /// Sorts `group`, which is in priority order, by [`Self::sort`]. The sort is stable, so ties keep priority
    /// order. Grouped blocked items always stay in priority order (product.md, List items §3). `parameters` are the
    /// heap's parameters; a sort by a parameter that isn't among them, or is archived, keeps priority order.
    /// Whether `group` is listed in priority order: under the priority sort, and always for blocked items.
    pub fn in_priority_order(&self, group: Group) -> bool {
        self.sort == SortKey::Priority || group == Group::Blocked
    }

    pub fn sort<T>(&self, group: Group, entries: &mut [T], item: impl Fn(&T) -> &Item, parameters: &[Parameter]) {
        if self.in_priority_order(group) {
            return;
        }
        let key = self.sort;
        let parameter = key
            .parameter()
            .and_then(|id| parameters.iter().find(|p| p.id == id && !p.is_archived()));
        entries.sort_by(|a, b| compare(key, parameter, item(a), item(b)));
    }
}

fn compare(key: SortKey, parameter: Option<&Parameter>, a: &Item, b: &Item) -> Ordering {
    match key {
        SortKey::Priority => Ordering::Equal,
        SortKey::Due => missing_last(a.due.map(|d| d.at), b.due.map(|d| d.at), Ord::cmp),
        SortKey::Effort => missing_last(a.effort, b.effort, Ord::cmp),
        SortKey::Impact => missing_last(a.impact, b.impact, |a, b| b.cmp(a)),
        SortKey::Parameter(_) => {
            let Some(parameter) = parameter else {
                return Ordering::Equal;
            };
            let rank = |item: &Item| {
                item.parameters
                    .get(&parameter.id)
                    .and_then(|option| parameter.resolve(*option))
                    .map(|value| value.rank)
            };
            missing_last(rank(a), rank(b), Ord::cmp)
        }
    }
}

/// Orders by `order`, with `None` after every value.
fn missing_last<T>(a: Option<T>, b: Option<T>, order: impl Fn(&T, &T) -> Ordering) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => order(&a, &b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::item::{Due, Kind, Need, NewItem, Title};
    use crate::{HeapId, ItemId, Timestamp};

    fn tag(n: u128) -> TagId {
        TagId::from_uuid(Uuid::from_u128(n))
    }

    fn item(n: u128) -> Item {
        let fields = NewItem {
            kind: Kind::Note,
            title: Title::parse(&format!("Item {n}")).expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: None,
            tags: BTreeSet::new(),
            parameters: std::collections::BTreeMap::new(),
        };
        let created = Timestamp::from_millis(0).expect("in range");
        Item::new(
            ItemId::from_uuid(Uuid::from_u128(n)),
            HeapId::from_uuid(Uuid::nil()),
            crate::handle::ItemKey::parse("k3x9a").expect("valid"),
            fields,
            crate::UserId::from_uuid(Uuid::nil()),
            created,
        )
    }

    fn sorted(view: &ListView, items: &[Item]) -> Vec<u128> {
        let mut items = items.to_vec();
        view.sort(Group::Ranked, &mut items, |item| item, &[]);
        items.iter().map(|item| item.id.as_uuid().as_u128()).collect()
    }

    fn by(sort: SortKey) -> ListView {
        ListView {
            sort,
            ..ListView::default()
        }
    }

    #[gtest]
    fn the_default_view_lists_open_and_in_progress_items() {
        expect_that!(
            ListView::default().listed_states(),
            eq(&vec![StateKind::Open, StateKind::InProgress])
        );
        let everything = ListView {
            blocked: StateMode::Shown,
            ..ListView::default()
        };
        expect_that!(
            everything.listed_states(),
            eq(&vec![StateKind::Open, StateKind::InProgress, StateKind::Blocked])
        );
    }

    #[gtest]
    fn codes_read_back_as_the_same_value() {
        expect_that!(
            StateMode::ALL.map(|m| StateMode::from_code(m.code())),
            eq(StateMode::ALL.map(Some))
        );
        expect_that!(
            BlockedLayout::ALL.map(|l| BlockedLayout::from_code(l.code())),
            eq(BlockedLayout::ALL.map(Some))
        );
        expect_that!(
            SortKey::FIXED.map(|k| SortKey::from_parts(k.code(), None)),
            eq(SortKey::FIXED.map(Some))
        );
        let parameter = ParameterId::from_uuid(Uuid::nil());
        expect_that!(
            SortKey::from_parts("parameter", Some(parameter)),
            some(eq(SortKey::Parameter(parameter)))
        );
        expect_that!(SortKey::from_parts("parameter", None), none());
        expect_that!(SortKey::from_parts("due", Some(parameter)), none());
        expect_that!(SortKey::from_parts("rank", None), none());
    }

    #[gtest]
    fn by_default_in_progress_is_pinned_open_is_shown_and_blocked_is_hidden() {
        let view = ListView::default();

        expect_that!(view.group_of(StateKind::InProgress), some(eq(Group::Pinned)));
        expect_that!(view.group_of(StateKind::Open), some(eq(Group::Ranked)));
        expect_that!(view.group_of(StateKind::Blocked), none());
        expect_that!(view.group_of(StateKind::Done), none());
    }

    #[gtest]
    fn shown_blocked_items_are_grouped_or_mixed_in() {
        let grouped = ListView {
            blocked: StateMode::Shown,
            ..ListView::default()
        };
        let mixed = ListView {
            blocked_layout: BlockedLayout::Mixed,
            ..grouped.clone()
        };

        expect_that!(grouped.group_of(StateKind::Blocked), some(eq(Group::Blocked)));
        expect_that!(mixed.group_of(StateKind::Blocked), some(eq(Group::Ranked)));
    }

    #[gtest]
    fn pinned_blocked_items_are_pinned_whatever_the_layout() {
        let view = ListView {
            blocked: StateMode::Pinned,
            blocked_layout: BlockedLayout::Mixed,
            ..ListView::default()
        };

        expect_that!(view.group_of(StateKind::Blocked), some(eq(Group::Pinned)));
    }

    #[gtest]
    fn the_tag_filter_keeps_items_with_any_of_its_tags() {
        let filter = ListFilter {
            tags: BTreeSet::from([tag(1), tag(2)]),
            ..ListFilter::default()
        };
        let mut second = item(1);
        second.tags = BTreeSet::from([tag(2), tag(3)]);
        let mut other = item(2);
        other.tags = BTreeSet::from([tag(3)]);

        expect_that!(filter.matches(&second, &[]), eq(true));
        expect_that!(filter.matches(&other, &[]), eq(false));
        expect_that!(filter.matches(&item(3), &[]), eq(false));
    }

    #[gtest]
    fn a_size_filter_drops_items_without_that_size() {
        let filter = ListFilter {
            effort: BTreeSet::from([Size::S]),
            ..ListFilter::default()
        };
        let mut small = item(1);
        small.effort = Some(Size::S);
        let mut large = item(2);
        large.effort = Some(Size::L);

        expect_that!(filter.matches(&small, &[]), eq(true));
        expect_that!(filter.matches(&large, &[]), eq(false));
        expect_that!(filter.matches(&item(3), &[]), eq(false));
    }

    #[gtest]
    fn the_need_filter_keeps_items_with_one_of_its_needs() {
        let filter = ListFilter {
            need: BTreeSet::from([Need::Critical, Need::Idea]),
            ..ListFilter::default()
        };
        let mut critical = item(1);
        critical.need = Need::Critical;
        let mut idea = item(2);
        idea.need = Need::Idea;

        expect_that!(filter.matches(&critical, &[]), eq(true));
        expect_that!(filter.matches(&idea, &[]), eq(true));
        expect_that!(filter.matches(&item(3), &[]), eq(false));
    }

    #[gtest]
    fn an_empty_filter_keeps_everything() {
        expect_that!(ListFilter::default().matches(&item(1), &[]), eq(true));
    }

    #[gtest]
    fn sorting_by_priority_keeps_the_order() {
        let items = [item(3), item(1), item(2)];

        expect_that!(
            sorted(&by(SortKey::Priority), &items),
            elements_are![eq(&3), eq(&1), eq(&2)]
        );
    }

    #[gtest]
    fn sorting_by_due_puts_the_soonest_first_and_undated_items_last() {
        let due = |n, day: i64| {
            let mut item = item(n);
            item.due = Some(Due {
                at: Timestamp::from_millis(day * 86_400_000).expect("in range"),
                firm: false,
            });
            item
        };
        let items = [item(1), due(2, 9), due(3, 4), item(4)];

        expect_that!(
            sorted(&by(SortKey::Due), &items),
            elements_are![eq(&3), eq(&2), eq(&1), eq(&4)]
        );
    }

    #[gtest]
    fn sorting_by_effort_puts_the_smallest_first_and_ties_keep_priority_order() {
        let sized = |n, size| {
            let mut item = item(n);
            item.effort = Some(size);
            item
        };
        let items = [sized(1, Size::L), item(2), sized(3, Size::S), sized(4, Size::L)];

        expect_that!(
            sorted(&by(SortKey::Effort), &items),
            elements_are![eq(&3), eq(&1), eq(&4), eq(&2)]
        );
    }

    #[gtest]
    fn grouped_blocked_items_stay_in_priority_order() {
        let mut items = vec![item(1), item(2)];
        items[1].effort = Some(Size::S);

        by(SortKey::Effort).sort(Group::Blocked, &mut items, |item| item, &[]);

        let ids: Vec<u128> = items.iter().map(|item| item.id.as_uuid().as_u128()).collect();
        expect_that!(ids, elements_are![eq(&1), eq(&2)]);
    }

    #[gtest]
    fn sorting_by_impact_puts_the_largest_first_and_unset_last() {
        let sized = |n, size| {
            let mut item = item(n);
            item.impact = Some(size);
            item
        };
        let items = [item(1), sized(2, Size::S), sized(3, Size::Xl)];

        expect_that!(
            sorted(&by(SortKey::Impact), &items),
            elements_are![eq(&3), eq(&2), eq(&1)]
        );
    }

    fn area() -> Parameter {
        use crate::parameter::{NewParameter, OptionLabel, ParameterName, ParameterOption, Weight};
        let option = |n: u128, label: &str| ParameterOption {
            id: OptionId::from_uuid(Uuid::from_u128(n)),
            label: OptionLabel::parse(label).expect("valid"),
        };
        let fields = NewParameter {
            name: ParameterName::parse("Area").expect("valid"),
            weight: Weight::DEFAULT,
            options: vec![option(11, "Infra"), option(12, "Web"), option(13, "Docs")],
            tags: BTreeSet::new(),
        };
        Parameter::new(
            ParameterId::from_uuid(Uuid::from_u128(10)),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            Timestamp::from_millis(0).expect("in range"),
        )
        .expect("valid")
    }

    fn valued(n: u128, option: Option<u128>) -> Item {
        let mut item = item(n);
        if let Some(option) = option {
            item.parameters.insert(
                ParameterId::from_uuid(Uuid::from_u128(10)),
                OptionId::from_uuid(Uuid::from_u128(option)),
            );
        }
        item
    }

    #[gtest]
    fn the_option_filter_follows_archived_options_to_their_replacement() {
        let area = area()
            .archive_option(
                OptionId::from_uuid(Uuid::from_u128(12)),
                Some(OptionId::from_uuid(Uuid::from_u128(13))),
                Timestamp::from_millis(1).expect("in range"),
            )
            .expect("allowed")
            .expect("changed");
        let filter = ListFilter {
            options: BTreeSet::from([OptionId::from_uuid(Uuid::from_u128(12))]),
            ..ListFilter::default()
        };
        let parameters = [area];

        expect_that!(filter.matches(&valued(1, Some(13)), &parameters), eq(true));
        expect_that!(filter.matches(&valued(2, Some(11)), &parameters), eq(false));
        expect_that!(filter.matches(&valued(3, None), &parameters), eq(false));
    }

    #[gtest]
    fn an_archived_parameter_neither_filters_nor_sorts() {
        let at = Timestamp::from_millis(1).expect("in range");
        let parameters = [area().archive(at).expect("changed")];
        let filter = ListFilter {
            options: BTreeSet::from([OptionId::from_uuid(Uuid::from_u128(11))]),
            ..ListFilter::default()
        };
        let view = by(SortKey::Parameter(ParameterId::from_uuid(Uuid::from_u128(10))));
        let mut items = vec![valued(1, None), valued(2, Some(13)), valued(3, Some(11))];

        view.sort(Group::Ranked, &mut items, |item| item, &parameters);

        expect_that!(filter.matches(&valued(4, Some(13)), &parameters), eq(true));
        let ids: Vec<u128> = items.iter().map(|item| item.id.as_uuid().as_u128()).collect();
        expect_that!(ids, elements_are![eq(&1), eq(&2), eq(&3)]);
    }

    #[gtest]
    fn sorting_by_a_parameter_puts_its_first_option_first_and_unset_last() {
        let parameters = [area()];
        let view = by(SortKey::Parameter(ParameterId::from_uuid(Uuid::from_u128(10))));
        let mut items = vec![
            valued(1, None),
            valued(2, Some(13)),
            valued(3, Some(11)),
            valued(4, Some(13)),
        ];

        view.sort(Group::Ranked, &mut items, |item| item, &parameters);

        let ids: Vec<u128> = items.iter().map(|item| item.id.as_uuid().as_u128()).collect();
        expect_that!(ids, elements_are![eq(&3), eq(&2), eq(&4), eq(&1)]);
    }
}
