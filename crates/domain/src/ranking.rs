//! Ranking: turns a heap's policy and its items into an ordered list with explanations.
//!
//! The formula is versioned. The server uses [`CURRENT_FORMULA`]; tests pin orderings per version, so a formula
//! change is always an explicit test update. Scores are never stored or returned, only positions and
//! contributors.
//!
//! **Need** counts twice: as a factor of its own, for what skipping the item costs, and as the rate at which the
//! item ages. Ideas age slowly, so an old idea doesn't climb past fresh work someone is waiting for; critical items
//! age fast, because every day they wait costs more.
//!
//! **Custom parameters** count like the heap's own factors: each parameter in use adds its weight times where the
//! item's option sits in the parameter's order, from 1 for the first option to 0 for the last. An unset value
//! counts as the last option (product.md, Priority §4), and an archived option as the option that replaced it.
//!
//! **Held back by** lists the fields the person could change to lift an item: a low need, no due date, unset sizes.
//! It follows the formula's weights and its defaults for unset sizes (product.md, Priority §9). A blocker ranked by
//! what it blocks has none, since its own fields don't move it.
//!
//! **Blockers** take on the priority of the most important item they block, times a small boost, through whole
//! chains. The blocking edges form a forest because cycles are refused on write; the walk still tolerates a cycle
//! in stored data by ignoring the edge that closes it.

use std::collections::{BTreeMap, HashMap};

use crate::due::fuzzy_label;
use crate::heap::HeapPolicy;
use crate::item::{Due, Item, Need, Size};
use crate::parameter::Parameter;
use crate::{ItemId, OptionId, ParameterId, Timestamp};

/// A named version of the scoring formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormulaVersion {
    /// A weighted sum of need, age scaled by need, deadline proximity, value per effort and the custom parameters.
    ///
    /// It replaced `V1`, which had no need. Items it ranked all count as expected, the step that leaves their age
    /// as it was, so they mostly keep their order among themselves. The exception is blockers: expected adds the
    /// same amount to every item, and a blocker takes on 1.1 times the score of what it blocks, so a blocker can
    /// move above an item it was just below.
    V2,
}

pub const CURRENT_FORMULA: FormulaVersion = FormulaVersion::V2;

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
    pub waiting_since: Timestamp,
    pub need: Need,
    pub effort: Option<Size>,
    pub impact: Option<Size>,
    pub due: Option<Due>,
    /// The item this one waits for. Ignored when that item isn't among the ranked items.
    pub blocked_by: Option<ItemId>,
    /// The item's option for each custom parameter that is set.
    pub parameters: &'a BTreeMap<ParameterId, OptionId>,
}

impl<'a> From<&'a Item> for ItemForRanking<'a> {
    fn from(item: &'a Item) -> Self {
        Self {
            id: item.id,
            title: item.title.as_str(),
            waiting_since: item.arrives_at.unwrap_or(item.created_at),
            need: item.need,
            effort: item.effort,
            impact: item.impact,
            due: item.due,
            blocked_by: item.blocked_by(),
            parameters: &item.parameters,
        }
    }
}

/// Which factor a contributor comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Factor {
    Need,
    Age,
    Deadline,
    Value,
    /// A custom parameter.
    Parameter,
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

/// Something that keeps an item from ranking higher, which the person could change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    /// The need is an idea or nice to have.
    Need,
    /// There is no due date.
    Due,
    /// Effort or impact is unset.
    Size,
}

/// One thing holding an item back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldBack {
    pub gap: Gap,
    /// Human-readable explanation, for example "No due date".
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RankedItem {
    pub item_id: ItemId,
    /// 1-based position, highest priority first.
    pub position: u32,
    /// The score the order is by: the item's own, raised to its blocked items' where it blocks them. Scores of
    /// different heaps compare as numbers, which is all "Waiting for me" uses them for.
    pub score: f64,
    /// Largest contributors first.
    pub contributors: Vec<Contributor>,
    /// What keeps it from ranking higher, in a fixed order (need, due, sizes); empty for a blocker ranked by what
    /// it blocks.
    pub held_back_by: Vec<HeldBack>,
}

