//! High-level rendering types for deferred draw call submission.
//!
//! This module provides types that avoid exposing ash::vk to the application layer.
//! Mesh and material data is registered with the renderer and referenced via opaque handles.

/// High-level mesh handle - opaque token, no ash types exposed.
///
/// The actual mesh data (vertex/index buffers) is stored internally in the AssetRegistry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub usize);

/// High-level material handle - opaque token.
///
/// The actual material (pipeline, textures, descriptors) is stored internally.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub usize);

/// Handle to a skeleton descriptor set for GPU skinning.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkeletonHandle(pub u32);

/// Frame-level uniforms that are shared across all draw calls.
///
/// These are set once per frame via `renderer.set_frame_uniforms()`.
/// View/projection matrices come from the camera, lighting from the scene.
#[derive(Clone, Debug)]
pub struct FrameUniforms {
    /// View matrix (world to camera transform) - column-major 4x4.
    pub view_matrix: [f32; 16],
    /// Projection matrix (camera to clip space) - column-major 4x4.
    pub proj_matrix: [f32; 16],
    /// Inverse view-projection matrix (clip to world space) - for sky rendering.
    pub inv_view_proj_matrix: [f32; 16],
    /// Camera position in world space.
    pub camera_position: [f32; 4],
    /// Light direction (normalized, points TO the light).
    pub light_direction: [f32; 4],
    /// Light color (RGB).
    pub light_color: [f32; 4],
    /// Light intensity.
    pub light_intensity: f32,
}

impl Default for FrameUniforms {
    fn default() -> Self {
        Self {
            view_matrix: [0.0; 16],
            proj_matrix: [0.0; 16],
            inv_view_proj_matrix: [0.0; 16],
            camera_position: [0.0, 0.0, 0.0, 0.0],
            light_direction: [-0.3, -1.0, -0.2, 0.0],
            light_color: [1.0, 0.95, 0.9, 0.0],
            light_intensity: 1.0,
        }
    }
}

/// High-level draw call description.
///
/// Contains all per-object information needed to render without exposing Vulkan types.
/// Frame-level data (view/proj matrices, lighting) is set separately via `set_frame_uniforms()`.
#[derive(Clone, Debug)]
pub struct DrawCall {
    /// Mesh to draw.
    pub mesh: MeshHandle,
    /// Material/shader to use.
    pub material: MaterialHandle,
    /// Model matrix (object to world transform) - column-major 4x4.
    pub model_matrix: [f32; 16],
    /// Optional material color (RGBA, 0.0-1.0 range) for blending with texture.
    pub color: Option<[f32; 4]>,
    /// PBR material parameters: metallic (0.0-1.0).
    /// 0.0 = dielectric (non-metal), 1.0 = full metal.
    pub metallic: f32,
    /// PBR material parameters: roughness (0.0-1.0).
    /// 0.0 = perfectly smooth (mirror-like), 1.0 = completely rough (diffuse).
    pub roughness: f32,
    /// Ambient occlusion factor (0.0-1.0).
    /// 0.0 = fully occluded, 1.0 = no occlusion.
    pub ao: f32,
    /// Optional sorting key (for transparent objects, etc.).
    pub sort_key: Option<u64>,
    /// Skeleton handle for GPU skinning (Set 2).
    /// Only set for animated meshes using skinned shaders.
    pub skeleton: Option<SkeletonHandle>,
}

impl DrawCall {
    /// Create a new draw call with default parameters.
    pub fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self {
            mesh,
            material,
            model_matrix: [0.0; 16],
            color: None,
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
            sort_key: None,
            skeleton: None,
        }
    }

    /// Set the model transform matrix from a 16-element array.
    pub fn with_transform(mut self, model: [f32; 16]) -> Self {
        self.model_matrix = model;
        self
    }

    /// Set the material color (RGBA, 0.0-1.0 range).
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    /// Set PBR metallic factor (0.0 = dielectric, 1.0 = metal).
    pub fn with_metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic;
        self
    }

    /// Set PBR roughness factor (0.0 = smooth, 1.0 = rough).
    pub fn with_roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness;
        self
    }

    /// Set ambient occlusion factor (0.0 = occluded, 1.0 = no occlusion).
    pub fn with_ao(mut self, ao: f32) -> Self {
        self.ao = ao;
        self
    }

    /// Set all PBR material parameters at once.
    pub fn with_pbr(mut self, metallic: f32, roughness: f32, ao: f32) -> Self {
        self.metallic = metallic;
        self.roughness = roughness;
        self.ao = ao;
        self
    }

    /// Set a sorting key for this draw call.
    ///
    /// Lower values are drawn first (useful for transparent objects).
    pub fn with_sort_key(mut self, key: u64) -> Self {
        self.sort_key = Some(key);
        self
    }

    /// Set skeleton handle for GPU skinning.
    pub fn with_skeleton(mut self, skeleton: SkeletonHandle) -> Self {
        self.skeleton = Some(skeleton);
        self
    }

    // === Deprecated backward-compat methods ===

    /// Set the camera matrices (view and projection).
    #[deprecated(since = "0.2.0", note = "Use renderer.set_frame_uniforms() instead")]
    pub fn with_camera(self, _view: [f32; 16], _proj: [f32; 16]) -> Self {
        // No-op: view/proj are now set via set_frame_uniforms()
        self
    }

    /// Set all matrices at once.
    #[deprecated(since = "0.2.0", note = "Use with_transform() and renderer.set_frame_uniforms() instead")]
    pub fn with_matrices(mut self, model: [f32; 16], _view: [f32; 16], _proj: [f32; 16]) -> Self {
        self.model_matrix = model;
        self
    }

    /// Set all matrices including inverse view-projection.
    #[deprecated(since = "0.2.0", note = "Use with_transform() and renderer.set_frame_uniforms() instead")]
    pub fn with_all_matrices(
        mut self,
        model: [f32; 16],
        _view: [f32; 16],
        _proj: [f32; 16],
        _inv_view_proj: [f32; 16],
    ) -> Self {
        self.model_matrix = model;
        self
    }

    /// Set the object index for storage buffer access.
    #[deprecated(since = "0.2.0", note = "Object index is now auto-assigned")]
    pub fn with_object_index(self, _index: u32) -> Self {
        // No-op: object index is auto-assigned
        self
    }
}

