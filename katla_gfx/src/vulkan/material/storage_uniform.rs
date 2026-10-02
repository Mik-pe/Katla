//! Per-slot storage for prepared draw instances.

use ash::vk;
use std::rc::Rc;

use super::super::context::VulkanContext;
use crate::RendererError;
use crate::vulkan::bda::DeviceAddressBuffer;

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
/// Defines the contiguous object-array layout.
/// All offsets are 16-byte aligned for proper access.
pub struct StorageUniformLayout;

impl StorageUniformLayout {
    /// The draw buffer starts with its object array.
    pub const OBJECT_ARRAY_OFFSET: usize = 0;

    /// Size per object (1 × mat4x4 + 3 × vec4 = 112 bytes).
    pub const OBJECT_STRIDE: usize = std::mem::size_of::<ObjectUniforms>();

    /// Maximum number of objects supported.
    pub const MAX_OBJECTS: usize = 256;

    /// Total buffer size for max objects.
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

/// Per-slot persistently mapped arrays indexed by the draw instance ID.
pub struct StorageUniformManager {
    /// Per-frame storage buffers (one for each frame in flight).
    buffers: Vec<DeviceAddressBuffer>,
}

impl StorageUniformManager {
    /// Create a new storage uniform manager with per-frame buffers.
    ///
    /// Creates `FRAMES_IN_FLIGHT` persistently mapped buffers to support
    /// double-buffered rendering. Buffer size is 28 KiB to support
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
    fn test_object_uniforms_size() {
        // 1 mat4x4 (64 bytes) + 3 vec4 (48 bytes) = 112 bytes
        assert_eq!(std::mem::size_of::<ObjectUniforms>(), 112);
    }

    #[test]
    fn test_layout_constants() {
        assert_eq!(StorageUniformLayout::OBJECT_ARRAY_OFFSET, 0);
        assert_eq!(StorageUniformLayout::OBJECT_STRIDE, 112);
        assert_eq!(StorageUniformLayout::MAX_OBJECTS, 256);
        assert_eq!(StorageUniformLayout::MAX_BUFFER_SIZE, 28672);
    }

    #[test]
    fn test_object_offset_calculation() {
        assert_eq!(StorageUniformLayout::object_offset(0), 0);
        assert_eq!(StorageUniformLayout::object_offset(1), 112);
        assert_eq!(StorageUniformLayout::object_offset(255), 28560);
    }
}