/// Ranks `items` under `policy` and the heap's `parameters` at `as_of`, highest priority first. Archived
/// parameters don't count.
///
/// An item that blocks others ranks at least as high as each of them. Ties go to the item that blocks the longer
/// chain, then to the older item, then to the lower ID, so the order is fully deterministic.
pub fn rank(
    formula: FormulaVersion,
    policy: &HeapPolicy,
    parameters: &[Parameter],
    items: &[ItemForRanking<'_>],
    as_of: Timestamp,
) -> Vec<RankedItem> {
    let parameters: Vec<&Parameter> = parameters.iter().filter(|p| !p.is_archived()).collect();
    let own: Vec<(f64, Vec<Component>)> = items
        .iter()
        .map(|item| {
            let components = score_components(formula, policy, &parameters, item, as_of);
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
            .then_with(|| a.waiting_since.cmp(&b.waiting_since))
            .then_with(|| a.id.cmp(&b.id))
    });

    scored
        .into_iter()
        .zip(1u32..)
        .map(|((inherited, item, components), position)| RankedItem {
            item_id: item.id,
            position,
            score: inherited.score,
            contributors: select_contributors(components, inherited.score),
            held_back_by: if inherited.via.is_some() {
                Vec::new()
            } else {
                find_gaps(formula, policy, item)
            },
        })
        .collect()
}

/// What keeps `item` from ranking higher: a low need, no due date and unset sizes, each only while its factor
/// counts, since otherwise changing it would move nothing. The need counts through its weight and through the
/// age factor, which it scales. Unset values count as the lowest (product.md, Priority §4), and the item answers
/// "why isn't this higher?" with them (Priority §9). Custom parameters are not named.
fn find_gaps(formula: FormulaVersion, policy: &HeapPolicy, item: &ItemForRanking<'_>) -> Vec<HeldBack> {
    let mut gaps = Vec::new();
    if (policy.need_weight.get() > 0 || policy.age_weight.get() > 0)
        && matches!(item.need, Need::Idea | Need::NiceToHave)
    {
        gaps.push(HeldBack {
            gap: Gap::Need,
            label: format!("Need: {}", item.need.label()),
        });
    }
    if policy.deadline_weight.get() > 0 && item.due.is_none() {
        gaps.push(HeldBack {
            gap: Gap::Due,
            label: "No due date".to_owned(),
        });
    }
    if policy.value_weight.get() > 0 {
        let (unset_effort, unset_impact) = match formula {
            FormulaVersion::V2 => (v2::UNSET_EFFORT, v2::UNSET_IMPACT),
        };
        let label = match (item.effort, item.impact) {
            (None, None) => Some("Effort and impact not set".to_owned()),
            (None, Some(_)) => Some(format!("Effort not set, counts as {}", unset_effort.label())),
            (Some(_), None) => Some(format!("Impact not set, counts as {}", unset_impact.label())),
            (Some(_), Some(_)) => None,
        };
        if let Some(label) = label {
            gaps.push(HeldBack { gap: Gap::Size, label });
        }
    }
    gaps
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
    parameters: &[&Parameter],
    item: &ItemForRanking<'_>,
    as_of: Timestamp,
) -> Vec<Component> {
    match formula {
        FormulaVersion::V2 => v2::components(policy, parameters, item, as_of),
    }
}

/// Factor applied to a score a blocker inherits, because finishing both takes more work than finishing one.
const fn blocker_boost(formula: FormulaVersion) -> f64 {
    match formula {
        FormulaVersion::V2 => v2::BLOCKER_BOOST,
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

mod v2 {
    //! Each factor yields a value of roughly 0 to 1 for typical items, multiplied by the heap's weight for it.

    use super::{Component, Factor, ItemForRanking, fuzzy_label};
    use crate::heap::HeapPolicy;
    use crate::item::{Due, Need, Size};
    use crate::parameter::Parameter;
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
    /// What an unset effort counts as: the largest.
    pub(super) const UNSET_EFFORT: Size = Size::Xl;
    /// What an unset impact counts as: the smallest.
    pub(super) const UNSET_IMPACT: Size = Size::S;

    pub(super) fn components(
        policy: &HeapPolicy,
        parameters: &[&Parameter],
        item: &ItemForRanking<'_>,
        as_of: Timestamp,
    ) -> Vec<Component> {
        let mut components = vec![
            age(policy.age_weight.get(), item.need, item.waiting_since, as_of),
            value(policy.value_weight.get(), item),
        ];
        if let Some(due) = item.due {
            components.push(deadline(policy.deadline_weight.get(), due, as_of));
        }
        components.extend(parameters.iter().filter_map(|parameter| custom(parameter, item)));
        // Last, so a tie in the contributors goes to the other factor: most items are expected, which says less.
        components.push(need(policy.need_weight.get(), item.need));
        components
    }

    /// Where the item's option sits in the parameter's order: 1 for the first, 0 for the last. Unset values add
    /// nothing, the same as the last option; so a parameter with a single option adds nothing either.
    fn custom(parameter: &Parameter, item: &ItemForRanking<'_>) -> Option<Component> {
        let value = item
            .parameters
            .get(&parameter.id)
            .and_then(|option| parameter.resolve(*option))?;
        let last = parameter.options.len().saturating_sub(1);
        let raw = if last == 0 {
            0.0
        } else {
            (last - value.rank) as f64 / last as f64
        };
        Some(Component {
            factor: Factor::Parameter,
            value: f64::from(parameter.weight.get()) * raw,
            label: format!("{}: {}", parameter.name.as_str(), value.option.label.as_str()),
        })
    }

    /// What skipping the item costs. The steps are set against the deadline factor, so they read in its terms: a
    /// nice-to-have pulls like a deadline nine days off, an expected item like one three days off, and a critical
    /// item like an exact deadline that has just passed. An idea adds nothing.
    ///
    /// Expected, the step most items have, is worth half of the best value per effort and of a month of waiting,
    /// so it doesn't crowd them out of the contributors.
    const fn need_points(need: Need) -> f64 {
        match need {
            Need::Idea => 0.0,
            Need::NiceToHave => 0.25,
            Need::Expected => 0.5,
            Need::Critical => 2.0,
        }
    }

    /// How fast an item ages, against an expected item. An idea takes four months to age as much as an expected
    /// item does in one; a critical item ages half again as fast.
    const fn aging_rate(need: Need) -> f64 {
        match need {
            Need::Idea => 0.25,
            Need::NiceToHave => 0.5,
            Need::Expected => 1.0,
            Need::Critical => 1.5,
        }
    }

    fn need(weight: u16, need: Need) -> Component {
        Component {
            factor: Factor::Need,
            value: f64::from(weight) * need_points(need),
            label: format!("Need: {}", need.label()),
        }
    }

    /// Age scaled by need. When the rate isn't an expected item's, the label says so, since an idea's need adds
    /// nothing and has no contributor of its own to explain why it sits low.
    fn age(weight: u16, need: Need, created_at: Timestamp, as_of: Timestamp) -> Component {
        let days = as_of.since(created_at).as_days_f64().max(0.0);
        let raw = aging_rate(need) * (1.0 + days).ln() / (1.0 + AGE_REFERENCE_DAYS).ln();
        Component {
            factor: Factor::Age,
            value: f64::from(weight) * raw,
            label: describe_wait(as_of.since(created_at), need),
        }
    }

    fn describe_wait(waited: Duration, need: Need) -> String {
        let days = waited.as_days_f64().floor() as i64;
        let wait = match days {
            ..1 => return "Added today".to_owned(),
            1 => "Waiting 1 day".to_owned(),
            2..14 => format!("Waiting {days} days"),
            14..61 => format!("Waiting {} weeks", days / 7),
            _ => format!("Waiting {} months", days / 30),
        };
        match need {
            Need::Expected => wait,
            Need::Idea | Need::NiceToHave => format!("{wait} ({}, ages slowly)", need.label().to_lowercase()),
            Need::Critical => format!("{wait} (critical, ages fast)"),
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
        let impact = item.impact.unwrap_or(UNSET_IMPACT);
        let effort = item.effort.unwrap_or(UNSET_EFFORT);
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
            Some(size) => size.label(),
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
    use crate::parameter::Weight;

    fn day(n: i64) -> Timestamp {
        Timestamp::from_millis(1_798_804_800_000 + Duration::from_days(n).as_millis()).expect("in range")
    }

    fn id(n: u128) -> ItemId {
        ItemId::from_uuid(Uuid::from_u128(n))
    }

    static NO_VALUES: BTreeMap<ParameterId, OptionId> = BTreeMap::new();

    fn item(n: u128, created_day: i64) -> ItemForRanking<'static> {
        ItemForRanking {
            id: id(n),
            title: "Item",
            waiting_since: day(created_day),
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            blocked_by: None,
            parameters: &NO_VALUES,
        }
    }

    /// Every heap weight at 0.
    fn all_off() -> HeapPolicy {
        let off = Weight::parse(0).expect("valid");
        HeapPolicy {
            need_weight: off,
            age_weight: off,
            deadline_weight: off,
            value_weight: off,
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

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &items, day(0));

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
    fn the_score_is_what_the_order_is_by() {
        let items = [item(1, 0), item(2, -10), item(3, -5)];

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &items, day(0));

        expect_that!(ranked.windows(2).all(|pair| pair[0].score > pair[1].score), eq(true));
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

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[large, small], day(0));

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

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[old, overdue], day(0));

        expect_that!(order(&ranked), elements_are![eq(&overdue.id), eq(&old.id)]);
        expect_that!(ranked[0].contributors[0].label, eq("Overdue by 2 days"));
        expect_that!(ranked[0].contributors[0].factor, eq(Factor::Deadline));
    }

    #[gtest]
    fn raising_the_deadline_weight_lifts_a_near_deadline_above_age() {
        let old = item(1, -60);
        let soon = ItemForRanking {
            due: Some(Due { at: day(1), firm: true }),
            ..item(2, 0)
        };
        let heavy = HeapPolicy {
            deadline_weight: Weight::parse(100).expect("valid"),
            ..HeapPolicy::default()
        };

        let by_default = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[old, soon], day(0));
        let by_deadline = rank(CURRENT_FORMULA, &heavy, &[], &[old, soon], day(0));

        expect_that!(order(&by_default), elements_are![eq(&old.id), eq(&soon.id)]);
        expect_that!(order(&by_deadline), elements_are![eq(&soon.id), eq(&old.id)]);
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

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[loose, firm], day(0));

        expect_that!(order(&ranked), elements_are![eq(&firm.id), eq(&loose.id)]);
    }

    /// The gaps of an item ranked second, behind a top item with everything set.
    fn held_back_by(policy: &HeapPolicy, second: ItemForRanking<'static>) -> Vec<HeldBack> {
        let top = ItemForRanking {
            need: Need::Critical,
            effort: Some(Size::S),
            impact: Some(Size::Xl),
            due: Some(Due { at: day(1), firm: true }),
            ..item(1, -100)
        };
        let ranked = rank(CURRENT_FORMULA, policy, &[], &[top, second], day(0));
        assert_eq!(ranked[0].item_id, top.id);
        ranked[1].held_back_by.clone()
    }

    /// An item with nothing unset and an expected need.
    fn complete(n: u128) -> ItemForRanking<'static> {
        ItemForRanking {
            effort: Some(Size::M),
            impact: Some(Size::M),
            due: Some(Due {
                at: day(5),
                firm: false,
            }),
            ..item(n, 0)
        }
    }

    fn gap(gap: Gap, label: &str) -> HeldBack {
        HeldBack {
            gap,
            label: label.to_owned(),
        }
    }

    #[gtest]
    fn an_idea_or_nice_to_have_need_holds_an_item_back() {
        let idea = ItemForRanking {
            need: Need::Idea,
            ..complete(2)
        };
        let nice = ItemForRanking {
            need: Need::NiceToHave,
            ..complete(2)
        };

        expect_that!(
            held_back_by(&HeapPolicy::default(), idea),
            elements_are![eq(&gap(Gap::Need, "Need: Idea"))]
        );
        expect_that!(
            held_back_by(&HeapPolicy::default(), nice),
            elements_are![eq(&gap(Gap::Need, "Need: Nice to have"))]
        );
    }

    #[gtest]
    fn expected_and_critical_needs_and_a_set_due_and_sizes_hold_nothing_back() {
        let critical = ItemForRanking {
            need: Need::Critical,
            ..complete(2)
        };
        let past_exact = ItemForRanking {
            due: Some(Due {
                at: day(-3),
                firm: true,
            }),
            ..complete(2)
        };

        expect_that!(held_back_by(&HeapPolicy::default(), complete(2)), is_empty());
        expect_that!(held_back_by(&HeapPolicy::default(), critical), is_empty());
        expect_that!(held_back_by(&HeapPolicy::default(), past_exact), is_empty());
    }

    #[gtest]
    fn no_due_date_holds_an_item_back() {
        let undated = ItemForRanking {
            due: None,
            ..complete(2)
        };

        expect_that!(
            held_back_by(&HeapPolicy::default(), undated),
            elements_are![eq(&gap(Gap::Due, "No due date"))]
        );
    }

    #[gtest]
    fn unset_sizes_hold_an_item_back_in_the_words_of_what_is_missing() {
        let no_effort = ItemForRanking {
            effort: None,
            ..complete(2)
        };
        let no_impact = ItemForRanking {
            impact: None,
            ..complete(2)
        };
        let neither = ItemForRanking {
            effort: None,
            impact: None,
            ..complete(2)
        };

        expect_that!(
            held_back_by(&HeapPolicy::default(), no_effort),
            elements_are![eq(&gap(Gap::Size, "Effort not set, counts as XL"))]
        );
        expect_that!(
            held_back_by(&HeapPolicy::default(), no_impact),
            elements_are![eq(&gap(Gap::Size, "Impact not set, counts as S"))]
        );
        expect_that!(
            held_back_by(&HeapPolicy::default(), neither),
            elements_are![eq(&gap(Gap::Size, "Effort and impact not set"))]
        );
    }

    #[gtest]
    fn a_factor_with_weight_zero_never_holds_an_item_back() {
        let bare = ItemForRanking {
            need: Need::Idea,
            ..item(2, 0)
        };
        let off = Weight::parse(0).expect("valid");
        // Need also scales the age factor, so it counts while either weight is above 0.
        let no_need = HeapPolicy {
            need_weight: off,
            ..HeapPolicy::default()
        };
        let no_need_or_age = HeapPolicy {
            need_weight: off,
            age_weight: off,
            ..HeapPolicy::default()
        };
        let no_deadline = HeapPolicy {
            deadline_weight: off,
            ..HeapPolicy::default()
        };
        let no_value = HeapPolicy {
            value_weight: off,
            ..HeapPolicy::default()
        };

        expect_that!(
            held_back_by(&no_need, bare),
            elements_are![
                eq(&gap(Gap::Need, "Need: Idea")),
                eq(&gap(Gap::Due, "No due date")),
                eq(&gap(Gap::Size, "Effort and impact not set"))
            ]
        );
        expect_that!(
            held_back_by(&no_need_or_age, bare),
            elements_are![
                eq(&gap(Gap::Due, "No due date")),
                eq(&gap(Gap::Size, "Effort and impact not set"))
            ]
        );
        expect_that!(
            held_back_by(&no_deadline, bare),
            elements_are![
                eq(&gap(Gap::Need, "Need: Idea")),
                eq(&gap(Gap::Size, "Effort and impact not set"))
            ]
        );
        expect_that!(
            held_back_by(&no_value, bare),
            elements_are![eq(&gap(Gap::Need, "Need: Idea")), eq(&gap(Gap::Due, "No due date"))]
        );
        expect_that!(held_back_by(&all_off(), bare), is_empty());
    }

    #[gtest]
    fn gaps_come_in_a_fixed_order() {
        let bare = ItemForRanking {
            need: Need::Idea,
            ..item(2, 0)
        };

        expect_that!(
            held_back_by(&HeapPolicy::default(), bare),
            elements_are![
                eq(&gap(Gap::Need, "Need: Idea")),
                eq(&gap(Gap::Due, "No due date")),
                eq(&gap(Gap::Size, "Effort and impact not set"))
            ]
        );
    }

    #[gtest]
    fn a_blocker_ranked_by_what_it_blocks_has_no_gaps() {
        let blocked = ItemForRanking {
            need: Need::Critical,
            effort: Some(Size::S),
            impact: Some(Size::Xl),
            due: Some(Due { at: day(1), firm: true }),
            blocked_by: Some(id(1)),
            ..item(2, 0)
        };
        let blocker = item(1, 0);

        let ranked = rank(
            CURRENT_FORMULA,
            &HeapPolicy::default(),
            &[],
            &[blocker, blocked],
            day(0),
        );

        expect_that!(order(&ranked), elements_are![eq(&blocker.id), eq(&blocked.id)]);
        expect_that!(ranked[0].held_back_by, is_empty());
    }

    #[gtest]
    fn zero_weights_leave_creation_order() {
        let policy = all_off();
        let items = [item(2, 0), item(1, 0)];

        let ranked = rank(CURRENT_FORMULA, &policy, &[], &items, day(1));

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
            &[],
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
            &[],
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
        let policy = all_off();
        // The blocked item is older, so without the tie-break it would come first.
        let blocked = ItemForRanking {
            blocked_by: Some(id(2)),
            ..item(1, -10)
        };
        let blocker = item(2, 0);

        let ranked = rank(CURRENT_FORMULA, &policy, &[], &[blocked, blocker], day(0));

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

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[a, b], day(0));

        expect_that!(ranked.len(), eq(2));
    }

    #[gtest]
    fn a_blocker_outside_the_ranked_items_is_ignored() {
        let blocked = ItemForRanking {
            blocked_by: Some(id(99)),
            ..item(1, 0)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[blocked], day(0));

        expect_that!(order(&ranked), elements_are![eq(&blocked.id)]);
        expect_that!(
            ranked[0].contributors.iter().map(|c| c.factor).collect::<Vec<_>>(),
            not(contains(eq(&Factor::Blocks)))
        );
    }

    #[gtest]
    fn a_critical_item_outranks_a_bigger_win_nobody_is_waiting_for() {
        let critical = ItemForRanking {
            need: Need::Critical,
            effort: Some(Size::M),
            impact: Some(Size::S),
            ..item(1, 0)
        };
        let win = ItemForRanking {
            need: Need::NiceToHave,
            effort: Some(Size::S),
            impact: Some(Size::Xl),
            ..item(2, -30)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[win, critical], day(0));

        expect_that!(order(&ranked), elements_are![eq(&critical.id), eq(&win.id)]);
        expect_that!(ranked[0].contributors[0].factor, eq(Factor::Need));
        expect_that!(ranked[0].contributors[0].label, eq("Need: Critical"));
    }

    #[gtest]
    fn an_old_idea_stays_below_fresh_work_someone_expects() {
        let idea = ItemForRanking {
            need: Need::Idea,
            ..item(1, -365)
        };
        let expected = item(2, -1);

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[idea, expected], day(0));

        expect_that!(order(&ranked), elements_are![eq(&expected.id), eq(&idea.id)]);
    }

    fn reminder(created_day: i64, arrives_day: i64) -> Item {
        use crate::handle::ItemKey;
        use crate::item::{Kind, NewItem, Title};
        use crate::{HeapId, UserId};

        let fields = NewItem {
            kind: Kind::Reminder,
            title: Title::parse("Renew the domain").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: Some(day(arrives_day)),
            tags: Default::default(),
            parameters: BTreeMap::new(),
        };
        Item::new(
            id(1),
            HeapId::from_uuid(Uuid::nil()),
            ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::nil()),
            day(created_day),
        )
    }

    #[gtest]
    fn a_reminders_age_counts_from_its_date_not_its_creation() {
        let reminder = reminder(-90, -3);

        let input = ItemForRanking::from(&reminder);
        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[input], day(0));

        expect_that!(input.waiting_since, eq(day(-3)));
        let age = ranked[0]
            .contributors
            .iter()
            .find(|c| c.factor == Factor::Age)
            .map(|c| c.label.clone());
        expect_that!(age.unwrap_or_default(), starts_with("Waiting 3 days"));
    }

    #[gtest]
    fn need_sets_how_fast_an_item_ages() {
        // With need's own factor off, only the aging rate tells them apart: same age, different needs.
        let policy = HeapPolicy {
            need_weight: Weight::parse(0).expect("valid"),
            ..HeapPolicy::default()
        };
        let items = Need::ALL.map(|need| ItemForRanking {
            need,
            ..item(need as u128 + 1, -60)
        });

        let ranked = rank(CURRENT_FORMULA, &policy, &[], &items, day(0));

        expect_that!(
            order(&ranked),
            elements_are![eq(&items[3].id), eq(&items[2].id), eq(&items[1].id), eq(&items[0].id)]
        );
        expect_that!(
            ranked[0].contributors[0].label,
            eq("Waiting 8 weeks (critical, ages fast)")
        );
        expect_that!(
            ranked[3].contributors[0].label,
            eq("Waiting 8 weeks (idea, ages slowly)")
        );
    }

    #[gtest]
    fn an_idea_has_no_need_contributor() {
        let idea = ItemForRanking {
            need: Need::Idea,
            ..item(1, -10)
        };

        let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &[idea], day(0));

        expect_that!(
            ranked[0].contributors.iter().map(|c| c.factor).collect::<Vec<_>>(),
            not(contains(eq(&Factor::Need)))
        );
    }

    /// A "Customer" parameter with `weight` and one option per label, numbered from 51 in order.
    fn customer(weight: u16, labels: &[&str]) -> crate::parameter::Parameter {
        use crate::parameter::{NewParameter, OptionLabel, ParameterName, ParameterOption, Weight};
        let options = (51..).zip(labels).map(|(n, label)| ParameterOption {
            id: option_id(n),
            label: OptionLabel::parse(label).expect("valid"),
        });
        crate::parameter::Parameter::new(
            ParameterId::from_uuid(Uuid::from_u128(50)),
            crate::HeapId::from_uuid(Uuid::nil()),
            NewParameter {
                name: ParameterName::parse("Customer").expect("valid"),
                weight: Weight::parse(weight.into()).expect("valid"),
                options: options.collect(),
                tags: std::collections::BTreeSet::new(),
            },
            day(0),
        )
        .expect("valid")
    }

    fn option_id(n: u128) -> crate::OptionId {
        crate::OptionId::from_uuid(Uuid::from_u128(n))
    }

    #[gtest]
    fn a_parameter_ranks_its_first_option_highest_and_explains_it() {
        let customer = customer(50, &["Enterprise", "Free"]);
        let enterprise = BTreeMap::from([(customer.id, option_id(51))]);
        let free = BTreeMap::from([(customer.id, option_id(52))]);
        let big = ItemForRanking {
            parameters: &enterprise,
            ..item(1, 0)
        };
        let small = ItemForRanking {
            parameters: &free,
            ..item(2, -20)
        };
        let unset = item(3, -20);

        let ranked = rank(
            CURRENT_FORMULA,
            &HeapPolicy::default(),
            std::slice::from_ref(&customer),
            &[unset, small, big],
            day(0),
        );
        let archived = rank(
            CURRENT_FORMULA,
            &HeapPolicy::default(),
            &[customer.archive(day(0)).expect("changed")],
            &[unset, small, big],
            day(0),
        );

        expect_that!(ranked[0].item_id, eq(big.id));
        expect_that!(ranked[0].contributors[0].factor, eq(Factor::Parameter));
        expect_that!(ranked[0].contributors[0].label, eq("Customer: Enterprise"));
        expect_that!(archived[0].item_id, not(eq(big.id)));
    }

    #[gtest]
    fn a_single_option_or_a_zero_weight_adds_nothing() {
        let single = customer(50, &["Enterprise"]);
        let weightless = customer(0, &["Enterprise", "Free"]);
        let values = BTreeMap::from([(single.id, option_id(51))]);
        let set = ItemForRanking {
            parameters: &values,
            ..item(1, 0)
        };

        for parameter in [single, weightless] {
            let ranked = rank(
                CURRENT_FORMULA,
                &HeapPolicy::default(),
                &[parameter],
                &[set, item(2, 0)],
                day(0),
            );

            expect_that!(
                ranked[0].contributors.iter().map(|c| c.factor).collect::<Vec<_>>(),
                not(contains(eq(&Factor::Parameter)))
            );
        }
    }

    fn arb_size() -> impl Strategy<Value = Option<Size>> {
        prop::option::of(prop::sample::select(Size::ALL.to_vec()))
    }

    fn arb_need() -> impl Strategy<Value = Need> {
        prop::sample::select(Need::ALL.to_vec())
    }

    fn arb_item() -> impl Strategy<Value = ItemForRanking<'static>> {
        (
            any::<u128>(),
            -400i64..0,
            arb_need(),
            arb_size(),
            arb_size(),
            prop::option::of((-60i64..400, any::<bool>())),
        )
            .prop_map(|(id, created, need, effort, impact, due)| ItemForRanking {
                id: ItemId::from_uuid(Uuid::from_u128(id)),
                title: "Item",
                waiting_since: day(created),
                need,
                effort,
                impact,
                due: due.map(|(d, firm)| Due { at: day(d), firm }),
                blocked_by: None,
                parameters: &NO_VALUES,
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
            let first = rank(CURRENT_FORMULA, &policy, &[], &items, day(0));
            items.reverse();
            let second = rank(CURRENT_FORMULA, &policy, &[], &items, day(0));
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

            let before = without(rank(CURRENT_FORMULA, &policy, &[], &items, day(0)));
            let after = without(rank(CURRENT_FORMULA, &policy, &[], &edited, day(0)));

            prop_assert_eq!(before, after);
        }

        #[test]
        fn raising_an_items_need_never_moves_it_down(
            items in prop::collection::vec(arb_item(), 1..40),
            index in any::<prop::sample::Index>(),
        ) {
            let policy = HeapPolicy::default();
            let raised_at = index.index(items.len());
            let mut raised = items.clone();
            raised[raised_at].need = Need::Critical;
            let id = items[raised_at].id;

            let before = position_of(&rank(CURRENT_FORMULA, &policy, &[], &items, day(0)), id);
            let after = position_of(&rank(CURRENT_FORMULA, &policy, &[], &raised, day(0)), id);

            prop_assert!(after <= before);
        }

        #[test]
        fn an_item_blocking_another_never_ranks_below_it(items in arb_forest()) {
            let ranked = rank(CURRENT_FORMULA, &HeapPolicy::default(), &[], &items, day(0));
            for item in &items {
                if let Some(blocker) = item.blocked_by {
                    prop_assert!(position_of(&ranked, blocker) < position_of(&ranked, item.id));
                }
            }
        }

        #[test]
        fn ranking_with_blockers_ignores_input_order(mut items in arb_forest()) {
            let policy = HeapPolicy::default();
            let first = rank(CURRENT_FORMULA, &policy, &[], &items, day(0));
            items.reverse();
            let second = rank(CURRENT_FORMULA, &policy, &[], &items, day(0));
            prop_assert_eq!(first, second);
        }
    }
}
