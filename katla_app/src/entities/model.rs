use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::Transform;
use katla_vulkan::{MaterialHandle, MeshHandle, RenderPass, VulkanContext, VulkanRenderer};

use crate::{
    components::{DrawableComponent, NameComponent, TransformComponent},
    rendering::{Material, Mesh},
    util::GLTFModel,
};

pub struct Model {
    pub entity: EntityId,
    /// Handle after registration (MeshHandle(0) if no renderer provided)
    pub mesh_handle: MeshHandle,
    /// Handle after registration (MaterialHandle(0) if no renderer provided)
    pub material_handle: MaterialHandle,
}

impl Model {
    pub fn new(
        world: &mut World,
        mut meshes: Vec<Mesh>,
        material: Material,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
    ) -> Self {
        let entity = world.create_entity();

        // Register assets with renderer if available
        let (mesh_handle, material_handle) = if let Some(r) = renderer {
            // Register mesh - take buffers from the first mesh
            // Note: For now we only support single-mesh models
            let mesh_h = if let Some(first_mesh) = meshes.first_mut() {
                let vertex_buffer = first_mesh.vertex_buffer.take();
                let index_buffer = first_mesh.index_buffer.take();
                r.register_mesh(vertex_buffer, index_buffer)
            } else {
                MeshHandle(0) // Dummy handle if no meshes
            };

            // Register material
            let mat_h = r.create_material(
                material.material_pipeline.clone(),
                material.texture.clone(),
                material.vertex_binding.clone(),
            );

            (mesh_h, mat_h)
        } else {
            // Use dummy handles when no renderer provided
            (MeshHandle(0), MaterialHandle(0))
        };

        world.add_component(entity, TransformComponent::new(transform));
        world.add_component(
            entity,
            DrawableComponent::with_handles(mesh_handle, material_handle),
        );
        world.add_component(entity, NameComponent::new("Model"));

        Self {
            entity,
            mesh_handle,
            material_handle,
        }
    }

    pub fn new_from_gltf(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        renderer: Option<&mut VulkanRenderer>,
        render_pass: &RenderPass,
        transform: Transform,
    ) -> Self {
        let material = Material::new(model.clone(), context.clone(), render_pass);
        let mesh = Mesh::new_from_model(model, context.clone());

        Self::new(world, vec![mesh], material, renderer, transform)
    }
}
