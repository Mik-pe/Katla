//! Frame context for submitting draws with automatic instance allocation.
//!
//! This module provides a high-level API for submitting draw calls. Object
//! storage slots are allocated by `katla_gfx`'s `DrawList` when a draw is
//! pushed, so callers never choose storage indices. The fluent builder
//! pattern makes it easy to configure draw calls.
//!
//! # Example
//!
//! ```ignore
//! use katla_app::rendering::FrameContext;
//! use katla_gfx::{MeshHandle, MaterialHandle};
//! use katla_app::rendering::FrameUniforms;
//!
//! // Create frame context at start of frame
//! let mut frame = FrameContext::new();
//!
//! // Set frame-level uniforms (camera, lighting)
//! let uniforms = FrameUniforms {
//!     view_matrix: view_array,
//!     proj_matrix: proj_array,
//!     inv_view_proj_matrix: inv_view_proj_array,
//!     camera_position: [x, y, z, 1.0],
//!     light_direction: [0.3, 1.0, 0.2, 0.0],
//!     light_color: [1.0, 0.98, 0.95, 0.0],
//!     light_intensity: [1.0, 0.0, 0.0, 0.0],
//! };
//! frame.set_frame_uniforms(uniforms);
//!
//! // Submit draws - instance allocation is automatic
//! frame.draw(cube_mesh, pbr_material)
//!     .with_transform(cube_transform)
//!     .with_color([1.0, 0.0, 0.0, 1.0])
//!     .with_pbr(0.0, 0.5, 1.0)
//!     .submit();
//!
//! // Get the draw list to pass to renderer
//! let submission = frame.take_submission();
//! ```

use katla_gfx::{
    MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle,
    renderer::{DrawCall, DrawList, InstanceData},
};

use super::{FrameUniforms, MaterialSampling, MaterialSurface, SurfaceParameters, UvTransform};

pub(crate) struct SurfaceRows<'a> {
    pub(crate) parameters: &'a [SurfaceParameters],
    pub(crate) samplers: &'a [[katla_gfx::SamplerDescriptor; 5]],
}

/// Geometry and surface values indexed by the draw list's assigned object slots.
pub struct FrameSubmission {
    /// Core geometry commands retain their slots across filtering and sorting.
    pub draw_list: DrawList,
    pub(crate) surfaces: Vec<SurfaceParameters>,
    pub(crate) samplers: Vec<[katla_gfx::SamplerDescriptor; 5]>,
}

/// Per-frame context for submitting draws with automatic instance allocation.
///
/// Object storage slots are assigned by the `DrawList` at push time;
/// `submit()` returns the base slot assigned to the draw. The draw list is
/// taken (and the context cleared) when `take_submission()` is called.
pub struct FrameContext {
    /// Accumulated draw calls for this frame
    draw_list: DrawList,
    /// Scene shader data for the submitted frame
    /// Always set via set_camera() or set_frame_uniforms() before rendering
    frame_uniforms: FrameUniforms,
    surfaces: Vec<SurfaceParameters>,
    samplers: Vec<[katla_gfx::SamplerDescriptor; 5]>,
}

impl Default for FrameContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameContext {
    /// Create a new empty frame context.
    ///
    /// Frame uniforms are initialized to defaults (identity matrices, origin camera).
    pub fn new() -> Self {
        Self {
            draw_list: DrawList::new(),
            frame_uniforms: FrameUniforms::default(),
            surfaces: vec![SurfaceParameters::default()],
            samplers: vec![MaterialSampling::default().samplers()],
        }
    }

    /// Set camera and lighting uniforms for this frame.
    ///
    /// This should be called once per frame before submitting any draws.
    /// Uses default lighting values (sunlight direction).
    ///
    /// # Arguments
    /// * `view_matrix` - View matrix (world to camera transform)
    /// * `proj_matrix` - Projection matrix (camera to clip space)
    /// * `camera_position` - Camera position in world space
    pub fn set_camera(
        &mut self,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
        camera_position: &[f32; 4],
    ) {
        self.frame_uniforms = FrameUniforms {
            view_matrix: *view_matrix,
            proj_matrix: *proj_matrix,
            inv_view_proj_matrix: [0.0f32; 16], // Will be computed by renderer if needed
            camera_position: *camera_position,
            light_direction: [0.3, 1.0, 0.2, 0.0], // Upward toward sun
            light_color: [1.0, 0.98, 0.95, 0.0],   // Slightly warm white
            light_intensity: [1.0, 0.0, 0.0, 0.0], // Base intensity for PBR
            tiles: [0, 0, 0, 0],
            tonemap: [1.0, 2.2, 0.0, 0.0],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        };
    }

