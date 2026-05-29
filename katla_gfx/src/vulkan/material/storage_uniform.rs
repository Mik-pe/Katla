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
//! │ Storage Uniform Buffer (~29KB, persistent mapping)          │
//! ├─ [Frame Uniforms: 320 bytes (incl. 48-byte padding)]       │
//! │  ├─ view: mat4x4 (64 bytes)                                │
//! │  ├─ proj: mat4x4 (64 bytes)                                │
//! │  ├─ inv_view_proj: mat4x4 (64 bytes)                       │
//! │  ├─ camera_position: vec4 (16 bytes)                       │
//! │  ├─ light_direction: vec4 (16 bytes)                       │
//! │  ├─ light_color: vec4 (16 bytes)                           │
//! │  ├─ light_intensity: vec4 (16 bytes)                       │
//! │  └─ tiles: vec4<u32> (16 bytes)                            │
//! ├─ [Object Array: 112 bytes × 256 = 28,672 bytes]            │
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
//! storage_manager.update_frame_with_lighting(frame_idx, &view_matrix, &proj_matrix, ...);
//!
//! // Update object uniforms (per draw call)
//! storage_manager.update_from_frame_uniforms(frame_idx, &frame_uniforms);
//!
//! // Bind in render loop - object index comes from first_instance in draw call
//! pipeline.bind_with_storage(command_buffer, storage_descriptor.set());
//! ```

use ash::vk;
use std::rc::Rc;

use super::super::context::VulkanContext;
use crate::RendererError;
use crate::vulkan::bda::DeviceAddressBuffer;

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
        // Binding 0: frame_data (storage buffer, first OBJECT_ARRAY_OFFSET bytes)
        // Binding 1: objects array (storage buffer, starting at offset OBJECT_ARRAY_OFFSET)
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
        // Binding 0: frame_data (offset 0, size OBJECT_ARRAY_OFFSET)
        let frame_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer)
            .offset(0)
            .range(StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64)];

        let frame_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .buffer_info(&frame_buffer_info);

        // Binding 1: objects array (offset OBJECT_ARRAY_OFFSET, remaining buffer)
        let objects_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer)
            .offset(StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64)
            .range(buffer_size - StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64)];

        let objects_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
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

    /// Get the descriptor set layout.
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.inner
            .layout()
            .expect("StorageDescriptorSet always stores its layout")
    }
}

/// Frame-level uniforms (view and projection matrices + lighting).
///
/// Shared across all objects in the buffer.
/// Total: 320 bytes (3 × mat4x4 + 4 × vec4 + 1 × vec4<u32> + 48 bytes padding).
/// Padded to 320 to ensure OBJECT_ARRAY_OFFSET is a multiple of 64
/// (minStorageBufferOffsetAlignment).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct FrameUniforms {
    /// View matrix (world-to-camera transform) - column-major.
    view: [f32; 16],

    /// Projection matrix (camera-to-clip transform) - column-major.
    proj: [f32; 16],

    /// Inverse view-projection matrix (clip-to-world transform) - column-major.
    /// Used for sky rendering to convert screen coords to world rays.
    inv_view_proj: [f32; 16],

    /// Camera position in world space (for specular calculations).
    camera_position: [f32; 4], // vec4 for alignment

    /// Light direction (normalized, points TO the light).
    light_direction: [f32; 4], // vec4 for alignment

    /// Light color (RGB).
    light_color: [f32; 4], // vec4 for alignment

    /// Light intensity.
    light_intensity: [f32; 4], // single f32 + padding

    /// Forward+ tile grid dimensions: [tiles_x, tiles_y, 0, 0].
    tiles: [u32; 4],

    /// Tonemap parameters: [exposure, gamma, mode, hdr_texture_index].
    tonemap: [f32; 4],

    /// Overlay parameters: [ldr_texture_index, stencil_indicator_index, 0, 0].
    overlay: [f32; 4],

    /// Compositing parameters: [screen_width, screen_height, viewport_count, viewport_bindless_index].
    compositing: [f32; 4],
}

/// Per-object uniforms (model matrix, color, PBR params, and bindless texture indices).
///
/// Total: 112 bytes (1 × mat4x4 + 3 × vec4).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct ObjectUniforms {
    /// Model matrix (object-to-world transform) - column-major.
    model: [f32; 16],

    /// Base color tint for the object (RGBA).
    base_color: [f32; 4],

    /// PBR material parameters.
    /// x = metallic, y = roughness, z = ambient occlusion, w = emission texture index
    material_params: [f32; 4],

    /// Bindless texture indices (stored as u32, interpreted as vec4<u32> in WGSL).
    /// x = albedo index, y = normal index, z = metallic/roughness index, w = ao index
    texture_indices: [u32; 4],
}

