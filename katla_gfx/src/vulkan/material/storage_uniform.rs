//! Storage buffer uniform manager with instance indexing.
//!
//! This module provides storage buffer-based uniform management,
//! replacing per-object descriptor updates with a single shared buffer
//! accessed via `@builtin(instance_index)` in shaders.
//!
//! # Benefits
//! - **Single ~24KB buffer** for up to 256 objects (vs 256 separate uniform buffers)
//! - **Persistent mapping** for CPU-side updates without repeated map/unmap
//! - **Storage buffer access** instead of descriptor-based uniforms
//! - **Instance index** for per-object data access (no push constants needed)
//! - **Type-safe offset calculation** via StorageUniformLayout
//!
//! # Architecture
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Storage Uniform Buffer (~24KB, persistent mapping)          │
//! ├─ [Frame Uniforms: 192 bytes]                               │
//! │  ├─ view: mat4x4 (64 bytes)                                │
//! │  ├─ proj: mat4x4 (64 bytes)                                │
//! │  ├─ camera_position: vec4 (16 bytes)                       │
//! │  ├─ light_direction: vec4 (16 bytes)                       │
//! │  ├─ light_color: vec4 (16 bytes)                           │
//! │  └─ light_intensity: vec4 (16 bytes)                       │
//! ├─ [Object Array: 96 bytes × 256 = 24,576 bytes]             │
//! │    ├─ Object[0]: model (64) + color (16) + material (16)   │
//! │    ├─ Object[1]: model (64) + color (16) + material (16)   │
//! │    ├─ ...                                                   │
//! │    └─ Object[255]: model (64) + color (16) + material (16) │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//! ```ignore
//! use katla_gfx::vulkan::material::storage_uniform::{StorageUniformManager, StorageUniformLayout, StorageDescriptorSet};
//! use katla_gfx::VulkanContext;
//! use std::rc::Rc;
//!
//! // Create manager with ~24KB buffer (supports 256 objects)
//! let mut storage_manager = StorageUniformManager::new(context.clone())?;
//!
//! // Create descriptor set for binding to shaders
//! let storage_descriptor = storage_manager.create_descriptor_set(&context, desc_layout)?;
//!
//! // Update frame uniforms (once per frame)
//! storage_manager.update_frame(&view_matrix, &proj_matrix);
//!
//! // Update object uniforms (per draw call)
//! let object_index = 0;
//! storage_manager.update_object(object_index, &model_matrix, &[1.0, 0.0, 0.0, 1.0]);
//!
//! // Or with PBR material params
//! storage_manager.update_object_with_material(index, &model, &color, 0.5, 0.3, 1.0);
//!
//! // Bind in render loop - object index comes from first_instance in draw call
//! pipeline.bind_with_storage(command_buffer, storage_descriptor.set());
//! ```

use ash::vk;
use std::rc::Rc;

use super::super::context::VulkanContext;
use crate::vulkan::{bda::DeviceAddressBuffer, DescriptorSetBuilder};
use crate::RendererError;

/// Storage buffer descriptor set for uniform buffers.
///
/// Contains descriptor set and pool for binding the storage buffer
/// to shaders as storage buffers (set 0).
///
/// This is a convenience wrapper around [`DescriptorSetBuilder`]
/// for the common storage uniform pattern with frame_data and objects.
pub struct StorageDescriptorSet {
    inner: crate::vulkan::DescriptorSet,
}

impl StorageDescriptorSet {
    /// Create a new storage descriptor set from the uniform manager's buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `storage_buffer` - The storage buffer to create descriptors for
    /// * `desc_layout` - Descriptor set layout for uniform set (set 0, wrapper type)
    ///
    /// # Returns
    /// A new StorageDescriptorSet with storage buffer bindings
    pub(crate) fn new(
        context: &Rc<VulkanContext>,
        storage_buffer: &DeviceAddressBuffer,
        desc_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<Self, RendererError> {
        // Use the unified builder with two bindings to the same buffer
        let inner = DescriptorSetBuilder::new(context)
            // Binding 0: frame_data (offset 0, size = FrameUniforms)
            .storage_buffer_range(
                0,
                storage_buffer,
                0,
                StorageUniformLayout::FRAME_SIZE as u64,
            )
            // Binding 1: objects array (offset 256, size = ObjectUniforms * MAX_OBJECTS)
            .storage_buffer_range(
                1,
                storage_buffer,
                StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64,
                (StorageUniformLayout::OBJECT_STRIDE * StorageUniformLayout::MAX_OBJECTS) as u64,
            )
            .build(desc_layout)?;

        Ok(Self { inner })
    }

    /// Get the raw Vulkan descriptor set handle.
    pub fn vk_set(&self) -> vk::DescriptorSet {
        self.inner.vk()
    }
}

/// Frame-level uniforms (view and projection matrices + lighting).
///
/// Shared across all objects in the buffer.
/// Total: 320 bytes (3 × mat4x4 + 4 × vec4).
#[derive(Debug, Clone, Copy)]
pub struct FrameUniforms {
    /// View matrix (world-to-camera transform) - column-major.
    pub view: [f32; 16],

