use katla_ecs::{EntityId, World};
use katla_vulkan::{MaterialHandle, MeshHandle, VulkanRenderer};

use crate::{
    application::Model,
    components::{DrawableComponent, NameComponent, TransformComponent},
};

pub struct ModelEntity {
    _entity: EntityId,
}

impl ModelEntity {
    pub fn new(world: &mut World, model: Model) -> Self {
        Self::new_with_renderer(world, model, None)
    }

    pub fn new_with_renderer(
        world: &mut World,
        mut model: Model,
        renderer: Option<&mut VulkanRenderer>,
    ) -> Self {
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

        // Extract the transform data to create TransformComponent
        let transform_ref = &model.transform;
        // Create a new Transform with the same values (Transform has no Clone, but we can reconstruct)
        let position = transform_ref.position;
        let rotation = transform_ref.rotation;
        let scale = transform_ref.scale;
        let transform = katla_math::Transform {
            position,
            rotation,
            scale,
        };

        world.add_component(entity, TransformComponent::new(transform));
        // Keep the original model with its transform intact for rendering
        world.add_component(
            entity,
            DrawableComponent::with_handles(
                Box::new(model),
                mesh_handle.unwrap(),
                material_handle.unwrap(),
            ),
        );
        world.add_component(entity, NameComponent::new("Model"));
        Self { _entity: entity }
    }

    pub fn new_with_transform(
        world: &mut World,
        model: Model,
        transform: katla_math::Transform,
    ) -> Self {
        let entity = world.create_entity();

        world.add_component(
            entity,
            DrawableComponent {
                drawable: Box::new(model),
                mesh_handle: None,
                material_handle: None,
            },
        );
        world.add_component(entity, TransformComponent::new(transform));
        world.add_component(entity, NameComponent::new("Model"));
        Self { _entity: entity }
    }
}
