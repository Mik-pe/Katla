//! GPU resource cleanup system driven by ECS events.
//!
//! Processes entity destruction and component removal events to automatically
//! release GPU resources (meshes, materials, textures, skeletons) when they
//! are no longer referenced.

use crate::components::DrawableComponent;
use crate::gpu_resource_tracker::GpuResourceTracker;

/// Process ECS entity destruction events and release GPU resources.
///
/// Called each frame after `world.update()` while events are still accessible.
/// Iterates over `EntityEvent::Destroyed` events and releases any GPU resources
/// held by the destroyed entity's `DrawableComponent`.
///
/// Shared resources (mesh/material used by multiple entities) are only destroyed
/// when the last entity referencing them is destroyed, thanks to the reference
/// counting in `GpuResourceTracker`.
pub fn process_gpu_cleanup_events(
    world: &katla_ecs::World,
    tracker: &mut GpuResourceTracker,
    renderer: &mut katla_gfx::renderer::VulkanRenderer,
) {
    let destroyed_entities: Vec<_> = world
        .entity_events()
        .iter()
        .filter_map(|event| {
            if let katla_ecs::events::EntityEvent::Destroyed(id) = event {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    for entity_id in destroyed_entities {
        release_entity_gpu_resources(world, tracker, renderer, entity_id);
    }
}

/// Release GPU resources held by a single entity's DrawableComponent.
///
/// Uses the GPU resource tracker for reference-counted cleanup. The renderer's
/// destroy methods are called only for resources whose ref count drops to zero.
pub fn release_entity_gpu_resources(
    world: &katla_ecs::World,
    tracker: &mut GpuResourceTracker,
    renderer: &mut katla_gfx::renderer::VulkanRenderer,
    entity_id: katla_ecs::EntityId,
) {
    if let Some(drawable) = world.get_component::<DrawableComponent>(entity_id) {
        let to_destroy = tracker.release_drawable(
            drawable.mesh_handle,
            drawable.material_handle,
            drawable.skeleton_handle,
        );

        for handle in to_destroy.meshes {
            renderer.destroy_mesh(handle);
        }
        for handle in to_destroy.materials {
            renderer.destroy_material(handle);
        }
        for handle in to_destroy.skeletons {
            renderer.destroy_skeleton(handle);
        }
    }
}
