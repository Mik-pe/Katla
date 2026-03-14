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
use crate::RendererError;
use crate::vulkan::bda::DeviceAddressBuffer;

/// Object data for bulk storage buffer updates.
///
/// Used by `update_objects_bulk` to efficiently write multiple objects at once.
/// Matches the layout of InstanceData but without exposing Vulkan types.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct ObjectData {
    /// Model matrix (object to world) - column-major [f32; 16]
    pub model_matrix: [f32; 16],
    /// Base color tint (RGBA)
    pub color: [f32; 4],
    /// PBR metallic factor (0.0 = dielectric, 1.0 = metal)
    pub metallic: f32,
    /// PBR roughness factor (0.0 = smooth, 1.0 = rough)
    pub roughness: f32,
    /// Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    pub ao: f32,
}

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
    /// Create a storage descriptor set from a storage uniform manager.
    ///
    /// Creates a descriptor set layout and descriptor set that binds the
    /// storage buffer to bindings 0 (frame_data) and 1 (objects array).
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `storage_buffer` - The storage buffer containing frame and object uniforms
    /// * `buffer_size` - Total size of the storage buffer
    ///
    /// # Returns
    /// A new StorageDescriptorSet, or an error if creation fails
    pub fn new(
        context: &Rc<VulkanContext>,
        storage_buffer: vk::Buffer,
        buffer_size: vk::DeviceSize,
    ) -> Result<Self, RendererError> {
        // Create descriptor set layout
        // Binding 0: frame_data (storage buffer, first 256 bytes)
        // Binding 1: objects array (storage buffer, starting at offset 256)
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        ];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

        let descriptor_layout = unsafe {
            context
                .device
                .create_descriptor_set_layout(&layout_info, None)?
        };

        // Create descriptor pool
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2), // 2 storage buffer bindings
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1)
            .flags(vk::DescriptorPoolCreateFlags::empty());

        let descriptor_pool = unsafe { context.device.create_descriptor_pool(&pool_info, None)? };

        // Allocate descriptor set
        let layouts = [descriptor_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { context.device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Write buffer descriptors
        // Binding 0: frame_data (offset 0, size 256)
        let frame_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer)
            .offset(0)
            .range(256)];

        let frame_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&frame_buffer_info);

        // Binding 1: objects array (offset 256, remaining buffer)
        let objects_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer)
            .offset(256)
            .range(buffer_size - 256)];

        let objects_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&objects_buffer_info);

        unsafe {
            context
                .device
                .update_descriptor_sets(&[frame_write, objects_write], &[]);
        }

        Ok(Self {
            inner: crate::vulkan::DescriptorSet::from_raw(
                descriptor_set,
                descriptor_pool,
                Some(descriptor_layout),
                context.device.clone(),
            ),
        })
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
#[allow(dead_code)] // NB: Dead code since we only use this for sizes
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
#[allow(dead_code)] // NB: Dead code since we only use this for sizes
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
    /// Get offset for object at given index.
    pub const fn object_offset(index: usize) -> usize {
        assert!(index < Self::MAX_OBJECTS, "Object index out of bounds");
        Self::OBJECT_ARRAY_OFFSET + (Self::OBJECT_STRIDE * index)
    }
}

/// Storage uniform manager.
///
/// Manages persistently mapped buffers containing frame uniforms
/// and arrays of per-object uniforms accessed via instance_index.
///
/// This manager holds `FRAMES_IN_FLIGHT` separate buffers to support
/// double-buffering. All methods require an explicit `frame_index` parameter
/// to select the target buffer.
///
/// # Frame Lifecycle
/// ```ignore
/// let frame_idx = renderer.current_frame();
/// storage_manager.update_frame(frame_idx, &view, &proj);
/// storage_manager.update_object_bindless(frame_idx, index, &model, &color, ...);
/// ```
pub struct StorageUniformManager {
    /// Per-frame storage buffers (one for each frame in flight).
    buffers: Vec<DeviceAddressBuffer>,
}