    /// Set full frame uniforms including camera and lighting.
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.frame_uniforms = uniforms;
    }

    /// Get the frame uniforms for this frame.
    pub fn frame_uniforms(&self) -> &FrameUniforms {
        &self.frame_uniforms
    }

    /// Submit a single draw call.
    ///
    /// Returns a fluent builder for configuring the draw call.
    ///
    /// # Arguments
    /// * `mesh` - Mesh handle to draw
    /// * `material` - Material handle to use
    pub fn draw(&mut self, mesh: MeshHandle, material: MaterialHandle) -> DrawBuilder<'_> {
        DrawBuilder {
            frame: self,
            mesh,
            material,
            skeleton: None,
            transform: None,
            color: None,
            metallic: None,
            roughness: None,
            ao: None,
            emission: None,
            surface: MaterialSurface::default(),
            sampling: MaterialSampling::default(),
            tangent_uv: None,
            instances: Vec::new(),
        }
    }

    /// Submit an instanced draw call.
    ///
    /// All instances share the same mesh and material but have different transforms.
    ///
    /// # Arguments
    /// * `mesh` - Mesh handle to draw
    /// * `material` - Material handle to use
    /// * `instances` - Vector of instance data (transforms, colors, PBR params)
    pub fn draw_instanced(
        &mut self,
        mesh: MeshHandle,
        material: MaterialHandle,
        instances: Vec<InstanceData>,
    ) -> DrawBuilder<'_> {
        DrawBuilder {
            frame: self,
            mesh,
            material,
            skeleton: None,
            transform: None,
            color: None,
            metallic: None,
            roughness: None,
            ao: None,
            emission: None,
            surface: MaterialSurface::default(),
            sampling: MaterialSampling::default(),
            tangent_uv: None,
            instances,
        }
    }

    /// Take the accumulated draw list, resetting the frame context.
    ///
    /// This should be called once per frame to submit all draws to the renderer.
    /// After calling this, the frame context is reset and ready for the next frame.
    pub fn take_submission(&mut self) -> FrameSubmission {
        self.frame_uniforms = FrameUniforms::default(); // Reset to defaults
        FrameSubmission {
            draw_list: std::mem::take(&mut self.draw_list),
            surfaces: std::mem::replace(&mut self.surfaces, vec![SurfaceParameters::default()]),
            samplers: std::mem::replace(
                &mut self.samplers,
                vec![MaterialSampling::default().samplers()],
            ),
        }
    }

    /// Get the current draw list without taking it (for inspection).
    pub fn draw_list(&self) -> &DrawList {
        &self.draw_list
    }
    fn push_draw(
        &mut self,
        draw: DrawCall,
        surface: MaterialSurface,
        sampling: MaterialSampling,
        tangent_uv: Option<UvTransform>,
    ) -> u32 {
        let mut parameters: SurfaceParameters = surface.into();
        parameters = parameters.with_sampling(sampling, tangent_uv);
        parameters.coverage[3] = f32::from(super::material::mirrored_transform(
            &draw.instances[0].model_matrix,
        ));
        let count = draw.instance_count() as usize;
        let slot = self.draw_list.push(draw);
        let end = slot as usize + count;
        self.surfaces.resize(end, SurfaceParameters::default());
        self.surfaces[slot as usize..end].fill(parameters);
        self.samplers
            .resize(end, MaterialSampling::default().samplers());
        self.samplers[slot as usize..end].fill(sampling.samplers());
        slot
    }
}