    /// Projection matrix (camera-to-clip transform) - column-major.
    pub proj: [f32; 16],

    /// Inverse view-projection matrix (clip-to-world transform) - column-major.
    /// Used for sky rendering to convert screen coords to world rays.
    pub inv_view_proj: [f32; 16],

    /// Camera position in world space (for specular calculations).
    pub camera_position: [f32; 4], // vec4 for alignment

    /// Light direction (normalized, points TO the light).
    pub light_direction: [f32; 4], // vec4 for alignment

    /// Light color (RGB).
    pub light_color: [f32; 4], // vec4 for alignment

    /// Light intensity.
    pub light_intensity: [f32; 4], // single f32 + padding
}

/// Per-object uniforms (model matrix, color, PBR params, and bindless texture indices).
///
/// Total: 112 bytes (1 × mat4x4 + 3 × vec4).
#[derive(Debug, Clone, Copy)]
pub struct ObjectUniforms {
    /// Model matrix (object-to-world transform) - column-major.
    pub model: [f32; 16],

    /// Base color tint for the object (RGBA).
    pub base_color: [f32; 4],

    /// PBR material parameters.
    /// x = metallic, y = roughness, z = ambient occlusion, w = emission texture index
    pub material_params: [f32; 4],

    /// Bindless texture indices (stored as u32, interpreted as vec4<u32> in WGSL).
    /// x = albedo index, y = normal index, z = metallic/roughness index, w = ao index
    pub texture_indices: [u32; 4],
}

/// Storage uniform buffer layout constants.
///
/// Defines memory layout for frame and object uniforms.
/// All offsets are 16-byte aligned for proper access.
pub struct StorageUniformLayout;

impl StorageUniformLayout {
    /// Frame uniforms start at offset 0.
    pub const FRAME_OFFSET: usize = 0;

    /// Size of frame uniforms (3 × mat4x4 + 4 × vec4 = 256 bytes).
    pub const FRAME_SIZE: usize = std::mem::size_of::<FrameUniforms>();

    /// Object array starts after frame uniforms (offset 256).
    pub const OBJECT_ARRAY_OFFSET: usize = 256;

    /// Size per object (1 × mat4x4 + 3 × vec4 = 112 bytes).
    pub const OBJECT_STRIDE: usize = std::mem::size_of::<ObjectUniforms>();

    /// Maximum number of objects supported.
    pub const MAX_OBJECTS: usize = 256;

    /// Total buffer size for max objects.
    /// 256 + (112 * 256) = 256 + 28672 = 28928 bytes (~28 KB)
    pub const MAX_BUFFER_SIZE: usize =
        Self::OBJECT_ARRAY_OFFSET + (Self::OBJECT_STRIDE * Self::MAX_OBJECTS);
}

impl StorageUniformLayout {
    /// Get total buffer size for maximum number of objects.
    pub const fn total_size() -> usize {
        Self::MAX_BUFFER_SIZE
    }

    /// Get offset for object at given index.
    pub const fn object_offset(index: usize) -> usize {
        assert!(index < Self::MAX_OBJECTS, "Object index out of bounds");
        Self::OBJECT_ARRAY_OFFSET + (Self::OBJECT_STRIDE * index)
    }

    /// Get number of 16-byte aligned slots in object array.
    pub const fn aligned_slots() -> usize {
        // 80 bytes / 16 bytes = 5 slots per object
        Self::OBJECT_STRIDE / 16
    }
}

/// Storage uniform manager.
///
/// Manages a persistently mapped buffer containing frame uniforms
/// and an array of per-object uniforms accessed via instance_index.
pub struct StorageUniformManager {
    /// Persistent storage buffer.
    buffer: DeviceAddressBuffer,
}

impl StorageUniformManager {
    /// Create a new storage uniform manager.
    ///
    /// Creates a persistently mapped buffer.
    /// Buffer size is ~20KB (20608 bytes) to support up to 256 objects.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for buffer creation
    ///
    /// # Returns
    /// A new StorageUniformManager, or an error if buffer creation fails
    ///
    /// # Errors
    /// Returns `RendererError::VulkanError` if allocation fails
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, RendererError> {
        let buffer = DeviceAddressBuffer::new_persistent(
            context,
            StorageUniformLayout::MAX_BUFFER_SIZE as u64,
        )?;

        Ok(Self { buffer })
    }

