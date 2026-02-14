use katla_ecs::Component;
use katla_math::Color;
use katla_vulkan::{MaterialHandle, MeshHandle, SkeletonHandle};

#[derive(Component)]
pub struct DrawableComponent {
    /// Mesh handle for the new rendering system
    pub mesh_handle: Option<MeshHandle>,
    /// Material handle for the new rendering system
    pub material_handle: Option<MaterialHandle>,
    /// Optional material color (multiplied with texture in shader)
    pub color: Option<Color>,
    /// Optional skeleton handle for GPU skeletal animation
    pub skeleton_handle: Option<SkeletonHandle>,
}

impl DrawableComponent {
    /// Create with asset handles for the new rendering system
    pub fn with_handles(mesh_handle: MeshHandle, material_handle: MaterialHandle) -> Self {
        DrawableComponent {
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
            color: None,
            skeleton_handle: None,
        }
    }

    /// Create with asset handles and color
    pub fn with_handles_and_color(
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
    ) -> Self {
        DrawableComponent {
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
            color: Some(color),
            skeleton_handle: None,
        }
    }

    /// Set skeleton handle for GPU skeletal animation
    pub fn with_skeleton(mut self, skeleton_handle: SkeletonHandle) -> Self {
        self.skeleton_handle = Some(skeleton_handle);
        self
    }
}
