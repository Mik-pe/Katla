//! Storage buffer uniform manager with instance indexing.
//!
//! This module provides storage buffer-based uniform management,
//! replacing per-object descriptor updates with a single shared buffer
//! accessed via `@builtin(instance_index)` in shaders.
//!
//! # Benefits
//! - **Single 20KB buffer** for up to 256 objects (vs 256 separate uniform buffers)
//! - **Persistent mapping** for CPU-side updates without repeated map/unmap
//! - **Storage buffer access** instead of descriptor-based uniforms
//! - **Instance index** for per-object data access (no push constants needed)
//! - **Type-safe offset calculation** via StorageUniformLayout
//!
//! # Architecture
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Storage Uniform Buffer (20KB, persistent mapping)           │
//! ├─ [Frame Uniforms: 128 bytes]                               │
//! │  └─ view: mat4x4 (64 bytes)                                │
//! │  └─ proj: mat4x4 (64 bytes)                                │
//! ├─ [Object Array: 80 bytes × 256 = 20,480 bytes]             │
//! │    ├─ Object[0]: model (64) + color (16) = 80              │
//! │    ├─ Object[1]: model (64) + color (16) = 80              │
//! │    ├─ ...                                                   │
//! │    └─ Object[255]: model (64) + color (16) = 80            │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//! ```ignore
//! use katla_vulkan::vulkan::material::storage_uniform::{StorageUniformManager, StorageUniformLayout, StorageDescriptorSet};
//! use katla_vulkan::VulkanContext;
//! use std::rc::Rc;
//!
//! // Create manager with ~20KB buffer (supports 256 objects)
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
//! // Bind in render loop - object index comes from first_instance in draw call
//! pipeline.bind_with_storage(command_buffer, storage_descriptor.set());
//! ```

use ash::vk;
use std::rc::Rc;

use crate::vulkan::{bda::DeviceAddressBuffer, VulkanContext};

/// Storage buffer descriptor set for uniform buffers.
///
/// Contains descriptor set and pool for binding the storage buffer
/// to shaders as storage buffers (set 0).
pub struct StorageDescriptorSet {
    /// Descriptor set containing frame_data (binding 0) and objects (binding 1).
    pub descriptor_set: vk::DescriptorSet,
    /// Descriptor pool (owned, for cleanup).
    descriptor_pool: vk::DescriptorPool,
    /// Device for cleanup.
    device: ash::Device,
}

impl StorageDescriptorSet {
    /// Create a new storage descriptor set from the uniform manager's buffer.
    ///
    /// # Arguments
    /// * `context` - Vulkan context
    /// * `storage_buffer` - The storage buffer to create descriptors for
    /// * `desc_layout` - Descriptor set layout for uniform set (set 0)
    ///
    /// # Returns
    /// A new StorageDescriptorSet with storage buffer bindings
    pub fn new(
        context: &Rc<VulkanContext>,
        storage_buffer: &DeviceAddressBuffer,
        desc_layout: vk::DescriptorSetLayout,
    ) -> Result<Self, vk::Result> {
        // Create descriptor pool for storage buffers
        let pool_sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(2), // frame_data + objects
        ];

        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&pool_sizes)
            .max_sets(1);

        let descriptor_pool = unsafe {
            context
                .device
                .create_descriptor_pool(&pool_info, None)?
        };

        // Allocate descriptor set
        let layouts = [desc_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&layouts);

        let descriptor_sets = unsafe { context.device.allocate_descriptor_sets(&alloc_info)? };
        let descriptor_set = descriptor_sets[0];

        // Create buffer info for frame_data (binding 0)
        // Offset 0, size 128 bytes (FrameUniforms)
        let frame_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer.buffer)
            .offset(0)
            .range(StorageUniformLayout::FRAME_SIZE as u64);

        // Create buffer info for objects array (binding 1)
        // Offset 128, size 80*256 bytes
        let objects_buffer_info = vk::DescriptorBufferInfo::default()
            .buffer(storage_buffer.buffer)
            .offset(StorageUniformLayout::OBJECT_ARRAY_OFFSET as u64)
            .range((StorageUniformLayout::OBJECT_STRIDE * StorageUniformLayout::MAX_OBJECTS) as u64);

        // Write descriptors
        let descriptor_writes = [
            // Binding 0: frame_data
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&frame_buffer_info)),
            // Binding 1: objects array
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(std::slice::from_ref(&objects_buffer_info)),
        ];

        unsafe {
            context
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        Ok(Self {
            descriptor_set,
            descriptor_pool,
            device: context.device.clone(),
        })
    }

    /// Get the descriptor set for binding.
    pub fn set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }
}

impl Drop for StorageDescriptorSet {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

/// Frame-level uniforms (view and projection matrices).
///
/// Shared across all objects in the buffer.
/// Total: 128 bytes (2 × mat4x4).
#[derive(Debug, Clone, Copy)]
pub struct FrameUniforms {
    /// View matrix (world-to-camera transform).
    pub view: [[f32; 4]; 4],

    /// Projection matrix (camera-to-clip transform).
    pub proj: [[f32; 4]; 4],
}

/// Per-object uniforms (model matrix and color).
///
/// Total: 80 bytes (1 × mat4x4 + 1 × vec4).
#[derive(Debug, Clone, Copy)]
pub struct ObjectUniforms {
    /// Model matrix (object-to-world transform).
    pub model: [[f32; 4]; 4],

