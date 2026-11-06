//! Ranking: turns a heap's policy and its items into an ordered list with explanations.
//!
//! The formula is versioned. The server uses [`CURRENT_FORMULA`]; tests pin orderings per version, so a formula
//! change is always an explicit test update. Scores are never stored or returned, only positions and
//! contributors.

use crate::due::fuzzy_label;
use crate::heap::HeapPolicy;
use crate::item::{Due, Item, Size};
use crate::{ItemId, Timestamp};

/// A named version of the scoring formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaVersion {
    /// Placeholder formula for the first release: a weighted sum of age, deadline proximity and value per
    /// effort. To be replaced by the formula designed separately.
    V1,
}

pub const CURRENT_FORMULA: FormulaVersion = FormulaVersion::V1;

/// At most this many contributors are returned per item.
pub const MAX_CONTRIBUTORS: usize = 2;

/// Contributors after the first are shown only from this share of the score up, so noise isn't presented as a
/// reason.
pub const MIN_SECONDARY_SHARE_PERCENT: u8 = 10;

/// The inputs ranking needs from an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemForRanking {
    pub id: ItemId,
    pub created_at: Timestamp,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    pub due: Option<Due>,
}

impl From<&Item> for ItemForRanking {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id,
            created_at: item.created_at,
            effort: item.effort,
            impact: item.impact,
            due: item.due,
        }
    }
}

/// Which factor a contributor comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factor {
    Age,
    Deadline,
    Value,
}

/// One reason for an item's position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contributor {
    pub factor: Factor,
    /// Human-readable explanation, for example "Waiting 3 weeks".
    pub label: String,
    /// This factor's share of the item's score, in whole percent (1 to 100).
    pub share_percent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankedItem {
    pub item_id: ItemId,
    /// 1-based position, highest priority first.
    pub position: u32,
    /// Largest contributors first.
    pub contributors: Vec<Contributor>,
}