    /// Update frame uniforms (view, projection, and lighting).
    ///
    /// This writes the frame data to the start of the buffer
    /// (offset 0, 256 bytes total). Should be called once per frame.
    ///
    /// # Arguments
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip)
    ///
    /// # Note
    /// This computes a default inverse view-projection matrix. For accurate
    /// sky rendering, use `update_frame_with_lighting()` with the correct
    /// inverse VP matrix.
    pub fn update_frame(&mut self, view: &[f32; 16], proj: &[f32; 16]) {
        // Default inverse VP (identity - won't work correctly for sky)
        let default_inv_vp = [0.0f32; 16];

        // Use default lighting when only view/proj provided
        // Light direction points TO the light (upward for sun/sky)
        self.update_frame_with_lighting(
            view,
            proj,
            &default_inv_vp,
            &[0.0, 0.0, 0.0, 0.0], // camera_position (will be computed from view inverse)
            &[0.3, 1.0, 0.2, 0.0], // light_direction (upward toward sun)
            &[1.0, 0.98, 0.95, 0.0], // light_color (slightly warm white)
            3.0,                   // light_intensity (HDR - brighter for PBR)
        );
    }

    /// Update frame uniforms with full lighting parameters.
    ///
    /// # Arguments
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip) - column-major [f32; 16]
    /// * `inv_view_proj` - Inverse view-projection matrix (clip-to-world) - column-major [f32; 16]
    /// * `camera_position` - Camera position in world space
    /// * `light_direction` - Normalized direction TO the light
    /// * `light_color` - Light color (RGB)
    /// * `light_intensity` - Light intensity multiplier
    pub fn update_frame_with_lighting(
        &mut self,
        view: &[f32; 16],
        proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_position: &[f32; 4],
        light_direction: &[f32; 4],
        light_color: &[f32; 4],
        light_intensity: f32,
    ) {
        unsafe {
            let mapped = self.buffer.map();
            let frame_ptr = mapped.as_ptr() as *mut FrameUniforms;
            *frame_ptr = FrameUniforms {
                view: *view,
                proj: *proj,
                inv_view_proj: *inv_view_proj,
                camera_position: *camera_position,
                light_direction: *light_direction,
                light_color: *light_color,
                light_intensity: [light_intensity, 0.0, 0.0, 0.0],
            };
        }
    }

    /// Update frame uniforms from a FrameUniforms struct.
    ///
    /// # Arguments
    /// * `frame` - Frame uniforms from the rendering module
    pub fn update_from_frame_uniforms(&mut self, frame: &crate::renderer::FrameUniforms) {
        self.update_frame_with_lighting(
            &frame.view_matrix,
            &frame.proj_matrix,
            &frame.inv_view_proj_matrix,
            &frame.camera_position,
            &frame.light_direction,
            &frame.light_color,
            frame.light_intensity,
        );
    }

    /// Update object uniforms at specific index.
    ///
    /// Writes per-object data (model matrix + color) to the object array.
    /// Automatically handles object index lookup and offset calculation.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Color tint (RGBA)
    ///
    /// # Panics
    /// Panics if index >= 256
    pub fn update_object(&mut self, index: usize, model: &[f32; 16], color: &[f32; 4]) {
        // Use default PBR material params
        self.update_object_with_material(index, model, color, 0.0, 0.5, 1.0);
    }

    /// Update object uniforms with PBR material parameters.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    pub fn update_object_with_material(
        &mut self,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) {
        // Default normal_scale to 1.0
        self.update_object_with_material_full(index, model, color, metallic, roughness, ao, 1.0);
    }

    /// Update object uniforms with full PBR material parameters including normal scale.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    /// * `emission_idx` - Emission texture index for bindless (0 = no emission)
    pub fn update_object_with_material_full(
        &mut self,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
        emission_idx: f32,
    ) {
        // Default texture indices (0 = no texture / use default)
        self.update_object_bindless(
            index,
            model,
            color,
            metallic,
            roughness,
            ao,
            emission_idx,
            [0, 0, 0, 0], // albedo, normal, mr, ao indices
        );
    }

    /// Update object uniforms with bindless texture indices.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix in column-major format (object-to-world)
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    /// * `emission_idx` - Emission texture index for bindless (0 = no emission)
    /// * `texture_indices` - [albedo_idx, normal_idx, mr_idx, ao_idx]
    pub fn update_object_bindless(
        &mut self,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
        emission_idx: f32,
        texture_indices: [u32; 4],
    ) {
        assert!(
            index < StorageUniformLayout::MAX_OBJECTS,
            "Object index out of bounds"
        );

        // Calculate offset for this object
        let offset = StorageUniformLayout::object_offset(index);

        // Map and write object uniforms at calculated offset
        unsafe {
            let mapped = self.buffer.map();
            let object_ptr = (mapped.as_ptr() as usize + offset) as *mut ObjectUniforms;
            *object_ptr = ObjectUniforms {
                model: *model,
                base_color: *color,
                material_params: [metallic, roughness, ao, emission_idx],
                texture_indices,
            };
        }
    }

    /// Get the maximum number of objects supported.
    #[inline]
    pub fn max_objects(&self) -> usize {
        StorageUniformLayout::MAX_OBJECTS
    }

    /// Get total buffer size in bytes.
    #[inline]
    pub fn buffer_size(&self) -> u64 {
        self.buffer.size()
    }

    /// Check if buffer is persistently mapped.
    #[inline]
    pub fn is_persistent(&self) -> bool {
        self.buffer.is_persistent()
    }

    /// Get the underlying buffer handle for descriptor creation.
    #[inline]
    pub fn buffer(&self) -> vk::Buffer {
        self.buffer.buffer
    }

    /// Create a descriptor set for binding this storage buffer to shaders.
    ///
    /// Creates a descriptor set with:
    /// - Binding 0: frame_data (storage buffer, offset 0, size 128)
    /// - Binding 1: objects array (storage buffer, offset 128)
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `desc_layout` - Descriptor set layout for uniform set (set 0, wrapper type)
    ///
    /// # Returns
    /// A StorageDescriptorSet that can be bound to a pipeline
    pub(crate) fn create_descriptor_set(
        &self,
        context: &Rc<VulkanContext>,
        desc_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<StorageDescriptorSet, RendererError> {
        StorageDescriptorSet::new(context, &self.buffer, desc_layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_uniforms_size() {
        // 3 mat4x4 (192 bytes) + 4 vec4 (64 bytes) = 256 bytes
        assert_eq!(std::mem::size_of::<FrameUniforms>(), 256);
    }

    #[test]
    fn test_object_uniforms_size() {
        // 1 mat4x4 (64 bytes) + 3 vec4 (48 bytes) = 112 bytes
        assert_eq!(std::mem::size_of::<ObjectUniforms>(), 112);
    }

    #[test]
    fn test_layout_constants() {
        assert_eq!(StorageUniformLayout::FRAME_OFFSET, 0);
        assert_eq!(StorageUniformLayout::FRAME_SIZE, 256);
        assert_eq!(StorageUniformLayout::OBJECT_ARRAY_OFFSET, 256);
        assert_eq!(StorageUniformLayout::OBJECT_STRIDE, 112);
        assert_eq!(StorageUniformLayout::MAX_OBJECTS, 256);
        // 256 + (112 * 256) = 256 + 28672 = 28928
        assert_eq!(StorageUniformLayout::MAX_BUFFER_SIZE, 28928);
    }

    #[test]
    fn test_aligned_object_slots() {
        // 112 bytes / 16 = 7 slots
        assert_eq!(StorageUniformLayout::aligned_slots(), 7);
    }

    #[test]
    fn test_object_offset_calculation() {
        // Object 0: offset 256
        assert_eq!(StorageUniformLayout::object_offset(0), 256);
        // Object 1: offset 256 + 112 = 368
        assert_eq!(StorageUniformLayout::object_offset(1), 368);
        // Object 255: offset 256 + (112 * 255) = 256 + 28560 = 28816
        assert_eq!(StorageUniformLayout::object_offset(255), 28816);
    }

    #[test]
    fn test_max_buffer_size() {
        // Frame (256) + objects (112 * 256) = 28928
        assert_eq!(StorageUniformLayout::total_size(), 28928);
    }
}