/// A collection of draw calls to be submitted together.
///
/// This allows the application to batch draw calls and optimize them
/// before submitting to the renderer.
#[derive(Clone, Debug)]
pub struct DrawList {
    /// The draw calls in this list.
    pub draws: Vec<DrawCall>,
}

impl DrawList {
    /// Create a new empty draw list.
    pub fn new() -> Self {
        Self { draws: Vec::new() }
    }

    /// Add a draw call to the list.
    pub fn push(&mut self, draw: DrawCall) {
        self.draws.push(draw);
    }

    /// Extend this list with all draws from another list.
    pub fn extend(&mut self, other: &mut DrawList) {
        self.draws.append(&mut other.draws);
    }

    /// Clear all draw calls from the list.
    pub fn clear(&mut self) {
        self.draws.clear();
    }

    /// Get the number of draw calls in the list.
    pub fn len(&self) -> usize {
        self.draws.len()
    }

    /// Check if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.draws.is_empty()
    }

    /// Get an iterator over the draw calls.
    pub fn iter(&self) -> impl Iterator<Item = &DrawCall> {
        self.draws.iter()
    }

    /// Get a mutable iterator over the draw calls.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut DrawCall> {
        self.draws.iter_mut()
    }

    /// Sort by sort_key (useful for transparency, etc.).
    ///
    /// Draws without a sort key are sorted last.
    pub fn sort(&mut self) {
        self.draws.sort_by_key(|d| d.sort_key.unwrap_or(u64::MAX));
    }

    /// Sort by material (reduces state changes during rendering).
    pub fn sort_by_material(&mut self) {
        self.draws.sort_by_key(|d| d.material.0);
    }

    /// Sort by mesh (can improve vertex buffer binding cache).
    pub fn sort_by_mesh(&mut self) {
        self.draws.sort_by_key(|d| d.mesh.0);
    }

    /// Reverse the order of draw calls (useful for certain transparency techniques).
    pub fn reverse(&mut self) {
        self.draws.reverse();
    }

    /// Reserve capacity for additional draw calls.
    pub fn reserve(&mut self, additional: usize) {
        self.draws.reserve(additional);
    }

    /// Remove all draw calls that match the predicate.
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&DrawCall) -> bool,
    {
        self.draws.retain(f);
    }
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for DrawList {
    type Item = DrawCall;
    type IntoIter = std::vec::IntoIter<DrawCall>;

    fn into_iter(self) -> Self::IntoIter {
        self.draws.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_call_creation() {
        let mesh = MeshHandle(0);
        let material = MaterialHandle(0);

        let draw = DrawCall::new(mesh, material);

        assert_eq!(draw.mesh, mesh);
        assert_eq!(draw.material, material);
    }

    #[test]
    fn test_draw_call_builder() {
        let mesh = MeshHandle(0);
        let material = MaterialHandle(0);
        let model = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

        let draw = DrawCall::new(mesh, material)
            .with_transform(model)
            .with_color([1.0, 0.0, 0.0, 1.0])
            .with_pbr(0.5, 0.3, 1.0)
            .with_sort_key(42);

        assert_eq!(draw.sort_key, Some(42));
        assert_eq!(draw.color, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(draw.metallic, 0.5);
        assert_eq!(draw.roughness, 0.3);
    }

    #[test]
    fn test_frame_uniforms_default() {
        let frame = FrameUniforms::default();
        assert_eq!(frame.light_intensity, 1.0);
    }

    #[test]
    fn test_draw_list() {
        let mut list = DrawList::new();

        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        let mesh = MeshHandle(0);
        let material = MaterialHandle(0);

        list.push(DrawCall::new(mesh, material));

        assert!(!list.is_empty());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_draw_list_sorting() {
        let mut list = DrawList::new();
        let mesh = MeshHandle(0);
        let material = MaterialHandle(0);

        list.push(DrawCall::new(mesh, material).with_sort_key(3));
        list.push(DrawCall::new(mesh, material).with_sort_key(1));
        list.push(DrawCall::new(mesh, material).with_sort_key(2));

        list.sort();

        assert_eq!(list.draws[0].sort_key, Some(1));
        assert_eq!(list.draws[1].sort_key, Some(2));
        assert_eq!(list.draws[2].sort_key, Some(3));
    }

    #[test]
    fn test_draw_list_into_iter() {
        let mut list = DrawList::new();
        let mesh = MeshHandle(0);
        let material = MaterialHandle(0);

        list.push(DrawCall::new(mesh, material));
        list.push(DrawCall::new(mesh, material));

        let count = list.into_iter().count();
        assert_eq!(count, 2);
    }
}
