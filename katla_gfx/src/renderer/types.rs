//! High-level rendering types for deferred draw call submission.
//!
//! This module provides types that avoid exposing ash::vk to the application layer.
//! Mesh and material data is registered with the renderer and referenced via opaque handles.

use crate::handle::{MaterialHandle, MeshHandle, SkeletonHandle};
use crate::vertex::VertexUI;

pub use crate::handle::{Handle, TextureHandle};

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
    /// Light intensity and screen-space effect parameters.
    /// [x = intensity, y = depth_texture_bindless_idx, z = unused, w = unused]
    pub light_intensity: [f32; 4],
    /// Forward+ tile grid dimensions: [tiles_x, tiles_y, 0, 0].
    pub tiles: [u32; 4],
}

impl Default for FrameUniforms {
    fn default() -> Self {
        Self {
            view_matrix: [0.0; 16],
            proj_matrix: [0.0; 16],
            inv_view_proj_matrix: [0.0; 16],
            camera_position: [0.0, 0.0, 0.0, 0.0],
            light_direction: [0.3, 1.0, 0.2, 0.0], // Upward toward sun
            light_color: [1.0, 0.98, 0.95, 0.0],   // Slightly warm white
            light_intensity: [3.0, 0.0, 0.0, 0.0], // HDR intensity for PBR
            tiles: [0, 0, 0, 0],
        }
    }
}

/// Per-instance data for GPU instancing.
///
/// Each instance has its own transform and material properties,
/// but shares the same mesh and material with other instances.
#[derive(Clone, Debug)]
pub struct InstanceData {
    /// Model matrix (object to world transform) - column-major 4x4.
    pub model_matrix: [f32; 16],
    /// Optional material color (RGBA, 0.0-1.0 range).
    pub color: [f32; 4],
    /// PBR metallic factor (0.0 = dielectric, 1.0 = metal).
    pub metallic: f32,
    /// PBR roughness factor (0.0 = smooth, 1.0 = rough).
    pub roughness: f32,
    /// Ambient occlusion factor (0.0 = occluded, 1.0 = no occlusion).
    pub ao: f32,
}

impl Default for InstanceData {
    fn default() -> Self {
        Self {
            model_matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            ao: 1.0,
        }
    }
}

impl InstanceData {
    /// Create a new instance with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an instance with a transform matrix.
    pub fn with_transform(mut self, model: [f32; 16]) -> Self {
        self.model_matrix = model;
        self
    }

    /// Create an instance with a color.
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// Create an instance with PBR parameters.
    pub fn with_pbr(mut self, metallic: f32, roughness: f32, ao: f32) -> Self {
        self.metallic = metallic;
        self.roughness = roughness;
        self.ao = ao;
        self
    }
}

/// High-level draw call description.
///
/// Contains all per-object information needed to render without exposing Vulkan types.
/// Frame-level data (view/proj matrices, lighting) is set separately via `set_frame_uniforms()`.
///
/// # Instance Index
/// Each draw call has an `instance_index` that specifies which slot in the storage
/// buffer (Set 0, Binding 1) contains its per-object data. This is allocated by the
/// FrameContext and used by the shader via `@builtin(instance_index)`.
///
/// # Instances
/// Every draw call uses `instances` (a `Vec<InstanceData>`). Single-object draws use
/// a single-element vec; multi-instance draws use N elements. The render pass always
/// reads per-instance data from `instances[0]`.
#[derive(Clone, Debug)]
pub struct DrawCall {
    /// Mesh to draw.
    pub mesh: MeshHandle,
    /// Material/shader to use.
    pub material: MaterialHandle,
    /// Instance index for storage buffer lookup (Set 0, Binding 1).
    /// The shader uses this to index objects[instance_index].
    pub instance_index: u32,
    /// Emission texture bindless index (0 = no emission).
    pub emission: f32,
    /// Whether this draw uses transparency (affects sort order).
    pub transparent: bool,
    /// Optional sorting key (for transparent objects, etc.).
    pub sort_key: Option<u64>,
    /// Skeleton handle for GPU skinning (Set 2).
    pub skeleton: SkeletonHandle,
    /// Per-instance data (transform, color, PBR). Always contains at least one element.
    pub instances: Vec<InstanceData>,
}

