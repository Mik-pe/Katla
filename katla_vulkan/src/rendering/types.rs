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

/// Material parameters that can be set per draw call.
///
/// These are uploaded to uniform buffers before drawing.
/// Matrices are stored as [f32; 16] arrays in column-major order.
#[derive(Clone, Debug)]
pub struct MaterialParams {
    /// Model matrix (object to world transform) - column-major 4x4.
    pub model_matrix: [f32; 16],
    /// View matrix (world to camera transform) - column-major 4x4.
    pub view_matrix: [f32; 16],
    /// Projection matrix (camera to clip space) - column-major 4x4.
    pub proj_matrix: [f32; 16],
    /// Optional material color (RGBA, 0.0-1.0 range) for blending with texture.
    pub color: Option<[f32; 4]>,
}

impl Default for MaterialParams {
    fn default() -> Self {
        Self {
            model_matrix: [0.0; 16],
            view_matrix: [0.0; 16],
            proj_matrix: [0.0; 16],
            color: None,
        }
    }
}

impl MaterialParams {
    /// Create new material parameters with zero matrices.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model matrix from a 16-element array.
    pub fn with_model(mut self, model: [f32; 16]) -> Self {
        self.model_matrix = model;
        self
    }

    /// Set the view matrix from a 16-element array.
    pub fn with_view(mut self, view: [f32; 16]) -> Self {
        self.view_matrix = view;
        self
    }

    /// Set the projection matrix from a 16-element array.
    pub fn with_projection(mut self, proj: [f32; 16]) -> Self {
        self.proj_matrix = proj;
        self
    }

    /// Set all three matrices at once from 16-element arrays.
    pub fn with_matrices(mut self, model: [f32; 16], view: [f32; 16], proj: [f32; 16]) -> Self {
        self.model_matrix = model;
        self.view_matrix = view;
        self.proj_matrix = proj;
        self
    }

    /// Set the material color (RGBA, 0.0-1.0 range).
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    /// Get all three matrices as a contiguous byte array for GPU upload.
    ///
    /// Returns 192 bytes (3 matrices × 16 floats × 4 bytes) or 208 bytes if color is present.
    ///
    /// NOTE: When color is None, this returns 192 bytes. If the material was created
    /// with has_color=true, you should either provide a color via with_color() or
    /// the shader will read uninitialized memory for the color uniform!
    pub fn as_bytes(&self) -> Vec<u8> {
        // Combine all three matrices into a single array
        let combined = [self.model_matrix, self.view_matrix, self.proj_matrix].concat();

        // Use bytemuck for safe casting
        let mut bytes = bytemuck::cast_slice(&combined).to_vec();

        // Add color if present
        if let Some(color) = self.color {
            bytes.extend_from_slice(bytemuck::cast_slice(&color));
        }

        bytes
    }

    /// Get all three matrices plus color as a contiguous byte array for GPU upload.
    ///
    /// This always includes the color (defaulting to white [1.0, 1.0, 1.0, 1.0] if not set).
    /// Returns 208 bytes (3 matrices + color).
    ///
    /// Use this when the material was created with has_color=true to ensure the uniform
    /// buffer is the correct size and the color uniform is properly initialized.
    pub fn as_bytes_with_color(&self) -> Vec<u8> {
        // Combine all three matrices into a single array
        let combined = [self.model_matrix, self.view_matrix, self.proj_matrix].concat();

        // Use bytemuck for safe casting
        let mut bytes = bytemuck::cast_slice(&combined).to_vec();

        // Add color, defaulting to white if not specified
        let color = self.color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
        bytes.extend_from_slice(bytemuck::cast_slice(&color));

        bytes
    }
}

/// High-level draw call description.
///
/// Contains all information needed to render an object without exposing Vulkan types.
#[derive(Clone, Debug)]
pub struct DrawCall {
    /// Mesh to draw.
    pub mesh: MeshHandle,
    /// Material/shader to use.
    pub material: MaterialHandle,
    /// Transform matrices and uniforms.
    pub params: MaterialParams,
    /// Optional sorting key (for transparent objects, etc.).
    pub sort_key: Option<u64>,
    /// Object index for storage buffer access.
    /// When set, the shader uses this index to access per-object uniforms.
    /// When None, uses default uniform binding (legacy mode).
    pub object_index: Option<u32>,
}

impl DrawCall {
    /// Create a new draw call with default parameters.
    pub fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self {
            mesh,
            material,
            params: MaterialParams::default(),
            sort_key: None,
            object_index: None,
        }
    }

    /// Set the model transform matrix from a 16-element array.
    pub fn with_transform(mut self, model: [f32; 16]) -> Self {
        self.params.model_matrix = model;
        self
    }

    /// Set the camera matrices (view and projection) from 16-element arrays.
    pub fn with_camera(mut self, view: [f32; 16], proj: [f32; 16]) -> Self {
        self.params.view_matrix = view;
        self.params.proj_matrix = proj;
        self
    }

    /// Set all matrices at once from 16-element arrays.
    pub fn with_matrices(mut self, model: [f32; 16], view: [f32; 16], proj: [f32; 16]) -> Self {
        self.params.model_matrix = model;
        self.params.view_matrix = view;
        self.params.proj_matrix = proj;
        self
    }

    /// Set a sorting key for this draw call.
    ///
    /// Lower values are drawn first (useful for transparent objects).
    pub fn with_sort_key(mut self, key: u64) -> Self {
        self.sort_key = Some(key);
        self
    }

    /// Set the material color (RGBA, 0.0-1.0 range).
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.params.color = Some(color);
        self
    }

    /// Set the material parameters directly.
    pub fn with_params(mut self, params: MaterialParams) -> Self {
        self.params = params;
        self
    }

    /// Set the object index for storage buffer access.
    ///
    /// When using storage buffer-based uniforms, this index is used
    /// by the shader to access per-object data from the object array.
    pub fn with_object_index(mut self, index: u32) -> Self {
        self.object_index = Some(index);
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
    fn test_material_params_as_bytes() {
        let identity = [0.0; 16];
        let params = MaterialParams::new()
            .with_model(identity)
            .with_view(identity)
            .with_projection(identity);

        let bytes = params.as_bytes();
        assert_eq!(bytes.len(), 192); // 3 matrices * 16 floats * 4 bytes
    }

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
        let model = [0.0; 16];
        let view = [0.0; 16];
        let proj = [0.0; 16];

        let draw = DrawCall::new(mesh, material)
            .with_transform(model)
            .with_camera(view, proj)
            .with_sort_key(42);

        assert_eq!(draw.sort_key, Some(42));
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
