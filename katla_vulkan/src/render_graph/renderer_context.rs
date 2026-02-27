//! Render frame context for passing per-frame data to execution closures

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use ash::khr::push_descriptor::Device as PushDescriptorDevice;

use crate::handle::{BufferHandle, ImageHandle, ResourceStorage};
use crate::renderer::registry::AssetRegistry;
use crate::renderer::{DrawList, PipelineHandle};
use crate::sync::{VkBuffer, VkImage, VkImageView};
use crate::vulkan::material::{
    MaterialPipeline, SkeletonDescriptorSet, StorageDescriptorSet, StorageUniformManager,
};
use crate::vulkan::BindlessTextureManager;
use crate::MaterialPipelineCache;

/// Trait for render frame context - provides access to per-frame data
/// without coupling katla_vulkan to application types.
pub trait RenderFrameContext: Any + Send + Sync {
    /// Get a reference to the context as Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get a mutable reference to the context as Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Default empty context implementation
pub struct EmptyRenderFrameContext;

impl RenderFrameContext for EmptyRenderFrameContext {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Raw pointers to renderer state for render graph passes.
///
/// Uses raw pointers because GPU resources cannot be cloned. Mutable access
/// is managed through the render graph's sequential execution model.
///
/// # Safety
///
/// Pointers must remain valid for the lifetime of the render graph execution.
/// This is guaranteed because VulkanRenderer owns the data and outlives the graph.
#[derive(Clone, Default)]
pub struct RendererContextPointers {
    /// Asset registry for mesh and material access (read-only during rendering)
    pub asset_registry: *const AssetRegistry,
    /// Material pipeline cache for resolving handles (read-only)
    pub material_cache: *const MaterialPipelineCache,
    /// Storage uniform manager for frame/object data (mutable during rendering)
    pub storage_manager: *mut StorageUniformManager,
    /// Storage descriptor set for binding (set 0)
    pub storage_descriptor_set: *const Option<StorageDescriptorSet>,
    /// Skeleton descriptors for GPU skeletal animation
    pub skeleton_descriptors: *const Vec<Option<SkeletonDescriptorSet>>,
    /// Bindless texture manager (read-only during rendering)
    pub bindless_manager: *const BindlessTextureManager,
    /// External image storage for resolving ImageHandle to (VkImage, VkImageView)
    pub external_images: *const ResourceStorage<(VkImage, VkImageView)>,
    /// External buffer storage for resolving BufferHandle to VkBuffer
    pub external_buffers: *const ResourceStorage<VkBuffer>,
    /// Device handle for Vulkan commands (cloned, not a pointer)
    pub vk_device: Option<ash::Device>,
    /// Push descriptor loader for dynamic descriptor updates
    pub push_descriptor_loader: Option<PushDescriptorDevice>,
}

/// Container for renderer state accessible from render graph passes.
///
/// Uses raw pointers for GPU resources that cannot be cloned.
/// Mutable access patterns are enforced by the render graph's sequential execution.
#[derive(Clone, Default)]
pub struct RendererContext {
    /// Raw pointers to non-cloneable GPU resources
    pub pointers: RendererContextPointers,
    /// Draw list for the current frame (already Rc<RefCell<>>)
    pub draw_list: Option<Rc<RefCell<Option<DrawList>>>>,
}

impl RendererContext {
    /// Create a new empty RendererContext.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if storage uniform system is available.
    pub fn has_storage(&self) -> bool {
        !self.pointers.storage_manager.is_null() && !self.pointers.storage_descriptor_set.is_null()
    }

    /// Get storage descriptor set for binding (set 0).
    pub fn storage_descriptor(&self) -> Option<crate::sync::VkDescriptorSet> {
        unsafe {
            (*self.pointers.storage_descriptor_set)
                .as_ref()
                .map(|ds| ds.set())
        }
    }

    /// Check if asset registry is available.
    pub fn has_asset_registry(&self) -> bool {
        !self.pointers.asset_registry.is_null()
    }

    /// Get the asset registry (read-only).
    pub fn asset_registry(&self) -> Option<&AssetRegistry> {
        if self.pointers.asset_registry.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.asset_registry) }
        }
    }

    /// Get the material pipeline cache (read-only).
    pub fn material_cache(&self) -> Option<&MaterialPipelineCache> {
        if self.pointers.material_cache.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.material_cache) }
        }
    }

    /// Get a material pipeline by handle.
    pub fn get_pipeline(&self, handle: PipelineHandle) -> Option<&MaterialPipeline> {
        self.material_cache()?.get_pipeline(handle)
    }

    /// Update storage uniforms for an object.
    ///
    /// This is the only method that mutates storage state, keeping mutation
    /// controlled and explicit rather than exposing a mutable reference.
    pub fn update_object_uniforms(
        &self,
        index: usize,
        model: &[f32; 16],
        color: &[f32; 4],
        metallic: f32,
        roughness: f32,
        ao: f32,
        emission_idx: f32,
        texture_indices: [u32; 4],
    ) {
        if self.pointers.storage_manager.is_null() {
            return;
        }
        unsafe {
            (*self.pointers.storage_manager).update_object_bindless(
                index,
                model,
                color,
                metallic,
                roughness,
                ao,
                emission_idx,
                texture_indices,
            );
        }
    }

    /// Get the bindless manager (read-only).
    pub fn bindless_manager(&self) -> Option<&BindlessTextureManager> {
        if self.pointers.bindless_manager.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.bindless_manager) }
        }
    }

    /// Get the skeleton descriptors (read-only).
    pub fn skeleton_descriptors(&self) -> Option<&Vec<Option<SkeletonDescriptorSet>>> {
        if self.pointers.skeleton_descriptors.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.skeleton_descriptors) }
        }
    }

    /// Get the device.
    pub fn vk_device(&self) -> Option<&ash::Device> {
        self.pointers.vk_device.as_ref()
    }

    /// Get the push descriptor loader.
    pub fn push_descriptor_loader(&self) -> Option<&PushDescriptorDevice> {
        self.pointers.push_descriptor_loader.as_ref()
    }

    /// Resolve an ImageHandle to (VkImage, VkImageView).
    /// Returns None if the handle is invalid or storage is not available.
    pub fn get_external_image(&self, handle: ImageHandle) -> Option<(VkImage, VkImageView)> {
        if self.pointers.external_images.is_null() {
            None
        } else {
            unsafe {
                (*self.pointers.external_images)
                    .get(handle.index())
                    .copied()
            }
        }
    }

    /// Resolve a BufferHandle to VkBuffer.
    /// Returns None if the handle is invalid or storage is not available.
    pub fn get_external_buffer(&self, handle: BufferHandle) -> Option<VkBuffer> {
        if self.pointers.external_buffers.is_null() {
            None
        } else {
            unsafe {
                (*self.pointers.external_buffers)
                    .get(handle.index())
                    .copied()
            }
        }
    }
}