/// Ranks `items` under `policy` at `as_of`, highest priority first.
///
/// Ties go to the older item, then to the lower ID, so the order is fully deterministic.
pub fn rank(
    formula: FormulaVersion,
    policy: &HeapPolicy,
    items: &[ItemForRanking],
    as_of: Timestamp,
) -> Vec<RankedItem> {
    let mut scored: Vec<(f64, &ItemForRanking, Vec<Component>)> = items
        .iter()
        .map(|item| {
            let components = score_components(formula, policy, item, as_of);
            let total = components.iter().map(|c| c.value).sum();
            (total, item, components)
        })
        .collect();

    scored.sort_by(|(score_a, a, _), (score_b, b, _)| {
        score_b
            .total_cmp(score_a)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    scored
        .into_iter()
        .zip(1u32..)
        .map(|((total, item, components), position)| RankedItem {
            item_id: item.id,
            position,
            contributors: select_contributors(components, total),
        })
        .collect()
}

/// One factor's weighted part of a score, with its explanation.
struct Component {
    factor: Factor,
    value: f64,
    label: String,
}

fn score_components(
    formula: FormulaVersion,
    policy: &HeapPolicy,
    item: &ItemForRanking,
    as_of: Timestamp,
) -> Vec<Component> {
    match formula {
        FormulaVersion::V1 => v1::components(policy, item, as_of),
    }
}

fn select_contributors(mut components: Vec<Component>, total: f64) -> Vec<Contributor> {
    if total <= 0.0 {
        return Vec::new();
    }
    components.sort_by(|a, b| b.value.total_cmp(&a.value));
    components
        .into_iter()
        .filter(|c| c.value > 0.0)
        .map(|c| Contributor {
            factor: c.factor,
            label: c.label,
            share_percent: ((c.value / total) * 100.0).round().clamp(1.0, 100.0) as u8,
        })
        .enumerate()
        .filter(|(index, c)| *index == 0 || c.share_percent >= MIN_SECONDARY_SHARE_PERCENT)
        .map(|(_, c)| c)
        .take(MAX_CONTRIBUTORS)
        .collect()
}

mod v1 {
    //! Each factor yields a value of roughly 0 to 1 for typical items, multiplied by the heap's weight for it.

    use super::{Component, Factor, ItemForRanking, fuzzy_label};
    use crate::heap::HeapPolicy;
    use crate::item::{Due, Size};
    use crate::{Duration, Timestamp};

    /// Age at which the age factor reaches 1. It keeps growing slowly after that.
    const AGE_REFERENCE_DAYS: f64 = 30.0;
    /// Days before an exact deadline at which its pull is half of what it is on the day.
    const DEADLINE_HALF_DAYS: f64 = 3.0;
    /// Past an exact deadline the factor starts at this value and climbs by 1 per week overdue.
    const OVERDUE_BASE: f64 = 2.0;
    /// Past a loose due date the factor climbs by this much per week.
    const LOOSE_PAST_RATE: f64 = 0.1;

    pub(super) fn components(policy: &HeapPolicy, item: &ItemForRanking, as_of: Timestamp) -> Vec<Component> {
        let mut components = vec![
            age(policy.age_weight, item.created_at, as_of),
            value(policy.value_weight, item),
        ];
        if let Some(due) = item.due {
            components.push(deadline(policy.deadline_weight, due, as_of));
        }
        components
    }

    fn age(weight: u16, created_at: Timestamp, as_of: Timestamp) -> Component {
        let days = as_of.since(created_at).as_days_f64().max(0.0);
        let raw = (1.0 + days).ln() / (1.0 + AGE_REFERENCE_DAYS).ln();
        Component {
            factor: Factor::Age,
            value: f64::from(weight) * raw,
            label: describe_wait(as_of.since(created_at)),
        }
    }

    fn describe_wait(waited: Duration) -> String {
        let days = waited.as_days_f64().floor() as i64;
        match days {
            ..1 => "Added today".to_owned(),
            1 => "Waiting 1 day".to_owned(),
            2..14 => format!("Waiting {days} days"),
            14..61 => format!("Waiting {} weeks", days / 7),
            _ => format!("Waiting {} months", days / 30),
        }
    }

    fn deadline(weight: u16, due: Due, as_of: Timestamp) -> Component {
        let days_left = due.at.since(as_of).as_days_f64();
        let raw = match (due.firm, days_left >= 0.0) {
            (_, true) => 1.0 / (1.0 + days_left / DEADLINE_HALF_DAYS),
            (true, false) => OVERDUE_BASE + (-days_left) / 7.0,
            (false, false) => 1.0 + LOOSE_PAST_RATE * (-days_left) / 7.0,
        };
        let label = match (due.firm, days_left >= 0.0) {
            (false, _) => format!("Due {}", fuzzy_label(due.at, as_of)),
            (true, true) => describe_days_left(days_left.floor() as i64),
            (true, false) => describe_overdue((-days_left).ceil() as i64),
        };
        Component {
            factor: Factor::Deadline,
            value: f64::from(weight) * raw,
            label,
        }
    }

    fn describe_days_left(days: i64) -> String {
        match days {
            0 => "Due within a day".to_owned(),
            1 => "Due in 1 day".to_owned(),
            _ => format!("Due in {days} days"),
        }
    }

    fn describe_overdue(days: i64) -> String {
        match days {
            ..=1 => "Overdue by 1 day".to_owned(),
            _ => format!("Overdue by {days} days"),
        }
    }

    /// Value per effort. Unset impact counts as the smallest, unset effort as the largest.
    fn value(weight: u16, item: &ItemForRanking) -> Component {
        let impact = item.impact.unwrap_or(Size::S);
        let effort = item.effort.unwrap_or(Size::Xl);
        let ratio = points(impact) / points(effort);
        // log2 of the ratio runs from -3 to 3; map that onto 0 to 1.
        let raw = (ratio.log2() + 3.0) / 6.0;
        let label = format!(
            "{} impact, {} effort",
            describe_size(item.impact),
            describe_size(item.effort)
        );
        Component {
            factor: Factor::Value,
            value: f64::from(weight) * raw,
            label,
        }
    }

    const fn points(size: Size) -> f64 {
        match size {
            Size::S => 1.0,
            Size::M => 2.0,
            Size::L => 4.0,
            Size::Xl => 8.0,
        }
    }

    const fn describe_size(size: Option<Size>) -> &'static str {
        match size {
            None => "unset",
            Some(Size::S) => "S",
            Some(Size::M) => "M",
            Some(Size::L) => "L",
            Some(Size::Xl) => "XL",
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::Duration;

    fn day(n: i64) -> Timestamp {
        Timestamp::from_millis(1_798_804_800_000 + Duration::from_days(n).as_millis()).expect("in range")
    }

    fn item(n: u128, created_day: i64) -> ItemForRanking {
        ItemForRanking {
            id: ItemId::from_uuid(Uuid::from_u128(n)),
            created_at: day(created_day),
            effort: None,
            impact: None,
            due: None,
        }
    }

    fn order(ranked: &[RankedItem]) -> Vec<ItemId> {
        ranked.iter().map(|r| r.item_id).collect()
    }

    #[gtest]
    fn older_items_rank_first_when_otherwise_equal() {
        let items = [item(1, 0), item(2, -10), item(3, -5)];

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &items, day(0));

        expect_that!(
            order(&ranked),
            elements_are![eq(&items[1].id), eq(&items[2].id), eq(&items[0].id)]
        );
        expect_that!(
            ranked.iter().map(|r| r.position).collect::<Vec<_>>(),
            elements_are![eq(&1), eq(&2), eq(&3)]
        );
    }

    #[gtest]
    fn small_high_impact_beats_large_same_impact() {
        let small = ItemForRanking {
            effort: Some(Size::S),
            impact: Some(Size::L),
            ..item(1, 0)
        };
        let large = ItemForRanking {
            effort: Some(Size::Xl),
            impact: Some(Size::L),
            ..item(2, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[large, small], day(0));

        expect_that!(order(&ranked), elements_are![eq(&small.id), eq(&large.id)]);
    }

    #[gtest]
    fn overdue_deadline_outranks_old_age() {
        let old = item(1, -60);
        let overdue = ItemForRanking {
            due: Some(Due {
                at: day(-2),
                firm: true,
            }),
            ..item(2, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[old, overdue], day(0));

        expect_that!(order(&ranked), elements_are![eq(&overdue.id), eq(&old.id)]);
        expect_that!(ranked[0].contributors[0].label, eq("Overdue by 2 days"));
        expect_that!(ranked[0].contributors[0].factor, eq(Factor::Deadline));
    }

    #[gtest]
    fn exact_deadline_outranks_the_same_loose_date_once_past() {
        let firm = ItemForRanking {
            due: Some(Due {
                at: day(-7),
                firm: true,
            }),
            ..item(1, 0)
        };
        let loose = ItemForRanking {
            due: Some(Due {
                at: day(-7),
                firm: false,
            }),
            ..item(2, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[loose, firm], day(0));

        expect_that!(order(&ranked), elements_are![eq(&firm.id), eq(&loose.id)]);
    }

    #[gtest]
    fn zero_weights_leave_creation_order() {
        let policy = HeapPolicy {
            age_weight: 0,
            deadline_weight: 0,
            value_weight: 0,
        };
        let items = [item(2, 0), item(1, 0)];

        let ranked = rank(CURRENT_FORMULA, &policy, &items, day(1));

        expect_that!(order(&ranked), elements_are![eq(&items[1].id), eq(&items[0].id)]);
        expect_that!(ranked[0].contributors, is_empty());
    }

    fn arb_size() -> impl Strategy<Value = Option<Size>> {
        prop::option::of(prop::sample::select(Size::ALL.to_vec()))
    }

    fn arb_item() -> impl Strategy<Value = ItemForRanking> {
        (
            any::<u128>(),
            -400i64..0,
            arb_size(),
            arb_size(),
            prop::option::of((-60i64..400, any::<bool>())),
        )
            .prop_map(|(id, created, effort, impact, due)| ItemForRanking {
                id: ItemId::from_uuid(Uuid::from_u128(id)),
                created_at: day(created),
                effort,
                impact,
                due: due.map(|(d, firm)| Due { at: day(d), firm }),
            })
    }

    proptest! {
        #[test]
        fn ranking_ignores_input_order(mut items in prop::collection::vec(arb_item(), 0..40)) {
            let policy = HeapPolicy::default();
            let first = rank(CURRENT_FORMULA, &policy, &items, day(0));
            items.reverse();
            let second = rank(CURRENT_FORMULA, &policy, &items, day(0));
            prop_assert_eq!(first, second);
        }

        #[test]
        fn editing_one_item_keeps_the_others_in_order(
            items in prop::collection::vec(arb_item(), 2..40),
            effort in arb_size(),
            index in any::<prop::sample::Index>(),
        ) {
            let policy = HeapPolicy::default();
            let edited_at = index.index(items.len());
            let mut edited = items.clone();
            edited[edited_at].effort = effort;
            let edited_id = items[edited_at].id;
            let without = |ranked: Vec<RankedItem>| -> Vec<ItemId> {
                ranked.into_iter().map(|r| r.item_id).filter(|id| *id != edited_id).collect()
            };

            let before = without(rank(CURRENT_FORMULA, &policy, &items, day(0)));
            let after = without(rank(CURRENT_FORMULA, &policy, &edited, day(0)));

            prop_assert_eq!(before, after);
        }
    }
}