#[allow(dead_code)]
impl StorageUniformManager {
    /// Create a new storage uniform manager with per-frame buffers.
    ///
    /// Creates `FRAMES_IN_FLIGHT` persistently mapped buffers to support
    /// double-buffered rendering. Buffer size is ~28KB (28928 bytes) to support
    /// up to 256 objects.
    ///
    /// # Arguments
    /// * `context` - Vulkan context for buffer creation
    /// * `frames_in_flight` - Number of concurrent frames (typically 2 for double-buffering)
    ///
    /// # Returns
    /// A new StorageUniformManager, or an error if buffer creation fails
    ///
    /// # Errors
    /// Returns `RendererError::VulkanError` if allocation fails
    pub fn new(context: Rc<VulkanContext>, frames_in_flight: usize) -> Result<Self, RendererError> {
        let mut buffers = Vec::with_capacity(frames_in_flight);
        for _ in 0..frames_in_flight {
            buffers.push(DeviceAddressBuffer::new_persistent(
                context.clone(),
                StorageUniformLayout::MAX_BUFFER_SIZE as u64,
            )?);
        }

        Ok(Self { buffers })
    }

    /// Get the number of frames in flight.
    pub fn frames_in_flight(&self) -> usize {
        self.buffers.len()
    }

    /// Update frame uniforms (view, projection, and lighting).
    ///
    /// This writes the frame data to the start of the specified frame's buffer
    /// (offset 0, 256 bytes total). Should be called once per frame.
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip)
    pub fn update_frame(&mut self, frame_index: usize, view: &[f32; 16], proj: &[f32; 16]) {
        // Default inverse VP (identity - won't work correctly for sky)
        let default_inv_vp = [0.0f32; 16];

        // Use default lighting when only view/proj provided
        // Light direction points TO the light (upward for sun/sky)
        self.update_frame_with_lighting(
            frame_index,
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
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip) - column-major [f32; 16]
    /// * `inv_view_proj` - Inverse view-projection matrix (clip-to-world) - column-major [f32; 16]
    /// * `camera_position` - Camera position in world space
    /// * `light_direction` - Normalized direction TO the light
    /// * `light_color` - Light color (RGB)
    /// * `light_intensity` - Light intensity multiplier
    #[allow(clippy::too_many_arguments)]
    pub fn update_frame_with_lighting(
        &mut self,
        frame_index: usize,
        view: &[f32; 16],
        proj: &[f32; 16],
        inv_view_proj: &[f32; 16],
        camera_position: &[f32; 4],
        light_direction: &[f32; 4],
        light_color: &[f32; 4],
        light_intensity: f32,
    ) {
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
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
        // Flush frame uniforms to make CPU writes visible to GPU
        buffer.flush(0, std::mem::size_of::<FrameUniforms>() as u64);
    }

    /// Update frame uniforms from a FrameUniforms struct.
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `frame` - Frame uniforms struct containing all frame data
    pub fn update_from_frame_uniforms(
        &mut self,
        frame_index: usize,
        frame: &crate::renderer::FrameUniforms,
    ) {
        self.update_frame_with_lighting(
            frame_index,
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
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Color tint (RGBA)
    ///
    /// # Panics
    /// Panics if index >= 256
    pub fn update_object(
        &mut self,
        frame_index: usize,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
    ) {
        // Use default PBR material params
        self.update_object_with_material(frame_index, index, model, color, 0.0, 0.5, 1.0);
    }
    /// Update object uniforms with PBR material parameters.
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    #[allow(clippy::too_many_arguments)]
    pub fn update_object_with_material(
        &mut self,
        frame_index: usize,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) {
        // Default normal_scale to 1.0
        self.update_object_with_material_full(
            frame_index,
            index,
            model,
            color,
            metallic,
            roughness,
            ao,
            1.0,
        );
    }

    /// Update object uniforms with full PBR material parameters including normal scale.
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world) - column-major [f32; 16]
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    /// * `emission_idx` - Emission texture index for bindless (0 = no emission)
    #[allow(clippy::too_many_arguments)]
    pub fn update_object_with_material_full(
        &mut self,
        frame_index: usize,
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
            frame_index,
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
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix in column-major format (object-to-world)
    /// * `color` - Base color tint (RGBA)
    /// * `metallic` - Metallic factor (0.0 = dielectric, 1.0 = metal)
    /// * `roughness` - Roughness factor (0.0 = smooth, 1.0 = rough)
    /// * `ao` - Ambient occlusion factor (0.0 = full occlusion, 1.0 = none)
    /// * `emission_idx` - Emission texture index for bindless (0 = no emission)
    /// * `texture_indices` - [albedo_idx, normal_idx, mr_idx, ao_idx]
    #[allow(clippy::too_many_arguments)]
    pub fn update_object_bindless(
        &mut self,
        frame_index: usize,
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
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let object_ptr = (mapped.as_ptr() as usize + offset) as *mut ObjectUniforms;
            *object_ptr = ObjectUniforms {
                model: *model,
                base_color: *color,
                material_params: [metallic, roughness, ao, emission_idx],
                texture_indices,
            };
        }
        // Flush object data to make CPU writes visible to GPU
        buffer.flush(offset as u64, std::mem::size_of::<ObjectUniforms>() as u64);
    }

    /// Bulk update multiple objects at once for efficient instancing.
    ///
    /// This is more efficient than calling `update_object_bindless` multiple times
    /// because it maps the buffer only once and writes all data in a batch.
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    /// * `start_index` - First object index to update
    /// * `objects` - Slice of object data to write (must fit within MAX_OBJECTS)
    ///
    /// # Panics
    /// Panics if start_index + objects.len() >= MAX_OBJECTS
    pub fn update_objects_bulk(
        &mut self,
        frame_index: usize,
        start_index: usize,
        objects: &[ObjectData],
    ) {
        let end_index = start_index + objects.len();
        assert!(
            end_index <= StorageUniformLayout::MAX_OBJECTS,
            "Bulk update exceeds MAX_OBJECTS"
        );

        // Map buffer once and write all objects
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let base_ptr = mapped.as_ptr() as usize + StorageUniformLayout::OBJECT_ARRAY_OFFSET;

            for (i, obj) in objects.iter().enumerate() {
                let offset = i * StorageUniformLayout::OBJECT_STRIDE;
                let object_ptr = (base_ptr + offset) as *mut ObjectUniforms;
                *object_ptr = ObjectUniforms {
                    model: obj.model_matrix,
                    base_color: obj.color,
                    material_params: [obj.metallic, obj.roughness, obj.ao, 0.0],
                    texture_indices: [0, 0, 0, 0], // Default texture indices
                };
            }
        }
    }

    /// Get the maximum number of objects supported.
    #[inline]
    pub fn max_objects(&self) -> usize {
        StorageUniformLayout::MAX_OBJECTS
    }

    /// Check if buffers are persistently mapped.
    #[inline]
    #[allow(dead_code)]
    pub fn is_persistent(&self) -> bool {
        self.buffers.first().is_some_and(|b| b.is_persistent())
    }

    /// Get buffer handle for a specific frame (for descriptor set initialization).
    ///
    /// # Arguments
    /// * `frame_index` - Frame index (0 to frames_in_flight-1)
    #[inline]
    pub fn buffer(&self, frame_index: usize) -> vk::Buffer {
        self.buffers[frame_index].buffer
    }

    /// Get total buffer size in bytes (same for all frames).
    #[inline]
    pub fn buffer_size(&self) -> u64 {
        StorageUniformLayout::MAX_BUFFER_SIZE as u64
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
        assert_eq!(StorageUniformLayout::OBJECT_ARRAY_OFFSET, 256);
        assert_eq!(StorageUniformLayout::OBJECT_STRIDE, 112);
        assert_eq!(StorageUniformLayout::MAX_OBJECTS, 256);
        // 256 + (112 * 256) = 256 + 28672 = 28928
        assert_eq!(StorageUniformLayout::MAX_BUFFER_SIZE, 28928);
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
}
