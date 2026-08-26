//! Deterministic transient allocation planning.
//!
//! The planner consumes post-culling resource lifetimes and assigns compatible,
//! non-overlapping transient images to reusable physical allocation slots. It is
//! backend-neutral: Vulkan and Metal can lower the same plan to native aliasing,
//! heaps, or conservative standalone allocations.

use std::collections::{BTreeMap, BTreeSet};

use super::compiler::ResourceLifetime;
use super::handles::ResourceId;
use super::resource::{GraphResourceDesc, GraphResourceType};
use crate::texture::ImageFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransientAllocationKind {
    ColorAttachment,
    DepthAttachment { sampled: bool },
    SampledImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransientCompatibilityKey {
    kind: TransientAllocationKind,
    format: ImageFormat,
    width: u32,
    height: u32,
    tracks_swapchain_size: bool,
}

impl From<&GraphResourceDesc> for TransientCompatibilityKey {
    fn from(resource: &GraphResourceDesc) -> Self {
        let kind = match &resource.resource_type {
            GraphResourceType::ColorAttachment { .. } => TransientAllocationKind::ColorAttachment,
            GraphResourceType::DepthAttachment { sampled, .. } => {
                TransientAllocationKind::DepthAttachment { sampled: *sampled }
            }
            GraphResourceType::SampledImage => TransientAllocationKind::SampledImage,
        };

        Self {
            kind,
            format: resource.format,
            width: resource.width,
            height: resource.height,
            tracks_swapchain_size: resource.tracks_swapchain_size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhysicalAllocationSlot {
    id: u32,
    compatibility: TransientCompatibilityKey,
    last_execution_position: usize,
    pinned: bool,
    bytes: u64,
}

/// Stable assignment of logical transient resources to physical allocation slots.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TransientAllocationPlan {
    assignments: BTreeMap<ResourceId, u32>,
    slots: Vec<PhysicalAllocationSlot>,
    logical_bytes: u64,
}

impl TransientAllocationPlan {
    pub(crate) fn build(
        resources: &[GraphResourceDesc],
        transient_resources: &[GraphResourceDesc],
        exported_resources: &BTreeSet<ResourceId>,
        lifetimes: &BTreeMap<ResourceId, ResourceLifetime>,
    ) -> Self {
        let resource_ids = resources
            .iter()
            .enumerate()
            .map(|(index, resource)| (resource.name.as_str(), ResourceId(index as u32)))
            .collect::<BTreeMap<_, _>>();

        let mut candidates = transient_resources
            .iter()
            .filter_map(|resource| {
                let resource_id = *resource_ids.get(resource.name.as_str())?;
                let lifetime = *lifetimes.get(&resource_id)?;
                Some((resource_id, resource, lifetime))
            })
            .collect::<Vec<_>>();
        candidates
            .sort_by_key(|(resource, _, lifetime)| (lifetime.first_execution_position, resource.0));

        let mut plan = Self::default();
        for (resource_id, resource, lifetime) in candidates {
            let bytes = u64::from(resource.width)
                .saturating_mul(u64::from(resource.height))
                .saturating_mul(u64::from(resource.format.bytes_per_pixel()));
            plan.logical_bytes = plan.logical_bytes.saturating_add(bytes);

            let compatibility = TransientCompatibilityKey::from(resource);
            let exported = exported_resources.contains(&resource_id);
            let reusable_slot = (!exported).then(|| {
                plan.slots.iter().position(|slot| {
                    !slot.pinned
                        && slot.compatibility == compatibility
                        && slot.last_execution_position < lifetime.first_execution_position
                })
            });

            let slot_index = reusable_slot.flatten().unwrap_or_else(|| {
                let id = u32::try_from(plan.slots.len())
                    .expect("transient allocation count exceeds u32::MAX");
                plan.slots.push(PhysicalAllocationSlot {
                    id,
                    compatibility,
                    last_execution_position: lifetime.last_execution_position,
                    pinned: exported,
                    bytes,
                });
                id as usize
            });

            let slot = &mut plan.slots[slot_index];
            slot.last_execution_position = lifetime.last_execution_position;
            slot.pinned |= exported;
            debug_assert_eq!(slot.bytes, bytes);
            plan.assignments.insert(resource_id, slot.id);
        }

        plan
    }

    pub(crate) fn physical_allocation_id(&self, resource: ResourceId) -> Option<u32> {
        self.assignments.get(&resource).copied()
    }

    pub(crate) fn physical_allocation_count(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }

    pub(crate) fn physical_bytes(&self) -> u64 {
        self.slots
            .iter()
            .fold(0, |total, slot| total.saturating_add(slot.bytes))
    }

    pub(crate) fn saved_bytes(&self) -> u64 {
        self.logical_bytes().saturating_sub(self.physical_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(name: &str) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: GraphResourceType::ColorAttachment { clear_value: None },
            format: ImageFormat::R8G8B8A8Unorm,
            width: 64,
            height: 64,
            tracks_swapchain_size: false,
        }
    }

    fn lifetime(first: usize, last: usize) -> ResourceLifetime {
        ResourceLifetime {
            first_execution_position: first,
            first_pass: first,
            last_execution_position: last,
            last_pass: last,
        }
    }

    #[test]
    fn reuses_compatible_non_overlapping_intervals() {
        let resources = vec![resource("a"), resource("b"), resource("overlap")];
        let lifetimes = BTreeMap::from([
            (ResourceId(0), lifetime(0, 1)),
            (ResourceId(1), lifetime(2, 3)),
            (ResourceId(2), lifetime(1, 2)),
        ]);

        let plan =
            TransientAllocationPlan::build(&resources, &resources, &BTreeSet::new(), &lifetimes);

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(2)), Some(1));
        assert_eq!(plan.physical_allocation_count(), 2);
        assert_eq!(plan.logical_bytes(), 3 * 64 * 64 * 4);
        assert_eq!(plan.physical_bytes(), 2 * 64 * 64 * 4);
    }

    #[test]
    fn exported_resources_receive_pinned_unique_slots() {
        let resources = vec![resource("exported"), resource("later")];
        let lifetimes = BTreeMap::from([
            (ResourceId(0), lifetime(0, 0)),
            (ResourceId(1), lifetime(1, 1)),
        ]);

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::from([ResourceId(0)]),
            &lifetimes,
        );

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), Some(1));
        assert_eq!(plan.saved_bytes(), 0);
    }

    #[test]
    fn culled_or_unused_resources_receive_no_allocation() {
        let resources = vec![resource("live"), resource("dead")];
        let lifetimes = BTreeMap::from([(ResourceId(0), lifetime(0, 0))]);

        let plan =
            TransientAllocationPlan::build(&resources, &resources, &BTreeSet::new(), &lifetimes);

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), None);
    }
}