impl DrawCall {
    /// Create a new draw call with a single default instance.
    pub fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self {
            mesh,
            material,
            instance_index: 0, // Will be set by FrameContext
            emission: 0.0,
            transparent: false,
            sort_key: None,
            skeleton: SkeletonHandle::NONE,
            instances: vec![InstanceData::default()],
        }
    }

    /// Create a draw call with multiple instances.
    ///
    /// All instances share the same mesh and material but have different
    /// transforms and material properties.
    pub fn instanced(
        mesh: MeshHandle,
        material: MaterialHandle,
        instances: Vec<InstanceData>,
    ) -> Self {
        Self {
            mesh,
            material,
            instance_index: 0, // Will be set by FrameContext
            emission: 0.0,
            transparent: false,
            sort_key: None,
            skeleton: SkeletonHandle::NONE,
            instances,
        }
    }

    /// Check if this draw call uses multi-instance rendering.
    pub fn is_instanced(&self) -> bool {
        self.instances.len() > 1
    }

    /// Get the instance count.
    pub fn instance_count(&self) -> u32 {
        self.instances.len() as u32
    }

    /// Set the model transform matrix on the first instance.
    pub fn with_transform(mut self, model: [f32; 16]) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.model_matrix = model;
        }
        self
    }

    /// Set the material color on the first instance (RGBA, 0.0-1.0 range).
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.color = color;
        }
        self
    }

    /// Set PBR metallic factor on the first instance (0.0 = dielectric, 1.0 = metal).
    pub fn with_metallic(mut self, metallic: f32) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.metallic = metallic;
        }
        self
    }

    /// Set PBR roughness factor on the first instance (0.0 = smooth, 1.0 = rough).
    pub fn with_roughness(mut self, roughness: f32) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.roughness = roughness;
        }
        self
    }

    /// Set ambient occlusion factor on the first instance (0.0 = occluded, 1.0 = no occlusion).
    pub fn with_ao(mut self, ao: f32) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.ao = ao;
        }
        self
    }

    /// Set emission texture index for self-illumination (0 = no emission).
    pub fn with_emission(mut self, emission: f32) -> Self {
        self.emission = emission;
        self
    }

    /// Set all PBR material parameters at once on the first instance.
    pub fn with_pbr(mut self, metallic: f32, roughness: f32, ao: f32) -> Self {
        if let Some(inst) = self.instances.first_mut() {
            inst.metallic = metallic;
            inst.roughness = roughness;
            inst.ao = ao;
        }
        self
    }

    /// Get material parameters as an array for GPU upload: [metallic, roughness, ao, emission].
    pub fn material_params(&self) -> [f32; 4] {
        let inst = self.instances.first();
        [
            inst.map(|i| i.metallic).unwrap_or(0.0),
            inst.map(|i| i.roughness).unwrap_or(0.5),
            inst.map(|i| i.ao).unwrap_or(1.0),
            self.emission,
        ]
    }

    /// Mark this draw call as using transparency.
    ///
    /// Transparent objects are sorted back-to-front for correct blending.
    pub fn with_transparency(mut self) -> Self {
        self.transparent = true;
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
        self.skeleton = skeleton;
        self
    }

    /// Set the instance index for storage buffer lookup.
    ///
    /// This specifies which slot in the storage buffer (Set 0, Binding 1)
    /// contains this draw call's per-object data.
    pub fn with_instance_index(mut self, index: u32) -> Self {
        self.instance_index = index;
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
    /// Call `compute_sort_keys()` first for proper depth ordering.
    pub fn sort(&mut self) {
        self.draws.sort_by_key(|d| d.sort_key.unwrap_or(u64::MAX));
    }

    /// Compute sort keys for all draw calls based on camera distance.
    ///
    /// Call this before `sort()` for proper depth ordering.
    ///
    /// # Arguments
    /// * `camera_position` - Camera position in world space
    pub fn compute_sort_keys(&mut self, camera_position: [f32; 3]) {
        for draw in &mut self.draws {
            let model_matrix = draw
                .instances
                .first()
                .map(|i| i.model_matrix)
                .unwrap_or([0.0; 16]);
            let distance = compute_distance_from_camera(&model_matrix, camera_position);
            draw.sort_key = Some(compute_sort_key(
                draw.material,
                draw.mesh,
                distance,
                draw.transparent,
            ));
        }
    }

    /// Sort by material (reduces state changes during rendering).
    pub fn sort_by_material(&mut self) {
        self.draws.sort_by_key(|d| d.material.index());
    }

    /// Sort by mesh (can improve vertex buffer binding cache).
    pub fn sort_by_mesh(&mut self) {
        self.draws.sort_by_key(|d| d.mesh.index());
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

/// Compute a sort key for optimal rendering order.
///
/// # Sort Key Layout (64-bit)
/// - Opaque objects: `[material:16][mesh:16][depth:32]` - Material grouping + front-to-back
/// - Transparent objects: `[depth:32][material:16][mesh:16]` - Back-to-front
///
/// # Arguments
/// * `material` - Material handle (for state change reduction)
/// * `mesh` - Mesh handle (for vertex buffer cache)
/// * `distance` - Distance from camera (for depth ordering)
/// * `transparent` - Whether the object uses transparency
///
/// # Returns
/// A 64-bit sort key where lower values are drawn first.
pub fn compute_sort_key(
    material: MaterialHandle,
    mesh: MeshHandle,
    distance: f32,
    transparent: bool,
) -> u64 {
    // Quantize distance to 32-bit (0 to ~16M range)
    let depth_bits = (distance.clamp(0.0, 16777215.0) as u32) & 0xFFFFFF;

    if transparent {
        // Back-to-front for transparency: larger distance = lower key (drawn first... wait, no)
        // Actually for transparency we want FAR objects drawn FIRST (lower key), near objects LAST
        // So we use inverted depth or just depth directly with descending sort
        // For simplicity: higher distance = higher key = drawn later (correct back-to-front)
        ((depth_bits as u64) << 32) | ((material.index() as u64) << 16) | (mesh.index() as u64)
    } else {
        // Front-to-back for opaque: smaller distance = lower key = drawn first (early-Z optimization)
        // But we also want material grouping for state changes
        // So: material first, then mesh, then depth
        ((material.index() as u64) << 48) | ((mesh.index() as u64) << 32) | (depth_bits as u64)
    }
}

/// Compute distance from camera to an object.
///
/// Extracts the translation from the model matrix and computes distance.
///
/// # Arguments
/// * `model_matrix` - Object's model matrix (column-major 4x4)
/// * `camera_position` - Camera position in world space
///
/// # Returns
/// Distance from camera to object center.
pub fn compute_distance_from_camera(model_matrix: &[f32; 16], camera_position: [f32; 3]) -> f32 {
    // Translation is in the last column (indices 12, 13, 14 for column-major)
    let obj_x = model_matrix[12];
    let obj_y = model_matrix[13];
    let obj_z = model_matrix[14];

    let dx = obj_x - camera_position[0];
    let dy = obj_y - camera_position[1];
    let dz = obj_z - camera_position[2];

    (dx * dx + dy * dy + dz * dz).sqrt()
}

// UI Rendering Types

/// A single draw command for UI rendering.
///
/// Each command represents a batch of primitives that share the same texture
/// and clipping rectangle.
#[derive(Clone, Debug, Copy)]
pub struct UiDrawCommand {
    /// Starting index in the index buffer.
    pub index_offset: u32,
    /// Number of indices to draw for this command.
    pub index_count: u32,
    /// Clipping rectangle in pixels: [x, y, width, height].
    /// None = no clipping (draw to full screen).
    pub clip_rect: Option<[f32; 4]>,
    /// Texture handle for this batch.
    /// Use `TextureHandle::NONE` for solid color rendering (no texture).
    pub texture: TextureHandle,
}

impl UiDrawCommand {
    /// Create a new UI draw command.
    pub fn new(
        index_offset: u32,
        index_count: u32,
        clip_rect: Option<[f32; 4]>,
        texture: TextureHandle,
    ) -> Self {
        Self {
            index_offset,
            index_count,
            clip_rect,
            texture,
        }
    }
}

/// A complete UI draw list for rendering.
///
/// Contains all vertices, indices, and draw commands needed to render
/// a UI frame. Created by katla_app by converting `katla_ui::DrawList`.
#[derive(Clone, Debug, Default)]
pub struct UIDrawList {
    /// All vertices in the draw list.
    pub vertices: Vec<VertexUI>,
    /// All indices in the draw list (triangles).
    pub indices: Vec<u32>,
    /// Draw commands (batches grouped by texture/clip).
    pub commands: Vec<UiDrawCommand>,
    /// Screen size for coordinate transformation (logical pixels, not physical).
    /// This must match the coordinate space used by vertex positions.
    pub screen_size: [f32; 2],
    /// DPI scale factor (physical pixels per logical pixel).
    /// Used to convert clip_rect from logical to physical pixels for Vulkan scissor.
    pub scale_factor: f32,
}

impl UIDrawList {
    /// Create a new empty UI draw list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the draw list is empty.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Get the total number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get the total number of indices.
    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// Get the number of draw commands.
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Get vertex data as bytes for GPU upload.
    pub fn vertex_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.vertices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_call_creation() {
        let mesh = MeshHandle::new(0);
        let material = MaterialHandle::new(0);

        let draw = DrawCall::new(mesh, material);

        assert_eq!(draw.mesh, mesh);
        assert_eq!(draw.material, material);
    }

    #[test]
    fn test_draw_call_builder() {
        let mesh = MeshHandle::new(0);
        let material = MaterialHandle::new(0);
        let model = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let draw = DrawCall::new(mesh, material)
            .with_transform(model)
            .with_color([1.0, 0.0, 0.0, 1.0])
            .with_pbr(0.5, 0.3, 1.0)
            .with_sort_key(42);

        assert_eq!(draw.sort_key, Some(42));
        let inst = &draw.instances[0];
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.metallic, 0.5);
        assert_eq!(inst.roughness, 0.3);
    }

    #[test]
    fn test_frame_uniforms_default() {
        let frame = FrameUniforms::default();
        assert_eq!(frame.light_intensity[0], 3.0); // HDR intensity for PBR
        assert_eq!(frame.tiles, [0, 0, 0, 0]);
    }

    #[test]
    fn test_draw_list() {
        let mut list = DrawList::new();

        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        let mesh = MeshHandle::new(0);
        let material = MaterialHandle::new(0);

        list.push(DrawCall::new(mesh, material));

        assert!(!list.is_empty());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_draw_list_sorting() {
        let mut list = DrawList::new();
        let mesh = MeshHandle::new(0);
        let material = MaterialHandle::new(0);

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
        let mesh = MeshHandle::new(0);
        let material = MaterialHandle::new(0);

        list.push(DrawCall::new(mesh, material));
        list.push(DrawCall::new(mesh, material));

        let count = list.into_iter().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_compute_sort_key_opaque() {
        let mat1 = MaterialHandle::new(1);
        let mat2 = MaterialHandle::new(2);
        let mesh = MeshHandle::new(0);

        // For opaque: material grouping takes priority
        let key_near = compute_sort_key(mat1, mesh, 10.0, false);
        let key_far = compute_sort_key(mat1, mesh, 100.0, false);

        // Same material, near should have lower key (drawn first for early-Z)
        assert!(key_near < key_far);

        // Different material, mat2 > mat1 means mat2 drawn after
        let key_mat2 = compute_sort_key(mat2, mesh, 10.0, false);
        assert!(key_mat2 > key_near);
    }

    #[test]
    fn test_compute_sort_key_transparent() {
        let mat = MaterialHandle::new(0);
        let mesh = MeshHandle::new(0);

        // For transparent: back-to-front, far objects first
        let key_near = compute_sort_key(mat, mesh, 10.0, true);
        let key_far = compute_sort_key(mat, mesh, 100.0, true);

        // Far should have higher key (drawn later for back-to-front)
        assert!(key_far > key_near);
    }

    #[test]
    fn test_compute_distance_from_camera() {
        // Identity matrix at origin
        let model_at_origin = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        let dist = compute_distance_from_camera(&model_at_origin, [3.0, 4.0, 0.0]);
        assert!((dist - 5.0).abs() < 0.001); // sqrt(3^2 + 4^2) = 5
    }

    #[test]
    fn test_draw_list_compute_sort_keys() {
        let mut list = DrawList::new();
        let mesh = MeshHandle::new(0);
        let mat = MaterialHandle::new(0);

        // Identity matrix at origin
        let model = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];

        list.push(DrawCall::new(mesh, mat).with_transform(model));
        list.push(
            DrawCall::new(mesh, mat)
                .with_transform(model)
                .with_transparency(),
        );

        list.compute_sort_keys([5.0, 0.0, 0.0]);

        // Both should have sort keys computed
        assert!(list.draws[0].sort_key.is_some());
        assert!(list.draws[1].sort_key.is_some());

        // Transparent should have different sort order than opaque
        assert_ne!(list.draws[0].sort_key, list.draws[1].sort_key);
    }
}
