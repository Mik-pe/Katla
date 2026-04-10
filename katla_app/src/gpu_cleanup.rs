//! GPU resource cleanup system driven by ECS events.
//!
//! Processes entity destruction and component removal events to automatically
//! release GPU resources (meshes, materials, textures, skeletons) when they
//! are no longer referenced.

use std::any::TypeId;
use std::collections::HashSet;

use crate::components::DrawableComponent;
use crate::gpu_resource_tracker::GpuResourceTracker;

/// Process ECS entity destruction and component removal events to release GPU resources.
///
/// Called each frame after `world.update()` while events are still accessible.
///
/// - `EntityEvent::Destroyed`: Releases GPU resources for the destroyed entity's
///   `DrawableComponent` (components already removed by `destroy_entity`).
/// - `ComponentEvent::Removed` for `DrawableComponent`: Handles standalone component
///   removal (entity still alive). Skips entities that were destroyed this frame
///   since their resources are already cleaned up via the destruction path.
pub fn process_gpu_cleanup_events(
    world: &katla_ecs::World,
    tracker: &mut GpuResourceTracker,
    renderer: &mut katla_gfx::renderer::VulkanRenderer,
) {
    let destroyed_entities: HashSet<_> = world
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

    let drawable_type_id = TypeId::of::<DrawableComponent>();

    let component_removed_entities: Vec<_> = world
        .component_events()
        .iter()
        .filter_map(|event| {
            if let katla_ecs::events::ComponentEvent::Removed(id, type_id) = event {
                if *type_id == drawable_type_id {
                    Some(*id)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .filter(|id| !destroyed_entities.contains(id))
        .collect();

    for entity_id in destroyed_entities {
        release_entity_gpu_resources(world, tracker, renderer, entity_id);
    }

    for entity_id in component_removed_entities {
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