    /// Color tint for the object.
    pub color: [f32; 4],
}

/// Storage uniform buffer layout constants.
///
/// Defines memory layout for frame and object uniforms.
/// All offsets are 16-byte aligned for proper access.
pub struct StorageUniformLayout;

impl StorageUniformLayout {
    /// Frame uniforms start at offset 0.
    pub const FRAME_OFFSET: usize = 0;

    /// Size of frame uniforms (2 × mat4x4 = 128 bytes).
    pub const FRAME_SIZE: usize = std::mem::size_of::<FrameUniforms>();

    /// Object array starts after frame uniforms (offset 128).
    pub const OBJECT_ARRAY_OFFSET: usize = 128;

    /// Size per object (1 × mat4x4 + 1 × vec4 = 80 bytes).
    pub const OBJECT_STRIDE: usize = std::mem::size_of::<ObjectUniforms>();

    /// Maximum number of objects supported.
    pub const MAX_OBJECTS: usize = 256;

    /// Total buffer size for max objects.
    /// 128 + (80 * 256) = 128 + 20480 = 20608 bytes (~20 KB)
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
    /// Returns `vk::Result::ERROR_OUT_OF_DEVICE_MEMORY` if allocation fails
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        let buffer = DeviceAddressBuffer::new_persistent(
            context,
            StorageUniformLayout::MAX_BUFFER_SIZE as u64,
        )?;

        Ok(Self { buffer })
    }

    /// Update frame uniforms (view and projection matrices).
    ///
    /// This writes the frame data to the start of the buffer
    /// (offset 0, 128 bytes total). Should be called once per frame.
    ///
    /// # Arguments
    /// * `view` - View matrix (world-to-camera)
    /// * `proj` - Projection matrix (camera-to-clip)
    pub fn update_frame(&mut self, view: &[[f32; 4]; 4], proj: &[[f32; 4]; 4]) {
        unsafe {
            let mapped = self.buffer.map();
            let frame_ptr = mapped.as_ptr() as *mut FrameUniforms;
            *frame_ptr = FrameUniforms {
                view: *view,
                proj: *proj,
            };
        }
    }

    /// Update object uniforms at specific index.
    ///
    /// Writes per-object data (model matrix + color) to the object array.
    /// Automatically handles object index lookup and offset calculation.
    ///
    /// # Arguments
    /// * `index` - Object index (0-255)
    /// * `model` - Model matrix (object-to-world)
    /// * `color` - Color tint (RGBA)
    ///
    /// # Panics
    /// Panics if index >= 256
    pub fn update_object(
        &mut self,
        index: usize,
        model: &[[f32; 4]; 4],
        color: &[f32; 4],
    ) {
        assert!(index < StorageUniformLayout::MAX_OBJECTS, "Object index out of bounds");

        // Calculate offset for this object
        let offset = StorageUniformLayout::object_offset(index);

        // Map and write object uniforms at calculated offset
        unsafe {
            let mapped = self.buffer.map();
            let object_ptr = (mapped.as_ptr() as usize + offset) as *mut ObjectUniforms;
            *object_ptr = ObjectUniforms {
                model: *model,
                color: *color,
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
    /// * `desc_layout` - Descriptor set layout for uniform set (set 0)
    ///
    /// # Returns
    /// A StorageDescriptorSet that can be bound to a pipeline
    pub fn create_descriptor_set(
        &self,
        context: &Rc<VulkanContext>,
        desc_layout: vk::DescriptorSetLayout,
    ) -> Result<StorageDescriptorSet, vk::Result> {
        StorageDescriptorSet::new(context, &self.buffer, desc_layout)
    }
}

// Backward compatibility aliases
#[deprecated(since = "0.1.0", note = "Use StorageDescriptorSet instead")]
pub type BdaDescriptorSet = StorageDescriptorSet;

#[deprecated(since = "0.1.0", note = "Use StorageUniformLayout instead")]
pub type BdaUniformLayout = StorageUniformLayout;

#[deprecated(since = "0.1.0", note = "Use StorageUniformManager instead")]
pub type BdaUniformManager = StorageUniformManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_uniforms_size() {
        assert_eq!(std::mem::size_of::<FrameUniforms>(), 128);
    }

    #[test]
    fn test_object_uniforms_size() {
        assert_eq!(std::mem::size_of::<ObjectUniforms>(), 80);
    }

    #[test]
    fn test_layout_constants() {
        assert_eq!(StorageUniformLayout::FRAME_OFFSET, 0);
        assert_eq!(StorageUniformLayout::FRAME_SIZE, 128);
        assert_eq!(StorageUniformLayout::OBJECT_ARRAY_OFFSET, 128);
        assert_eq!(StorageUniformLayout::OBJECT_STRIDE, 80);
        assert_eq!(StorageUniformLayout::MAX_OBJECTS, 256);
        assert_eq!(StorageUniformLayout::MAX_BUFFER_SIZE, 20608);
    }

    #[test]
    fn test_aligned_object_slots() {
        // 80 bytes / 16 = 5 slots
        assert_eq!(StorageUniformLayout::aligned_slots(), 5);
    }

    #[test]
    fn test_object_offset_calculation() {
        // Object 0: offset 128
        assert_eq!(StorageUniformLayout::object_offset(0), 128);
        // Object 1: offset 128 + 80 = 208
        assert_eq!(StorageUniformLayout::object_offset(1), 208);
        // Object 255: offset 128 + (80 * 255) = 128 + 20400 = 20528
        assert_eq!(StorageUniformLayout::object_offset(255), 20528);
    }

    #[test]
    fn test_max_buffer_size() {
        // Frame (128) + objects (80 * 256) = 20608
        assert_eq!(StorageUniformLayout::total_size(), 20608);
    }
}
