//! Heaps and their ranking policy.

use crate::handle::Handle;
use crate::parameter::Weight;
use crate::text::{Context, Name};
use crate::{HeapId, TeamId, Timestamp, Version};

/// The heap's ranking weights. Each weight scales one factor's influence relative to the others; `0` turns the
/// factor off. The custom parameters' weights count next to these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapPolicy {
    pub need_weight: Weight,
    pub age_weight: Weight,
    pub deadline_weight: Weight,
    /// Shared by effort and impact, which count together as one value-per-effort factor.
    pub value_weight: Weight,
}

impl Default for HeapPolicy {
    fn default() -> Self {
        Self {
            need_weight: Weight::DEFAULT,
            age_weight: Weight::DEFAULT,
            deadline_weight: Weight::DEFAULT,
            value_weight: Weight::DEFAULT,
        }
    }
}

/// A partial edit of the heap's weights. `None` leaves the weight unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeapPolicyPatch {
    pub need: Option<Weight>,
    pub age: Option<Weight>,
    pub deadline: Option<Weight>,
    pub value: Option<Weight>,
}

impl HeapPolicyPatch {
    /// `policy` with the weights this patch sets.
    pub fn apply(&self, policy: &HeapPolicy) -> HeapPolicy {
        HeapPolicy {
            need_weight: self.need.unwrap_or(policy.need_weight),
            age_weight: self.age.unwrap_or(policy.age_weight),
            deadline_weight: self.deadline.unwrap_or(policy.deadline_weight),
            value_weight: self.value.unwrap_or(policy.value_weight),
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn weight(raw: i64) -> Weight {
        Weight::parse(raw).expect("valid")
    }

    #[gtest]
    fn a_patch_changes_only_the_weights_it_sets() {
        let patch = HeapPolicyPatch {
            need: Some(weight(30)),
            value: Some(weight(0)),
            ..HeapPolicyPatch::default()
        };

        let patched = patch.apply(&HeapPolicy::default());

        expect_that!(
            patched,
            eq(HeapPolicy {
                need_weight: weight(30),
                value_weight: weight(0),
                ..HeapPolicy::default()
            })
        );
        expect_that!(HeapPolicyPatch::default().apply(&patched), eq(patched));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heap {
    pub id: HeapId,
    pub team_id: TeamId,
    /// Unique within the team.
    pub slug: Handle,
    pub name: Name,
    pub policy: HeapPolicy,
    /// What the project is and where its work lives, as written; `None` when nobody has written one.
    pub context: Option<Context>,
    /// `None` while the heap is active.
    pub archived_at: Option<Timestamp>,
    pub version: Version,
}

impl Heap {
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}