/// Fluent builder for configuring draw calls.
///
/// Created by `FrameContext::draw()` or `draw_instanced()`.
/// Use the builder methods to configure the draw call, then call `submit()`.
pub struct DrawBuilder<'a> {
    /// Reference to parent frame context
    frame: &'a mut FrameContext,
    /// Mesh handle
    mesh: MeshHandle,
    /// Material handle
    material: MaterialHandle,
    /// Optional skeleton handle (for skinned meshes)
    skeleton: Option<SkeletonHandle>,
    /// Transform matrix (model to world)
    transform: Option<[f32; 16]>,
    /// Base color tint
    color: Option<[f32; 4]>,
    /// Metallic factor
    metallic: Option<f32>,
    /// Roughness factor
    roughness: Option<f32>,
    /// Ambient occlusion factor
    ao: Option<f32>,
    /// Emission texture handle
    emission: Option<TextureHandle>,
    surface: MaterialSurface,
    sampling: MaterialSampling,
    tangent_uv: Option<UvTransform>,
    /// Instance data for instanced rendering
    instances: Vec<InstanceData>,
}

impl<'a> DrawBuilder<'a> {
    /// Select independent texture coordinates and sampling policies.
    pub fn with_sampling(mut self, sampling: MaterialSampling) -> Self {
        self.sampling = sampling;
        self
    }
    pub(crate) fn with_tangent_uv(mut self, tangent_uv: Option<UvTransform>) -> Self {
        self.tangent_uv = tangent_uv;
        self
    }

    /// Set the drawable's linear emission and sampled normal/occlusion multipliers.
    pub fn with_surface(mut self, surface: MaterialSurface) -> Self {
        self.surface = surface;
        self
    }
    /// Set the model transform matrix (object to world transform).
    pub fn with_transform(mut self, matrix: [f32; 16]) -> Self {
        self.transform = Some(matrix);
        self
    }

    /// Set the base color tint (RGBA, 0.0-1.0 range).
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    /// Set PBR material parameters.
    ///
    /// # Arguments
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion (0.0 = full occlusion, 1.0 = none)
    pub fn with_pbr(mut self, metallic: f32, roughness: f32, ao: f32) -> Self {
        self.metallic = Some(metallic);
        self.roughness = Some(roughness);
        self.ao = Some(ao);
        self
    }

    /// Set the emission texture for self-illumination.
    ///
    /// # Arguments
    /// * `emission` - Emission texture handle (`NONE` = no emission)
    pub fn with_emission(mut self, emission: TextureHandle) -> Self {
        self.emission = Some(emission);
        self
    }

    /// Set skeleton handle for GPU skeletal animation.
    ///
    /// When set, the skeleton's joint matrices (Set 2) will be bound during rendering.
    ///
    /// # Arguments
    /// * `skeleton` - Skeleton handle with joint matrices
    pub fn with_skeleton(mut self, skeleton: SkeletonHandle) -> Self {
        self.skeleton = Some(skeleton);
        self
    }

