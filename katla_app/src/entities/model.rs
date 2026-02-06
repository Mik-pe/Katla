use katla_ecs::{EntityId, World};
use katla_vulkan::{MaterialHandle, MeshHandle, VulkanRenderer};

use crate::{
    application::Model,
    components::{DrawableComponent, NameComponent, TransformComponent},
};
use katla_math::Transform;

/// Create a model entity from a Model.
/// The Model is consumed during registration.
///
/// # Arguments
/// * `world` - The ECS world to create the entity in
/// * `model` - The model data (meshes and materials)
/// * `renderer` - Optional Vulkan renderer for asset registration
/// * `transform` - Initial transform for the entity
///
/// # Returns
/// The created entity ID
pub fn create_model_entity(
    world: &mut World,
    mut model: Model,
    renderer: Option<&mut VulkanRenderer>,
    transform: Transform,
) -> EntityId {
    let entity = world.create_entity();

    // Register assets with renderer if available
    let (mesh_handle, material_handle) = if let Some(r) = renderer {
        // Register mesh - take buffers from the first mesh
        // Note: For now we only support single-mesh models
        let mesh_h = if let Some(first_mesh) = model.meshes.first_mut() {
            let vertex_buffer = first_mesh.vertex_buffer.take();
            let index_buffer = first_mesh.index_buffer.take();
            r.register_mesh(vertex_buffer, index_buffer)
        } else {
            MeshHandle(0) // Dummy handle if no meshes
        };

        // Register material
        let mat_h = r.create_material(
            model.material.material_pipeline.clone(),
            model.material.texture.clone(),
            model.material.vertex_binding.clone(),
        );

        // Store handles in the model
        model.mesh_handle = Some(mesh_h);
        model.material_handle = Some(mat_h);

        (Some(mesh_h), Some(mat_h))
    } else {
        // Use dummy handles
        (Some(MeshHandle(0)), Some(MaterialHandle(0)))
    };

    world.add_component(entity, TransformComponent::new(transform));
    world.add_component(
        entity,
        DrawableComponent::with_handles(
            mesh_handle.unwrap(),
            material_handle.unwrap(),
        ),
    );
    world.add_component(entity, NameComponent::new("Model"));

    entity
}
