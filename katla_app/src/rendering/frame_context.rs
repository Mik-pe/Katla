//! Frame context for submitting draws with automatic instance allocation.
//!
//! This module provides a high-level API for submitting draw calls that automatically
//! manages instance index allocation. The fluent builder pattern makes it easy to
//! configure draw calls without manually tracking storage buffer offsets.
//!
//! # Example
//!
//! ```ignore
//! use katla_app::rendering::FrameContext;
//! use katla_gfx::{MeshHandle, MaterialHandle, renderer::FrameUniforms};
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
//!     light_intensity: 1.0,
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
//! let draw_list = frame.take_draw_list();
//! ```

use katla_gfx::{
    renderer::{DrawCall, DrawList, FrameUniforms, InstanceData},
    MaterialHandle, MeshHandle, SkeletonHandle,
};

/// Per-frame context for submitting draws with automatic instance allocation.
///
/// The FrameContext tracks instance index allocation and provides fluent builders
/// for configuring draw calls. Instance indices are allocated sequentially and
/// automatically reset when `take_draw_list()` is called.
pub struct FrameContext {
    /// Next instance index to allocate
    next_instance_index: u32,
    /// Accumulated draw calls for this frame
    draw_list: DrawList,
    /// Frame uniforms (camera, lighting) - from katla_gfx public API
    /// Always set via set_camera() or set_frame_uniforms() before rendering
    frame_uniforms: FrameUniforms,
    /// Maximum instances per frame (panic in debug if exceeded)
    max_instances: u32,
}

impl Default for FrameContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameContext {
    /// Create a new empty frame context.
    ///
    /// Instance counter starts at 1 (slot 0 is reserved for fullscreen passes)
    /// and is reset when `take_draw_list()` is called.
    /// Frame uniforms are initialized to defaults (identity matrices, origin camera).
    pub fn new() -> Self {
        Self {
            next_instance_index: 1, // Slot 0 reserved for fullscreen/post-processing passes
            draw_list: DrawList::new(),
            frame_uniforms: FrameUniforms::default(),
            max_instances: 256, // Match StorageUniformLayout::MAX_OBJECTS
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
            light_intensity: 1.0,                  // Base intensity for PBR
            tiles: [0, 0, 0, 0],
        };
    }

    /// Set full frame uniforms including camera and lighting.
    ///
    /// Use this for custom lighting configurations.
    pub fn set_frame_uniforms(&mut self, uniforms: FrameUniforms) {
        self.frame_uniforms = uniforms;
    }

    /// Get the frame uniforms for this frame.
    pub fn frame_uniforms(&self) -> &FrameUniforms {
        &self.frame_uniforms
    }

    /// Submit a single draw call (allocates 1 instance slot).
    ///
    /// Returns a fluent builder for configuring the draw call.
    ///
    /// # Arguments
    /// * `mesh` - Mesh handle to draw
    /// * `material` - Material handle to use
    pub fn draw(&mut self, mesh: MeshHandle, material: MaterialHandle) -> DrawBuilder<'_> {
        let instance_idx = self.alloc_instance(1);
        DrawBuilder {
            frame: self,
            instance_index: instance_idx,
            mesh,
            material,
            skeleton: None,
            transform: None,
            color: None,
            metallic: None,
            roughness: None,
            ao: None,
            emission: None,
            instances: Vec::new(),
        }
    }

    /// Submit an instanced draw call (allocates N instance slots).
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
        let start_idx = self.alloc_instance(instances.len() as u32);
        DrawBuilder {
            frame: self,
            instance_index: start_idx,
            mesh,
            material,
            skeleton: None,
            transform: None,
            color: None,
            metallic: None,
            roughness: None,
            ao: None,
            emission: None,
            instances,
        }
    }

    /// Allocate instance slots and return the starting index.
    fn alloc_instance(&mut self, count: u32) -> u32 {
        let start_idx = self.next_instance_index;
        self.next_instance_index += count;

        // Panic in debug mode if we exceed max instances
        if cfg!(debug_assertions) && self.next_instance_index > self.max_instances {
            panic!(
                "FrameContext: exceeded maximum instances per frame ({})",
                self.max_instances
            );
        }

        start_idx
    }

    /// Get the current instance count (number of slots allocated).
    pub fn instance_count(&self) -> u32 {
        self.next_instance_index
    }

    /// Take the accumulated draw list, resetting the frame context.
    ///
    /// This should be called once per frame to submit all draws to the renderer.
    /// After calling this, the frame context is reset and ready for the next frame.
    pub fn take_draw_list(&mut self) -> DrawList {
        self.next_instance_index = 1; // Slot 0 reserved for fullscreen passes
        self.frame_uniforms = FrameUniforms::default(); // Reset to defaults
        std::mem::take(&mut self.draw_list)
    }

    /// Get the current draw list without taking it (for inspection).
    pub fn draw_list(&self) -> &DrawList {
        &self.draw_list
    }

    /// Get mutable access to the current draw list.
    pub fn draw_list_mut(&mut self) -> &mut DrawList {
        &mut self.draw_list
    }

    /// Add a draw call directly to the draw list (advanced usage).
    ///
    /// This bypasses the fluent builder and instance allocation.
    /// Use `draw()` or `draw_instanced()` for normal usage.
    pub fn push_draw(&mut self, draw: DrawCall) {
        self.draw_list.push(draw);
    }
}

