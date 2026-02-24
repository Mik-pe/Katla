//! System for uploading skeleton transforms to GPU each frame.
//!
//! This bridges the CPU-side SkeletalAnimationSystem with the GPU
//! by converting ECS Skeleton components to SkeletonBuffer uploads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use katla_ecs::{EntityId, System, World};
use katla_math::Mat4;
use katla_vulkan::{SkeletonBuffer, SkeletonDescriptorSet, VulkanContext};

use crate::animation::Skeleton;

/// Manages GPU skeleton buffers and descriptor sets for animated meshes.
///
/// Each animated mesh needs:
/// 1. A SkeletonBuffer to store joint matrices
/// 2. A SkeletonDescriptorSet to bind the buffer to Set 2
///
/// This system creates these resources on-demand and updates them each frame.
pub struct SkeletonUploadSystem {
    /// Map from entity ID to its skeleton GPU resources
    skeleton_resources: HashMap<EntityId, SkeletonResource>,
    /// Vulkan context for creating new buffers
    context: Option<Rc<VulkanContext>>,
}

/// GPU resources for a single animated mesh's skeleton
struct SkeletonResource {
    buffer: Rc<RefCell<SkeletonBuffer>>,
    descriptor_set: Rc<RefCell<Option<SkeletonDescriptorSet>>>,
    joint_count: usize,
}

impl SkeletonUploadSystem {
    pub fn new() -> Self {
        Self {
            skeleton_resources: HashMap::new(),
            context: None,
        }
    }

    /// Set the Vulkan context (called once during initialization)
    pub fn set_context(&mut self, context: Rc<VulkanContext>) {
        self.context = Some(context);
    }

    /// Register an animated mesh entity with its joint count.
    /// Creates GPU resources for the skeleton.
    pub fn register_skeleton(&mut self, entity: EntityId, joint_count: usize) {
        let context = match &self.context {
            Some(ctx) => ctx.clone(),
            None => {
                log::warn!("SkeletonUploadSystem has no Vulkan context");
                return;
            }
        };

        let buffer = Rc::new(RefCell::new(SkeletonBuffer::new(context, joint_count)));

        self.skeleton_resources.insert(
            entity,
            SkeletonResource {
                buffer,
                descriptor_set: Rc::new(RefCell::new(None)),
                joint_count,
            },
        );

        log::info!(
            "Registered skeleton for entity {:?} with {} joints",
            entity,
            joint_count
        );
    }

    /// Get the skeleton buffer for an entity (for descriptor set creation)
    pub fn get_skeleton_buffer(&self, entity: EntityId) -> Option<Rc<RefCell<SkeletonBuffer>>> {
        self.skeleton_resources
            .get(&entity)
            .map(|r| r.buffer.clone())
    }

    /// Get the skeleton descriptor set for an entity
    pub fn get_descriptor_set(
        &self,
        entity: EntityId,
    ) -> Option<Rc<RefCell<Option<SkeletonDescriptorSet>>>> {
        self.skeleton_resources
            .get(&entity)
            .map(|r| r.descriptor_set.clone())
    }

    /// Convert Mat4 to GPU-friendly [[f32; 4]; 4] format
    /// Both katla_math and WGSL use column-major, so direct copy
    fn mat4_to_array(matrix: &Mat4) -> [[f32; 4]; 4] {
        let data: [[f32; 4]; 4] = matrix.clone().into();
        // Direct copy - both are column-major
        data
    }
}

impl System for SkeletonUploadSystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        // For each entity with a Skeleton component, upload joint transforms
        for (entity, skeleton) in world.query::<&Skeleton>() {
            // Get or create GPU resources
            if !self.skeleton_resources.contains_key(&entity) {
                self.register_skeleton(entity, skeleton.joint_transforms.len());
            }

            // Get the buffer
            let buffer = match self.skeleton_resources.get(&entity) {
                Some(r) => r.buffer.clone(),
                None => continue,
            };

            // Convert Mat4 joint transforms to GPU format
            let joint_matrices: Vec<[[f32; 4]; 4]> = skeleton
                .joint_transforms
                .iter()
                .map(Self::mat4_to_array)
                .collect();

            // Upload to GPU
            buffer.borrow_mut().upload(&joint_matrices);
        }
    }

    fn name(&self) -> &str {
        "SkeletonUploadSystem"
    }
}

impl Default for SkeletonUploadSystem {
    fn default() -> Self {
        Self::new()
    }
}
