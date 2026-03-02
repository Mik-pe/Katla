use katla_ecs::Component;
use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle};
use katla_math::Color;

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
    /// PBR metallic factor (0.0 = dielectric, 1.0 = metal)
    pub metallic: f32,
    /// PBR roughness factor (0.0 = smooth, 1.0 = rough)
    pub roughness: f32,
    /// Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    pub ao: f32,
}

impl DrawableComponent {
    /// Create with asset handles for the new rendering system
    pub fn with_handles(mesh_handle: MeshHandle, material_handle: MaterialHandle) -> Self {
        DrawableComponent {
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
            color: None,
            skeleton_handle: None,
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
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
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }
    }

    /// Create with asset handles and PBR material values
    pub fn with_handles_and_material(
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Option<Color>,
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) -> Self {
        DrawableComponent {
            mesh_handle: Some(mesh_handle),
            material_handle: Some(material_handle),
            color,
            skeleton_handle: None,
            metallic,
            roughness,
            ao,
        }
    }

    /// Set skeleton handle for GPU skeletal animation
    pub fn with_skeleton(mut self, skeleton_handle: SkeletonHandle) -> Self {
        self.skeleton_handle = Some(skeleton_handle);
        self
    }
}