    /// Submit the draw call to the frame context.
    ///
    /// This adds the draw call to the frame's draw list, assigning it a unique
    /// object storage slot range, and returns the assigned base slot.
    pub fn submit(self) -> u32 {
        let has_overrides = self.transform.is_some()
            || self.color.is_some()
            || self.metallic.is_some()
            || self.roughness.is_some()
            || self.ao.is_some();

        let instances = if self.instances.is_empty() {
            let mut instance = InstanceData::default();
            if let Some(transform) = self.transform {
                instance.model_matrix = transform;
            }
            if let Some(color) = self.color {
                instance.color = color;
            }
            if let Some(metallic) = self.metallic {
                instance.metallic = metallic;
            }
            if let Some(roughness) = self.roughness {
                instance.roughness = roughness;
            }
            if let Some(ao) = self.ao {
                instance.ao = ao;
            }
            vec![instance]
        } else if has_overrides {
            self.instances
                .into_iter()
                .map(|mut instance| {
                    if let Some(transform) = self.transform {
                        instance.model_matrix = transform;
                    }
                    if let Some(color) = self.color {
                        instance.color = color;
                    }
                    if let Some(metallic) = self.metallic {
                        instance.metallic = metallic;
                    }
                    if let Some(roughness) = self.roughness {
                        instance.roughness = roughness;
                    }
                    if let Some(ao) = self.ao {
                        instance.ao = ao;
                    }
                    instance
                })
                .collect()
        } else {
            self.instances
        };

        let first_mirrored = super::material::mirrored_transform(&instances[0].model_matrix);
        let split = self.surface.alpha_mode == super::AlphaMode::Blend
            || instances.iter().any(|instance| {
                super::material::mirrored_transform(&instance.model_matrix) != first_mirrored
            });
        let mut draw = DrawCall::instanced(self.mesh, self.material, instances)
            .with_emission(self.emission.unwrap_or(TextureHandle::NONE));
        draw.transparent = self.surface.alpha_mode == super::AlphaMode::Blend;
        if let Some(skeleton) = self.skeleton {
            draw = draw.with_skeleton(skeleton);
        }
        if !split {
            return self
                .frame
                .push_draw(draw, self.surface, self.sampling, self.tangent_uv);
        }
        let base_slot = self.frame.surfaces.len() as u32;
        for instance in draw.instances {
            let mut separated = DrawCall::new(draw.mesh, draw.material)
                .with_emission(draw.emission)
                .with_skeleton(draw.skeleton);
            separated.transparent = draw.transparent;
            separated.instances[0] = instance;
            self.frame
                .push_draw(separated, self.surface, self.sampling, self.tangent_uv);
        }
        base_slot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blended_and_mixed_handedness_instances_keep_slots_and_sort_individually() {
        let mut context = FrameContext::new();
        let mut first = InstanceData::default();
        first.model_matrix[14] = -1.25;
        let mut second = InstanceData::default();
        second.model_matrix[14] = -1.75;
        let mut third = InstanceData::default();
        third.model_matrix[14] = -1.5;
        let base = context
            .draw_instanced(
                MeshHandle::NONE,
                MaterialHandle::NONE,
                vec![first.clone(), second.clone(), third],
            )
            .with_surface(MaterialSurface {
                alpha_mode: super::super::AlphaMode::Blend,
                ..Default::default()
            })
            .submit();
        first.model_matrix[0] = -1.0;
        let opaque = context
            .draw_instanced(MeshHandle::NONE, MaterialHandle::NONE, vec![first, second])
            .submit();
        let mut submission = context.take_submission();
        assert_eq!(submission.draw_list.len(), 5);
        submission
            .draw_list
            .sort_for_view(&katla_math::Mat4::identity().to_array());
        let slots: Vec<_> = submission
            .draw_list
            .iter()
            .map(|draw| draw.base_object_slot())
            .collect();
        assert_eq!(slots, [opaque, opaque + 1, base + 1, base + 2, base]);
        assert_eq!(
            submission.surfaces[opaque as usize].cull_mode(),
            katla_gfx::CullMode::Front
        );
        assert_eq!(
            submission.surfaces[(opaque + 1) as usize].cull_mode(),
            katla_gfx::CullMode::Back
        );
        assert_eq!(submission.surfaces.len(), 6);
    }

    #[test]
    fn test_surface_slots_survive_material_sorting_and_reset_with_submission() {
        let mut context = FrameContext::new();
        let surface = MaterialSurface {
            emissive_factor: [2.0, 0.5, 0.0],
            normal_scale: 0.0,
            occlusion_strength: 0.25,
            ..Default::default()
        };
        let first = context
            .draw(MeshHandle::from_raw(1, 0), MaterialHandle::from_raw(2, 0))
            .with_surface(surface)
            .submit();
        let second = context
            .draw_instanced(
                MeshHandle::from_raw(2, 0),
                MaterialHandle::from_raw(1, 0),
                vec![InstanceData::default(); 2],
            )
            .submit();
        let mut submission = context.take_submission();
        submission.draw_list.sort_by_material();
        assert_eq!(
            submission
                .draw_list
                .iter()
                .next()
                .unwrap()
                .base_object_slot(),
            second
        );
        assert_eq!(submission.surfaces.len(), second as usize + 2);
        assert_eq!(
            submission.surfaces[first as usize].emissive,
            [2.0, 0.5, 0.0, 0.0]
        );
        assert_eq!(submission.surfaces[second as usize].emissive, [0.0; 4]);
        assert_eq!(
            submission.surfaces[second as usize + 1].normal_occlusion,
            [1.0, 1.0, 0.0, 0.0]
        );
        let empty = context.take_submission();
        assert!(empty.draw_list.is_empty());
        assert_eq!(empty.surfaces.len(), 1);
    }
}