/// Storage uniform buffer layout constants.
///
/// Defines memory layout for frame and object uniforms.
/// All offsets are 16-byte aligned for proper access.
pub struct StorageUniformLayout;

impl StorageUniformLayout {
    /// Object array starts after frame uniforms (offset 320, padded for 64-byte alignment).
    pub const OBJECT_ARRAY_OFFSET: usize = std::mem::size_of::<FrameUniforms>(); // 320

    /// Size per object (1 × mat4x4 + 3 × vec4 = 112 bytes).
    pub const OBJECT_STRIDE: usize = std::mem::size_of::<ObjectUniforms>();

    /// Maximum number of objects supported.
    pub const MAX_OBJECTS: usize = 256;

    /// Total buffer size for max objects.
    /// 320 + (112 * 256) = 320 + 28672 = 28992 bytes (~28 KB)
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

/// Parameters for updating frame lighting and post-processing uniforms.
pub struct FrameLightingParams<'a> {
    pub view: &'a [f32; 16],
    pub proj: &'a [f32; 16],
    pub inv_view_proj: &'a [f32; 16],
    pub camera_position: &'a [f32; 4],
    pub light_direction: &'a [f32; 4],
    pub light_color: &'a [f32; 4],
    pub light_intensity: [f32; 4],
    pub tiles: [u32; 4],
    pub tonemap: [f32; 4],
    pub overlay: [f32; 4],
}

