//! Deterministic transient allocation planning.
//!
//! The planner consumes post-culling resource lifetimes and assigns compatible,
//! non-overlapping transient images to reusable physical allocation slots. It is
//! backend-neutral: Vulkan and Metal can lower the same plan to native aliasing,
//! heaps, or conservative standalone allocations.

use std::collections::{BTreeMap, BTreeSet};

use super::access::{ImageAccess, ImageUsage};
use super::compiler::ResourceLifetime;
use super::handles::ResourceId;
use super::resource::{GraphResourceDesc, GraphResourceType};
use crate::texture::ImageFormat;

/// Which compiled fact prevents a slot's storage from being tile-resident.
///
/// Tile-memory storage is only sound when every access to a slot member stays
/// inside the render pass that produces and consumes it: a tile-resident
/// attachment cannot be sampled, used as a storage image, transferred, or
/// presented, and the pass must cover the whole resource. The verdict comes
/// from the same typed accesses that compile the dependency DAG and the
/// synchronization plan, so it cannot disagree with scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TileMemoryEligibility {
    /// Every access is a whole-resource attachment access, and a pass writes
    /// the resource.
    Eligible,
    /// The resource is exported: something outside the graph (a readback, an
    /// overlay, the swapchain) observes its contents.
    Exported,
    /// A sampled, storage, transfer, or present access appears in the graph.
    AccessedOutsideAttachment,
    /// A pass covers only part of the resource, so part of it must outlive
    /// the pass.
    PartialSubresourceCoverage,
    /// No live pass writes the resource, so its contents are undefined.
    NeverWritten,
}

impl TileMemoryEligibility {
    /// Whether a backend may keep the slot's storage in tile memory.
    pub(crate) fn is_eligible(self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// Why the slot is or is not tile-memory eligible, for diagnostics.
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Eligible => "every access is a whole-resource attachment access",
            Self::Exported => "resource is exported outside the graph",
            Self::AccessedOutsideAttachment => {
                "resource is sampled, stored, transferred, or presented outside an attachment"
            }
            Self::PartialSubresourceCoverage => "an access covers only part of the resource",
            Self::NeverWritten => "no live pass writes the resource",
        }
    }

    /// Combine two members' verdicts for the slot that holds both.
    ///
    /// A slot is tile-resident only if every member is. The reported reason is
    /// stable rather than declaration-order dependent, so diagnostics stay
    /// deterministic.
    fn combine(self, other: Self) -> Self {
        fn rank(verdict: TileMemoryEligibility) -> u8 {
            match verdict {
                TileMemoryEligibility::Exported => 0,
                TileMemoryEligibility::AccessedOutsideAttachment => 1,
                TileMemoryEligibility::PartialSubresourceCoverage => 2,
                TileMemoryEligibility::NeverWritten => 3,
                TileMemoryEligibility::Eligible => 4,
            }
        }

        if rank(self) <= rank(other) {
            self
        } else {
            other
        }
    }
}

