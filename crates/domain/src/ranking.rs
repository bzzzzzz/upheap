//! Ranking: turns a heap's policy and its items into an ordered list with explanations.
//!
//! The formula is versioned. The server uses [`CURRENT_FORMULA`]; tests pin orderings per version, so a formula
//! change is always an explicit test update. Scores are never stored or returned, only positions and
//! contributors.
//!
//! **Blockers** take on the priority of the most important item they block, times a small boost, through whole
//! chains. The blocking edges form a forest because cycles are refused on write; the walk still tolerates a cycle
//! in stored data by ignoring the edge that closes it.

use std::collections::HashMap;

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
pub struct ItemForRanking<'a> {
    pub id: ItemId,
    /// Shown in the "Blocks …" contributor of the item that blocks this one.
    pub title: &'a str,
    pub created_at: Timestamp,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    pub due: Option<Due>,
    /// The item this one waits for. Ignored when that item isn't among the ranked items.
    pub blocked_by: Option<ItemId>,
}

impl<'a> From<&'a Item> for ItemForRanking<'a> {
    fn from(item: &'a Item) -> Self {
        Self {
            id: item.id,
            title: item.title.as_str(),
            created_at: item.created_at,
            effort: item.effort,
            impact: item.impact,
            due: item.due,
            blocked_by: item.blocked_by(),
        }
    }
}

