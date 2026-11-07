//! Heaps and their ranking policy.

use crate::handle::Handle;
use crate::text::Name;
use crate::{HeapId, TeamId, Timestamp, Version};

/// The heap's ranking weights. Each weight scales one factor's influence relative to the others; `0` turns the
/// factor off. Custom parameter weights will join these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapPolicy {
    pub age_weight: u16,
    pub deadline_weight: u16,
    /// Shared by effort and impact, which count together as one value-per-effort factor.
    pub value_weight: u16,
}

impl Default for HeapPolicy {
    fn default() -> Self {
        Self {
            age_weight: 10,
            deadline_weight: 10,
            value_weight: 10,
        }
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
    /// `None` while the heap is active.
    pub archived_at: Option<Timestamp>,
    pub version: Version,
}

impl Heap {
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}
