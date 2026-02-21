//! Render frame context for passing per-frame data to execution closures

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use ash::vk;

use crate::rendering::registry::AssetRegistry;
use crate::rendering::DrawList;
use crate::vulkan::material::{
    MaterialPipeline, SkeletonDescriptorSet, StorageDescriptorSet, StorageUniformManager,
};
use crate::vulkan::BindlessTextureManager;
use crate::{UIBuffers, UITextures, UiDrawData};

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
/// This uses raw pointers because the underlying GPU resources cannot be cloned.
/// The pointers are valid for the lifetime of the render graph execution.
///
/// # Safety
///
/// The caller must ensure that the VulkanRenderer outlives any RendererContext
/// created from it. This is guaranteed because:
/// 1. RendererContext is created in compile_render_graph()
/// 2. The render graph is stored in VulkanRenderer
/// 3. VulkanRenderer cannot be dropped while the render graph exists
#[derive(Clone, Default)]
pub struct RendererContextPointers {
    /// Asset registry for mesh and material access
    pub asset_registry: *mut AssetRegistry,
    /// Storage uniform manager for frame/object data
    pub storage_manager: *mut Option<StorageUniformManager>,
    /// Storage descriptor set for binding (set 0)
    pub storage_descriptor_set: *const Option<StorageDescriptorSet>,
    /// Skeleton descriptors for GPU skeletal animation
    pub skeleton_descriptors: *const Vec<Option<SkeletonDescriptorSet>>,
    /// Bindless texture manager for efficient texture binding
    pub bindless_manager: *mut Option<BindlessTextureManager>,
    /// Device handle for Vulkan commands (cloned, not a pointer)
    pub vk_device: Option<ash::Device>,
    /// UI data pointer
    pub ui_data: *const std::cell::RefCell<Option<UiDrawData>>,
    /// UI buffers pointer
    pub ui_buffers: *const Vec<UIBuffers>,
    /// UI textures pointer
    pub ui_textures: *const Option<UITextures>,
    /// UI frame index pointer
    pub ui_frame_index: *const std::cell::Cell<usize>,
}

/// Container for renderer state accessible from render graph passes.
///
/// Uses raw pointers for GPU resources that cannot be cloned.
#[derive(Clone, Default)]
pub struct RendererContext {
    /// Raw pointers to non-cloneable GPU resources
    pub pointers: RendererContextPointers,
    /// Draw list for the current frame (already Rc<RefCell<>>)
    pub draw_list: Option<Rc<RefCell<Option<DrawList>>>>,
    /// Application-specific pipelines (owned by application, not used here)
    pub sky_pipeline: Option<Rc<RefCell<Option<Rc<RefCell<MaterialPipeline>>>>>>,
    pub grid_pipeline: Option<Rc<RefCell<Option<Rc<RefCell<MaterialPipeline>>>>>>,
    pub ui_pipeline: Option<Rc<RefCell<Option<Rc<RefCell<MaterialPipeline>>>>>>,
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

    /// Get storage descriptor set as raw vk handle (internal use).
    pub(crate) fn vk_storage_descriptor(&self) -> Option<vk::DescriptorSet> {
        unsafe {
            (*self.pointers.storage_descriptor_set)
                .as_ref()
                .map(|ds| ds.vk_set())
        }
    }

    /// Check if asset registry is available.
    pub fn has_asset_registry(&self) -> bool {
        !self.pointers.asset_registry.is_null()
    }

    /// Get the asset registry.
    pub fn asset_registry(&self) -> Option<&AssetRegistry> {
        if self.pointers.asset_registry.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.asset_registry) }
        }
    }

    /// Get the asset registry mutably.
    pub fn asset_registry_mut(&self) -> Option<&mut AssetRegistry> {
        if self.pointers.asset_registry.is_null() {
            None
        } else {
            unsafe { Some(&mut *self.pointers.asset_registry) }
        }
    }

    /// Get the storage manager.
    pub fn storage_manager(&self) -> Option<&mut Option<StorageUniformManager>> {
        if self.pointers.storage_manager.is_null() {
            None
        } else {
            unsafe { Some(&mut *self.pointers.storage_manager) }
        }
    }

    /// Get the bindless manager.
    pub fn bindless_manager(&self) -> Option<&Option<BindlessTextureManager>> {
        if self.pointers.bindless_manager.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.bindless_manager) }
        }
    }

    /// Get the skeleton descriptors.
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

    /// Get UI data.
    pub fn ui_data(&self) -> Option<&std::cell::RefCell<Option<UiDrawData>>> {
        if self.pointers.ui_data.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.ui_data) }
        }
    }

    /// Get UI buffers.
    pub fn ui_buffers(&self) -> Option<&Vec<UIBuffers>> {
        if self.pointers.ui_buffers.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.ui_buffers) }
        }
    }

    /// Get UI textures.
    pub fn ui_textures(&self) -> Option<&Option<UITextures>> {
        if self.pointers.ui_textures.is_null() {
            None
        } else {
            unsafe { Some(&*self.pointers.ui_textures) }
        }
    }

    /// Get UI frame index.
    pub fn ui_frame_index(&self) -> Option<usize> {
        if self.pointers.ui_frame_index.is_null() {
            None
        } else {
            unsafe { Some((*self.pointers.ui_frame_index).get()) }
        }
    }
}