/// Which factor a contributor comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factor {
    Age,
    Deadline,
    Value,
    /// Priority inherited from an item this one blocks.
    Blocks,
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
/// An item that blocks others ranks at least as high as each of them. Ties go to the item that blocks the longer
/// chain, then to the older item, then to the lower ID, so the order is fully deterministic.
pub fn rank(
    formula: FormulaVersion,
    policy: &HeapPolicy,
    items: &[ItemForRanking<'_>],
    as_of: Timestamp,
) -> Vec<RankedItem> {
    let own: Vec<(f64, Vec<Component>)> = items
        .iter()
        .map(|item| {
            let components = score_components(formula, policy, item, as_of);
            let total = components.iter().map(|c| c.value).sum();
            (total, components)
        })
        .collect();
    let inherited = inherit_from_blocked(items, &own, blocker_boost(formula));

    let mut scored: Vec<(Inherited, &ItemForRanking<'_>, Vec<Component>)> = own
        .into_iter()
        .zip(inherited)
        .zip(items)
        .map(|(((own_total, mut components), inherited), item)| {
            if let Some(via) = inherited.via {
                components.push(Component {
                    factor: Factor::Blocks,
                    value: inherited.score - own_total,
                    label: format!("Blocks “{}”", items[via].title),
                });
            }
            (inherited, item, components)
        })
        .collect();

    scored.sort_by(|(score_a, a, _), (score_b, b, _)| {
        score_b
            .score
            .total_cmp(&score_a.score)
            .then_with(|| score_b.height.cmp(&score_a.height))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    scored
        .into_iter()
        .zip(1u32..)
        .map(|((inherited, item, components), position)| RankedItem {
            item_id: item.id,
            position,
            contributors: select_contributors(components, inherited.score),
        })
        .collect()
}

/// An item's score after taking on the priority of the items it blocks.
#[derive(Clone, Copy, Debug, Default)]
struct Inherited {
    score: f64,
    /// Index of the directly blocked item the score came from, when it beat the item's own score.
    via: Option<usize>,
    /// Length of the longest chain of items this one blocks, so a blocker wins a tie with what it blocks.
    height: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    New,
    Open,
    Done,
}

/// Each item's score as the maximum of its own and `boost` times the inherited score of each item it blocks.
///
/// A memoized depth-first walk, iterative so long chains can't overflow the stack: linear in items and edges.
fn inherit_from_blocked(items: &[ItemForRanking<'_>], own: &[(f64, Vec<Component>)], boost: f64) -> Vec<Inherited> {
    let index: HashMap<ItemId, usize> = items.iter().enumerate().map(|(i, item)| (item.id, i)).collect();
    // `blocked[b]` lists the items that wait for item `b`, by ID so the result doesn't depend on input order.
    let mut blocked: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    for (i, item) in items.iter().enumerate() {
        if let Some(&b) = item.blocked_by.and_then(|id| index.get(&id)) {
            blocked[b].push(i);
        }
    }
    for list in &mut blocked {
        list.sort_by_key(|&i| items[i].id);
    }

    let mut result = vec![Inherited::default(); items.len()];
    let mut visit = vec![Visit::New; items.len()];
    for start in 0..items.len() {
        if visit[start] != Visit::New {
            continue;
        }
        visit[start] = Visit::Open;
        let mut stack = vec![(start, 0usize)];
        while let Some((node, next)) = stack.last_mut() {
            let node = *node;
            if let Some(&child) = blocked[node].get(*next) {
                *next += 1;
                // An `Open` child closes a cycle in stored data; its edge is ignored.
                if visit[child] == Visit::New {
                    visit[child] = Visit::Open;
                    stack.push((child, 0));
                }
                continue;
            }
            let mut best = Inherited {
                score: own[node].0,
                via: None,
                height: 0,
            };
            for &child in blocked[node].iter().filter(|&&c| visit[c] == Visit::Done) {
                let candidate = result[child].score * boost;
                if candidate > best.score {
                    best.score = candidate;
                    best.via = Some(child);
                }
                best.height = best.height.max(result[child].height + 1);
            }
            result[node] = best;
            visit[node] = Visit::Done;
            stack.pop();
        }
    }
    result
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
    item: &ItemForRanking<'_>,
    as_of: Timestamp,
) -> Vec<Component> {
    match formula {
        FormulaVersion::V1 => v1::components(policy, item, as_of),
    }
}

/// Factor applied to a score a blocker inherits, because finishing both takes more work than finishing one.
const fn blocker_boost(formula: FormulaVersion) -> f64 {
    match formula {
        FormulaVersion::V1 => v1::BLOCKER_BOOST,
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
    /// A blocker ranks as if it were this much more important than the item it blocks.
    pub(super) const BLOCKER_BOOST: f64 = 1.1;

    pub(super) fn components(policy: &HeapPolicy, item: &ItemForRanking<'_>, as_of: Timestamp) -> Vec<Component> {
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
    fn value(weight: u16, item: &ItemForRanking<'_>) -> Component {
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

    fn id(n: u128) -> ItemId {
        ItemId::from_uuid(Uuid::from_u128(n))
    }

    fn item(n: u128, created_day: i64) -> ItemForRanking<'static> {
        ItemForRanking {
            id: id(n),
            title: "Item",
            created_at: day(created_day),
            effort: None,
            impact: None,
            due: None,
            blocked_by: None,
        }
    }

    fn position_of(ranked: &[RankedItem], item: ItemId) -> usize {
        ranked.iter().position(|r| r.item_id == item).expect("ranked")
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

    #[gtest]
    fn a_blocker_ranks_above_what_it_blocks() {
        let blocked = ItemForRanking {
            title: "Ship release",
            due: Some(Due {
                at: day(-2),
                firm: true,
            }),
            blocked_by: Some(id(1)),
            ..item(2, 0)
        };
        let blocker = item(1, 0);
        let middle = item(3, -30);

        let ranked = rank(
            CURRENT_FORMULA,
            &HeapPolicy::default(),
            &[middle, blocked, blocker],
            day(0),
        );

        expect_that!(
            order(&ranked),
            elements_are![eq(&blocker.id), eq(&blocked.id), eq(&middle.id)]
        );
        expect_that!(ranked[0].contributors[0].factor, eq(Factor::Blocks));
        expect_that!(ranked[0].contributors[0].label, eq("Blocks “Ship release”"));
    }

    #[gtest]
    fn inheritance_carries_through_chains() {
        // 1 blocks 2, which blocks 3; only 3 is urgent.
        let urgent = ItemForRanking {
            title: "Urgent",
            due: Some(Due {
                at: day(-5),
                firm: true,
            }),
            blocked_by: Some(id(2)),
            ..item(3, 0)
        };
        let middle = ItemForRanking {
            title: "Middle",
            blocked_by: Some(id(1)),
            ..item(2, 0)
        };
        let root = item(1, 0);
        let other = item(4, -60);

        let ranked = rank(
            CURRENT_FORMULA,
            &HeapPolicy::default(),
            &[other, urgent, middle, root],
            day(0),
        );

        expect_that!(
            order(&ranked),
            elements_are![eq(&root.id), eq(&middle.id), eq(&urgent.id), eq(&other.id)]
        );
        expect_that!(ranked[0].contributors[0].label, eq("Blocks “Middle”"));
    }

    #[gtest]
    fn a_blocker_wins_a_tie_with_what_it_blocks() {
        let policy = HeapPolicy {
            age_weight: 0,
            deadline_weight: 0,
            value_weight: 0,
        };
        // The blocked item is older, so without the tie-break it would come first.
        let blocked = ItemForRanking {
            blocked_by: Some(id(2)),
            ..item(1, -10)
        };
        let blocker = item(2, 0);

        let ranked = rank(CURRENT_FORMULA, &policy, &[blocked, blocker], day(0));

        expect_that!(order(&ranked), elements_are![eq(&blocker.id), eq(&blocked.id)]);
    }

    #[gtest]
    fn a_cycle_in_stored_data_still_ranks_every_item() {
        let a = ItemForRanking {
            blocked_by: Some(id(2)),
            ..item(1, 0)
        };
        let b = ItemForRanking {
            blocked_by: Some(id(1)),
            ..item(2, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[a, b], day(0));

        expect_that!(ranked.len(), eq(2));
    }

    #[gtest]
    fn a_blocker_outside_the_ranked_items_is_ignored() {
        let blocked = ItemForRanking {
            blocked_by: Some(id(99)),
            ..item(1, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[blocked], day(0));

        expect_that!(order(&ranked), elements_are![eq(&blocked.id)]);
        expect_that!(
            ranked[0].contributors.iter().map(|c| c.factor).collect::<Vec<_>>(),
            not(contains(eq(&Factor::Blocks)))
        );
    }

    fn arb_size() -> impl Strategy<Value = Option<Size>> {
        prop::option::of(prop::sample::select(Size::ALL.to_vec()))
    }

    fn arb_item() -> impl Strategy<Value = ItemForRanking<'static>> {
        (
            any::<u128>(),
            -400i64..0,
            arb_size(),
            arb_size(),
            prop::option::of((-60i64..400, any::<bool>())),
        )
            .prop_map(|(id, created, effort, impact, due)| ItemForRanking {
                id: ItemId::from_uuid(Uuid::from_u128(id)),
                title: "Item",
                created_at: day(created),
                effort,
                impact,
                due: due.map(|(d, firm)| Due { at: day(d), firm }),
                blocked_by: None,
            })
    }

    /// Items where each may wait for an earlier one, which can't form a cycle.
    fn arb_forest() -> impl Strategy<Value = Vec<ItemForRanking<'static>>> {
        prop::collection::vec((arb_item(), prop::option::of(any::<prop::sample::Index>())), 1..40).prop_map(|entries| {
            let ids: Vec<ItemId> = entries.iter().map(|(item, _)| item.id).collect();
            entries
                .into_iter()
                .enumerate()
                .map(|(i, (item, blocker))| ItemForRanking {
                    blocked_by: blocker.filter(|_| i > 0).map(|b| ids[b.index(i)]),
                    ..item
                })
                .collect()
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

        #[test]
        fn an_item_blocking_another_never_ranks_below_it(items in arb_forest()) {
            let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &items, day(0));
            for item in &items {
                if let Some(blocker) = item.blocked_by {
                    prop_assert!(position_of(&ranked, blocker) < position_of(&ranked, item.id));
                }
            }
        }

        #[test]
        fn ranking_with_blockers_ignores_input_order(mut items in arb_forest()) {
            let policy = HeapPolicy::default();
            let first = rank(CURRENT_FORMULA, &policy, &items, day(0));
            items.reverse();
            let second = rank(CURRENT_FORMULA, &policy, &items, day(0));
            prop_assert_eq!(first, second);
        }
    }
}
