use katla_ecs::Component;
use katla_vulkan::{MaterialHandle, MeshHandle};

#[derive(Component)]
pub struct DrawableComponent {
    /// Mesh handle for the new rendering system
    pub mesh_handle: Option<MeshHandle>,
    /// Material handle for the new rendering system
    pub material_handle: Option<MaterialHandle>,
}

impl DrawableComponent {
    /// Create with asset handles for the new rendering system
    pub fn with_handles(
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
    ) -> Self {
        DrawableComponent {
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
        }
    }
}