/// Parameters for updating per-object bindless uniforms.
pub struct ObjectBindlessParams<'a> {
    pub model: &'a [f32; 16],
    pub color: &'a [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub ao: f32,
    pub emission_idx: f32,
    pub texture_indices: [u32; 4],
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
    pub fn new(
        context: &Rc<VulkanContext>,
        frames_in_flight: usize,
    ) -> Result<Self, RendererError> {
        let mut buffers = Vec::with_capacity(frames_in_flight);
        for _ in 0..frames_in_flight {
            buffers.push(DeviceAddressBuffer::new_persistent(
                context.clone(),
                StorageUniformLayout::MAX_BUFFER_SIZE as u64,
            )?);
        }

        Ok(Self { buffers })
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
    /// * `light_intensity` - Light intensity and screen-space params [intensity, depth_tex_idx, 0, 0]
    /// * `tiles` - Forward+ tile grid dimensions [tiles_x, tiles_y, 0, 0]
    ///
    /// Update frame uniforms including post-processing parameters.
    pub fn update_frame_with_lighting_and_postprocess(
        &mut self,
        frame_index: usize,
        params: &FrameLightingParams,
    ) {
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let frame_ptr = mapped.as_ptr() as *mut FrameUniforms;
            *frame_ptr = FrameUniforms {
                view: *params.view,
                proj: *params.proj,
                inv_view_proj: *params.inv_view_proj,
                camera_position: *params.camera_position,
                light_direction: *params.light_direction,
                light_color: *params.light_color,
                light_intensity: params.light_intensity,
                tiles: params.tiles,
                tonemap: params.tonemap,
                overlay: params.overlay,
                compositing: [0.0; 4],
            };
        }
        buffer.flush(0, std::mem::size_of::<FrameUniforms>() as u64);
    }

    /// Update only the tonemap parameters in the frame uniform buffer.
    ///
    /// This avoids rewriting the entire frame uniform block when only
    /// tonemap params change (e.g., per-frame transient texture indices).
    pub fn update_tonemap_params(&mut self, frame_index: usize, tonemap: [f32; 4]) {
        let offset = std::mem::size_of::<FrameUniforms>() - std::mem::size_of::<[f32; 4]>() * 3; // skip overlay + compositing
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let ptr = (mapped.as_ptr() as usize + offset) as *mut [f32; 4];
            *ptr = tonemap;
        }
        buffer.flush(offset as u64, std::mem::size_of::<[f32; 4]>() as u64);
    }

    /// Update only the overlay parameters in the frame uniform buffer.
    pub fn update_overlay_params(&mut self, frame_index: usize, overlay: [f32; 4]) {
        let offset = std::mem::size_of::<FrameUniforms>() - std::mem::size_of::<[f32; 4]>() * 2; // skip compositing
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let ptr = (mapped.as_ptr() as usize + offset) as *mut [f32; 4];
            *ptr = overlay;
        }
        buffer.flush(offset as u64, std::mem::size_of::<[f32; 4]>() as u64);
    }

    /// Update only the compositing parameters in the frame uniform buffer.
    pub fn update_compositing_params(&mut self, frame_index: usize, compositing: [f32; 4]) {
        let offset = std::mem::size_of::<FrameUniforms>() - std::mem::size_of::<[f32; 4]>(); // last field
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let ptr = (mapped.as_ptr() as usize + offset) as *mut [f32; 4];
            *ptr = compositing;
        }
        buffer.flush(offset as u64, std::mem::size_of::<[f32; 4]>() as u64);
    }

    /// Read back the tonemap parameters (debug only).
    #[cfg(debug_assertions)]
    pub fn read_tonemap_params(&mut self, frame_index: usize) -> [f32; 4] {
        let offset = std::mem::size_of::<FrameUniforms>() - std::mem::size_of::<[f32; 4]>() * 3;
        let buffer = &mut self.buffers[frame_index];
        unsafe {
            let mapped = buffer.map();
            let ptr = (mapped.as_ptr() as usize + offset) as *const [f32; 4];
            *ptr
        }
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
        self.update_frame_with_lighting_and_postprocess(
            frame_index,
            &FrameLightingParams {
                view: &frame.view_matrix,
                proj: &frame.proj_matrix,
                inv_view_proj: &frame.inv_view_proj_matrix,
                camera_position: &frame.camera_position,
                light_direction: &frame.light_direction,
                light_color: &frame.light_color,
                light_intensity: frame.light_intensity,
                tiles: frame.tiles,
                tonemap: frame.tonemap,
                overlay: frame.overlay,
            },
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
    pub fn update_object_bindless(
        &mut self,
        frame_index: usize,
        index: usize,
        params: &ObjectBindlessParams,
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
                model: *params.model,
                base_color: *params.color,
                material_params: [
                    params.metallic,
                    params.roughness,
                    params.ao,
                    params.emission_idx,
                ],
                texture_indices: params.texture_indices,
            };
        }
        // Flush object data to make CPU writes visible to GPU
        buffer.flush(offset as u64, std::mem::size_of::<ObjectUniforms>() as u64);
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
        // 3 mat4x4 (192) + 4 vec4 (64) + 1 vec4<u32> (16) + 3 vec4 tonemap/overlay/compositing (48) = 320 bytes
        assert_eq!(std::mem::size_of::<FrameUniforms>(), 320);

        // Verify field offsets match WGSL std140 layout
        unsafe {
            let base = std::ptr::NonNull::<FrameUniforms>::dangling().as_ptr();
            assert_eq!(
                (*base).tiles.as_ptr() as usize - base as usize,
                256,
                "tiles offset mismatch"
            );
            assert_eq!(
                (*base).tonemap.as_ptr() as usize - base as usize,
                272,
                "tonemap offset mismatch"
            );
            assert_eq!(
                (*base).overlay.as_ptr() as usize - base as usize,
                288,
                "overlay offset mismatch"
            );
            assert_eq!(
                (*base).compositing.as_ptr() as usize - base as usize,
                304,
                "compositing offset mismatch"
            );
        }
    }

    #[test]
    fn test_object_uniforms_size() {
        // 1 mat4x4 (64 bytes) + 3 vec4 (48 bytes) = 112 bytes
        assert_eq!(std::mem::size_of::<ObjectUniforms>(), 112);
    }

    #[test]
    fn test_layout_constants() {
        assert_eq!(StorageUniformLayout::OBJECT_ARRAY_OFFSET, 320);
        assert_eq!(StorageUniformLayout::OBJECT_STRIDE, 112);
        assert_eq!(StorageUniformLayout::MAX_OBJECTS, 256);
        // 320 + (112 * 256) = 320 + 28672 = 28992
        assert_eq!(StorageUniformLayout::MAX_BUFFER_SIZE, 28992);
    }

    #[test]
    fn test_object_offset_calculation() {
        // Object 0: offset 320
        assert_eq!(StorageUniformLayout::object_offset(0), 320);
        // Object 1: offset 320 + 112 = 432
        assert_eq!(StorageUniformLayout::object_offset(1), 432);
        // Object 255: offset 320 + (112 * 255) = 320 + 28560 = 28880
        assert_eq!(StorageUniformLayout::object_offset(255), 28880);
    }
}