/// Fluent builder for configuring draw calls.
///
/// Created by `FrameContext::draw()` or `draw_instanced()`.
/// Use the builder methods to configure the draw call, then call `submit()`.
pub struct DrawBuilder<'a> {
    /// Reference to parent frame context
    frame: &'a mut FrameContext,
    /// Allocated instance index for this draw
    instance_index: u32,
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
    /// Emission texture bindless index
    emission: Option<f32>,
    /// Instance data for instanced rendering
    instances: Vec<InstanceData>,
}

impl<'a> DrawBuilder<'a> {
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

    /// Set emission texture index for self-illumination.
    ///
    /// # Arguments
    /// * `emission` - Emission texture bindless index (0.0 = no emission)
    pub fn with_emission(mut self, emission: f32) -> Self {
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
    /// This writes the per-object data to the storage buffer (at the allocated
    /// instance index) and adds the draw call to the frame's draw list.
    pub fn submit(self) {
        // Default values
        let transform = self.transform.unwrap_or([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let color = self.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let metallic = self.metallic.unwrap_or(0.0);
        let roughness = self.roughness.unwrap_or(0.5);
        let ao = self.ao.unwrap_or(1.0);
        let emission = self.emission.unwrap_or(0.0);

        if self.instances.is_empty() {
            // Single draw (or skinned mesh)
            let mut draw_call = DrawCall::new(self.mesh, self.material)
                .with_transform(transform)
                .with_color(color)
                .with_pbr(metallic, roughness, ao)
                .with_emission(emission)
                .with_instance_index(self.instance_index);

            // Add skeleton if present
            if let Some(skeleton) = self.skeleton {
                draw_call = draw_call.with_skeleton(skeleton);
            }

            self.frame.push_draw(draw_call);
        } else {
            // Instanced draw
            let draw_call = DrawCall::instanced(self.mesh, self.material, self.instances.clone())
                .with_instance_index(self.instance_index);

            self.frame.push_draw(draw_call);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_context_new() {
        let frame = FrameContext::new();
        assert_eq!(frame.instance_count(), 0);
    }

    #[test]
    fn test_frame_context_set_camera() {
        let mut frame = FrameContext::new();
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let proj = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let camera_pos = [0.0, 0.0, 0.0, 1.0];
        frame.set_camera(&view, &proj, &camera_pos);

        let uniforms = frame.frame_uniforms();
        assert_eq!(uniforms.view_matrix, view);
        assert_eq!(uniforms.proj_matrix, proj);
    }

    #[test]
    fn test_instance_allocation() {
        let mut frame = FrameContext::new();

        // Each draw allocates 1 instance
        let _ = frame.draw(MeshHandle::NONE, MaterialHandle::NONE);
        assert_eq!(frame.instance_count(), 1);

        let _ = frame.draw(MeshHandle::NONE, MaterialHandle::NONE);
        assert_eq!(frame.instance_count(), 2);

        let _ = frame.draw(MeshHandle::NONE, MaterialHandle::NONE);
        assert_eq!(frame.instance_count(), 3);
    }

    #[test]
    fn test_instanced_allocation() {
        let mut frame = FrameContext::new();

        // Instanced draw with 5 instances
        let instances = vec![InstanceData::new(); 5];
        let _ = frame.draw_instanced(MeshHandle::NONE, MaterialHandle::NONE, instances);

        assert_eq!(frame.instance_count(), 5);
    }

    #[test]
    fn test_take_reset() {
        let mut frame = FrameContext::new();

        frame.draw(MeshHandle::NONE, MaterialHandle::NONE).submit();
        assert_eq!(frame.instance_count(), 1);

        let list = frame.take_draw_list();
        assert_eq!(frame.instance_count(), 0);
        assert!(!list.is_empty());
    }

    #[test]
    #[should_panic(expected = "exceeded maximum instances")]
    #[cfg(debug_assertions)]
    fn test_max_instances_panic() {
        let mut frame = FrameContext::new();
        frame.max_instances = 10;

        // Allocate more than max_instances
        for _ in 0..11 {
            let _ = frame.draw(MeshHandle::NONE, MaterialHandle::NONE);
        }
    }
}
