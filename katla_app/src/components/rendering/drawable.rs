use katla_ecs::Component;
use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
use katla_math::{AABB, Color};

#[derive(Component)]
pub struct DrawableComponent {
    /// Mesh handle for the new rendering system
    pub mesh_handle: MeshHandle,
    /// Material handle for the new rendering system
    pub material_handle: MaterialHandle,
    /// Optional material color (multiplied with texture in shader)
    pub color: Option<Color>,
    /// Skeleton handle for GPU skeletal animation (use SkeletonHandle::NONE if not animated)
    pub skeleton_handle: SkeletonHandle,
    /// PBR metallic factor (0.0 = dielectric, 1.0 = metal)
    pub metallic: f32,
    /// PBR roughness factor (0.0 = smooth, 1.0 = rough)
    pub roughness: f32,
    /// Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    pub ao: f32,
    /// Emission texture for self-illumination, referenced by handle
    /// (the backend resolves it to its binding table each frame)
    pub emission: TextureHandle,
    /// Surface multipliers and coverage policy owned by this drawable.
    pub surface: crate::rendering::MaterialSurface,
    pub(crate) uv_sets: [bool; 2],
    pub(crate) texture_bindings: crate::material_images::TextureBindings,
    pub(crate) texture_roles: [bool; 5],
    /// Per-role sampling properties, independent of image ownership.
    pub sampling: crate::rendering::MaterialSampling,
    /// Original generated tangent coordinates; authored tangent bases remain unchanged.
    pub(crate) tangent_uv: Option<crate::rendering::UvTransform>,
    /// Local-space bounding box for frustum culling
    pub bounds: Option<AABB>,
}

impl DrawableComponent {
    pub(crate) fn validate_sampling(
        &self,
        sampling: crate::rendering::MaterialSampling,
    ) -> Result<(), String> {
        sampling.validate().map_err(str::to_owned)?;
        for ((role, has_texture), value) in katla_agent::material_sampling::TextureRole::ALL
            .into_iter()
            .zip(self.texture_roles)
            .zip(sampling.roles())
        {
            let has_texture = self.texture_bindings.0[role.index()]
                .as_ref()
                .map_or(has_texture, |binding| binding.has_image());
            if has_texture && !self.uv_sets[value.uv.tex_coord as usize] {
                return Err(format!(
                    "{} texture requires missing TEXCOORD_{}",
                    role.name(),
                    value.uv.tex_coord
                ));
            }
        }
        Ok(())
    }

    /// Create with asset handles for the new rendering system
    pub fn with_handles(mesh_handle: MeshHandle, material_handle: MaterialHandle) -> Self {
        DrawableComponent {
            mesh_handle,
            material_handle,
            color: None,
            skeleton_handle: SkeletonHandle::NONE,
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
            emission: TextureHandle::NONE,
            surface: Default::default(),
            sampling: Default::default(),
            uv_sets: [true; 2],
            texture_roles: [false; 5],
            texture_bindings: Default::default(),
            tangent_uv: None,
            bounds: None,
        }
    }

    /// Create with asset handles and color
    pub fn with_handles_and_color(
        mesh_handle: MeshHandle,
        material_handle: MaterialHandle,
        color: Color,
    ) -> Self {
        DrawableComponent {
            mesh_handle,
            material_handle,
            color: Some(color),
            skeleton_handle: SkeletonHandle::NONE,
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
            emission: TextureHandle::NONE,
            surface: Default::default(),
            sampling: Default::default(),
            uv_sets: [true; 2],
            texture_roles: [false; 5],
            texture_bindings: Default::default(),
            tangent_uv: None,
            bounds: None,
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
            mesh_handle,
            material_handle,
            color,
            skeleton_handle: SkeletonHandle::NONE,
            metallic,
            roughness,
            ao,
            emission: TextureHandle::NONE,
            surface: Default::default(),
            sampling: Default::default(),
            uv_sets: [true; 2],
            texture_roles: [false; 5],
            texture_bindings: Default::default(),
            tangent_uv: None,
            bounds: None,
        }
    }

    /// Set skeleton handle for GPU skeletal animation
    pub fn with_skeleton(mut self, skeleton_handle: SkeletonHandle) -> Self {
        self.skeleton_handle = skeleton_handle;
        self
    }

    /// Set local-space bounding box for frustum culling
    pub fn with_bounds(mut self, bounds: AABB) -> Self {
        self.bounds = Some(bounds);
        self
    }
}