/// Classify one slot member from its compiled typed accesses.
fn member_tile_eligibility(
    resource: ResourceId,
    exported: bool,
    accesses: &[ImageAccess],
) -> TileMemoryEligibility {
    if exported {
        return TileMemoryEligibility::Exported;
    }

    let mut written = false;

    for access in accesses.iter().filter(|access| access.resource == resource) {
        match access.usage {
            ImageUsage::ColorAttachment | ImageUsage::DepthStencilAttachment => {}
            ImageUsage::Sampled
            | ImageUsage::Storage
            | ImageUsage::TransferSource
            | ImageUsage::TransferDestination
            | ImageUsage::Present => return TileMemoryEligibility::AccessedOutsideAttachment,
        }

        if !access.range.covers_whole_resource() {
            return TileMemoryEligibility::PartialSubresourceCoverage;
        }

        written |= access.mode.writes();
    }

    if written {
        TileMemoryEligibility::Eligible
    } else {
        TileMemoryEligibility::NeverWritten
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransientAllocationKind {
    ColorAttachment,
    DepthAttachment { sampled: bool },
    SampledImage,
}

impl TransientAllocationKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ColorAttachment => "color_attachment",
            Self::DepthAttachment { sampled: true } => "sampled_depth_attachment",
            Self::DepthAttachment { sampled: false } => "depth_attachment",
            Self::SampledImage => "sampled_image",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TransientCompatibilityKey {
    pub(crate) kind: TransientAllocationKind,
    pub(crate) format: ImageFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tracks_swapchain_size: bool,
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
pub(crate) struct PhysicalAllocationSlot {
    pub(crate) id: u32,
    pub(crate) compatibility: TransientCompatibilityKey,
    /// Members in first-use order: each member's alias predecessor precedes
    /// it and its alias successor follows it in this list.
    pub(crate) members: Vec<ResourceId>,
    pub(crate) first_execution_position: usize,
    pub(crate) last_execution_position: usize,
    pub(crate) pinned: bool,
    pub(crate) bytes: u64,
    /// Whether every member's compiled accesses allow tile-resident storage.
    /// A slot is eligible only if all of its members are.
    pub(crate) tile_memory: TileMemoryEligibility,
}

/// Stable assignment of logical transient resources to physical allocation slots.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TransientAllocationPlan {
    assignments: BTreeMap<ResourceId, u32>,
    slots: Vec<PhysicalAllocationSlot>,
    member_bytes: BTreeMap<ResourceId, u64>,
    logical_bytes: u64,
}

impl TransientAllocationPlan {
    pub(crate) fn build(
        resources: &[GraphResourceDesc],
        transient_resources: &[GraphResourceDesc],
        exported_resources: &BTreeSet<ResourceId>,
        lifetimes: &BTreeMap<ResourceId, ResourceLifetime>,
        image_accesses: &[ImageAccess],
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
            let tile_memory = member_tile_eligibility(resource_id, exported, image_accesses);
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
                    members: Vec::new(),
                    first_execution_position: lifetime.first_execution_position,
                    last_execution_position: lifetime.last_execution_position,
                    pinned: exported,
                    bytes,
                    tile_memory,
                });
                id as usize
            });

            let slot = &mut plan.slots[slot_index];
            slot.members.push(resource_id);
            slot.last_execution_position = lifetime.last_execution_position;
            slot.tile_memory = slot.tile_memory.combine(tile_memory);
            slot.pinned |= exported;
            debug_assert_eq!(slot.bytes, bytes);
            plan.member_bytes.insert(resource_id, bytes);
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

    pub(crate) fn slots(&self) -> &[PhysicalAllocationSlot] {
        &self.slots
    }

    pub(crate) fn slot(&self, id: u32) -> Option<&PhysicalAllocationSlot> {
        self.slots.get(id as usize).filter(|slot| slot.id == id)
    }

    /// Standalone allocation bytes of a slot's members added up.
    pub(crate) fn slot_logical_bytes(&self, id: u32) -> Option<u64> {
        self.slot(id).map(|slot| {
            slot.members
                .iter()
                .filter_map(|member| self.member_bytes.get(member))
                .fold(0u64, |total, &bytes| total.saturating_add(bytes))
        })
    }

    /// Estimated memory the slot's aliasing saves over standalone allocations.
    pub(crate) fn slot_saved_bytes(&self, id: u32) -> Option<u64> {
        let slot = self.slot(id)?;
        Some(self.slot_logical_bytes(id)?.saturating_sub(slot.bytes))
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

    /// Physical bytes in slots eligible for tile-resident storage.
    pub(crate) fn tile_memory_eligible_bytes(&self) -> u64 {
        self.slots
            .iter()
            .filter(|slot| slot.tile_memory.is_eligible())
            .fold(0, |total, slot| total.saturating_add(slot.bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::super::access::{
        ImageAccessMode, ImageAspects, ImagePipelineStage, ImageSubresourceRange,
    };
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

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &[],
        );

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(2)), Some(1));
        assert_eq!(plan.physical_allocation_count(), 2);
        assert_eq!(plan.logical_bytes(), 3 * 64 * 64 * 4);
        assert_eq!(plan.physical_bytes(), 2 * 64 * 64 * 4);

        let shared = plan.slot(0).unwrap();
        assert_eq!(shared.members, vec![ResourceId(0), ResourceId(1)]);
        assert_eq!(shared.first_execution_position, 0);
        assert_eq!(shared.last_execution_position, 3);
        assert_eq!(plan.slot_logical_bytes(0), Some(2 * 64 * 64 * 4));
        assert_eq!(plan.slot_saved_bytes(0), Some(64 * 64 * 4));
        assert_eq!(plan.slot_saved_bytes(1), Some(0));
        assert_eq!(plan.slot_saved_bytes(9), None);
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
            &[],
        );

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), Some(1));
        assert_eq!(plan.saved_bytes(), 0);
    }

    #[test]
    fn culled_or_unused_resources_receive_no_allocation() {
        let resources = vec![resource("live"), resource("dead")];
        let lifetimes = BTreeMap::from([(ResourceId(0), lifetime(0, 0))]);

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &[],
        );

        assert_eq!(plan.physical_allocation_id(ResourceId(0)), Some(0));
        assert_eq!(plan.physical_allocation_id(ResourceId(1)), None);
    }

    fn attachment_write(resource: ResourceId) -> ImageAccess {
        ImageAccess::color_attachment_write(resource)
    }

    #[test]
    fn tile_memory_requires_whole_resource_attachment_accesses() {
        // Overlapping lifetimes keep each resource in its own slot, so the
        // verdicts below are per-resource.
        let resources = vec![resource("tile"), resource("sampled"), resource("partial")];
        let lifetimes = BTreeMap::from([
            (ResourceId(0), lifetime(0, 3)),
            (ResourceId(1), lifetime(0, 3)),
            (ResourceId(2), lifetime(0, 3)),
        ]);
        let accesses = vec![
            attachment_write(ResourceId(0)),
            ImageAccess::sampled_read(ResourceId(1)),
            attachment_write(ResourceId(2)).with_range(ImageSubresourceRange::new(
                ImageAspects::COLOR,
                0,
                1,
                0,
                1,
            )),
        ];

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &accesses,
        );

        assert_eq!(
            plan.slot(0).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::Eligible)
        );
        assert!(
            plan.slot(0)
                .map(|slot| slot.tile_memory)
                .unwrap()
                .is_eligible()
        );

        assert_eq!(
            plan.slot(1).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::AccessedOutsideAttachment)
        );
        assert_eq!(
            plan.slot(2).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::PartialSubresourceCoverage)
        );
        assert_eq!(plan.slot(9).map(|slot| slot.tile_memory), None);
    }

    #[test]
    fn tile_memory_needs_a_writing_pass() {
        let resources = vec![resource("read_only")];
        let lifetimes = BTreeMap::from([(ResourceId(0), lifetime(0, 0))]);
        let accesses = vec![
            ImageAccess::color_attachment_read_write(ResourceId(0))
                .with_range(ImageSubresourceRange::WHOLE_COLOR),
        ];

        // A read-only attachment never establishes contents, so it can never
        // be tile-resident.
        let read_only = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &[ImageAccess::new(
                ResourceId(0),
                ImageAccessMode::Read,
                ImageUsage::ColorAttachment,
                ImagePipelineStage::ColorAttachmentOutput,
                ImageSubresourceRange::WHOLE_COLOR,
            )],
        );
        assert_eq!(
            read_only.slot(0).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::NeverWritten)
        );

        let written = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &accesses,
        );
        assert_eq!(
            written.slot(0).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::Eligible)
        );
    }

    #[test]
    fn an_exported_resource_is_never_tile_resident() {
        let resources = vec![resource("exported")];
        let lifetimes = BTreeMap::from([(ResourceId(0), lifetime(0, 0))]);
        let accesses = vec![attachment_write(ResourceId(0))];

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::from([ResourceId(0)]),
            &lifetimes,
            &accesses,
        );

        // Everything outside the graph would observe the export, so its
        // storage must survive the pass.
        assert_eq!(
            plan.slot(0).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::Exported)
        );
    }

    #[test]
    fn a_shared_slot_is_eligible_only_if_every_member_is() {
        let resources = vec![resource("tile"), resource("sampled")];
        let lifetimes = BTreeMap::from([
            (ResourceId(0), lifetime(0, 0)),
            (ResourceId(1), lifetime(1, 1)),
        ]);
        let accesses = vec![
            attachment_write(ResourceId(0)),
            ImageAccess::sampled_read(ResourceId(1)),
        ];

        let plan = TransientAllocationPlan::build(
            &resources,
            &resources,
            &BTreeSet::new(),
            &lifetimes,
            &accesses,
        );

        // Both members are compatible and disjoint, so they alias one slot;
        // the sampled member makes the whole slot ineligible.
        assert_eq!(plan.physical_allocation_count(), 1);
        assert_eq!(plan.slot(0).unwrap().members.len(), 2);
        assert_eq!(
            plan.slot(0).map(|slot| slot.tile_memory),
            Some(TileMemoryEligibility::AccessedOutsideAttachment)
        );
    }
}
