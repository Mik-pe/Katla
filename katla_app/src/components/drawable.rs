use katla_ecs::Component;

use crate::rendering;
use katla_vulkan::{MaterialHandle, MeshHandle};

#[derive(Component)]
pub struct DrawableComponent {
    /// The drawable object (old system - will be removed in future)
    pub drawable: Box<dyn rendering::Drawable>,
    /// Mesh handle for the new rendering system
    pub mesh_handle: Option<MeshHandle>,
    /// Material handle for the new rendering system
    pub material_handle: Option<MaterialHandle>,
}

impl DrawableComponent {
    pub fn new(drawable: Box<dyn rendering::Drawable>) -> Self {
        DrawableComponent {
            drawable,
            mesh_handle: None,
            material_handle: None,
        }
    }

    /// Create with asset handles for the new rendering system
    pub fn with_handles(
        drawable: Box<dyn rendering::Drawable>,
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
    ) -> Self {
        DrawableComponent {
            drawable,
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
        }
    }
}
