//! Render frame context for passing per-frame data to execution closures

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use ash::vk;

use crate::rendering::DrawList;
use crate::rendering::registry::AssetRegistry;
use crate::vulkan::material::{MaterialPipeline, StorageDescriptorSet, StorageUniformManager, SkeletonDescriptorSet};
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

/// Safe container for renderer state accessible from render graph passes.
///
/// This struct wraps all the renderer state that render graph closures need
/// to access, using Rc<RefCell<>> for safe shared mutable access. This
/// eliminates the need for unsafe pointer patterns.
///
/// # Usage
///
/// ```ignore
/// let ctx = renderer.renderer_context();
///
/// graph_builder.add_pass("geometry_pass", move |pass| {
///     pass.execute("geometry_pass", move |ctx| {
///         if let Some(rc) = ctx.renderer_context() {
///             let mut registry = rc.asset_registry.borrow_mut();
///             // Use registry safely...
///         }
///     });
/// });
/// ```
#[derive(Clone, Default)]
pub struct RendererContext {
    /// Asset registry for mesh and material access
    pub asset_registry: Option<Rc<RefCell<AssetRegistry>>>,
    /// Storage uniform manager for frame/object data
    pub storage_manager: Option<Rc<RefCell<Option<StorageUniformManager>>>>,
    /// Storage descriptor set for binding (set 0)
    pub storage_descriptor_set: Option<Rc<RefCell<Option<StorageDescriptorSet>>>>,
    /// Sky rendering pipeline
    pub sky_pipeline: Option<Rc<RefCell<Option<Rc<RefCell<MaterialPipeline>>>>>>,
    /// UI draw data for the current frame
    pub ui_data: Option<Rc<RefCell<Option<UiDrawData>>>>,
    /// UI rendering pipeline
    pub ui_pipeline: Option<Rc<RefCell<Option<Rc<RefCell<MaterialPipeline>>>>>>,
    /// UI vertex/index buffers (one per frame in flight)
    pub ui_buffers: Option<Rc<RefCell<Vec<UIBuffers>>>>,
    /// UI textures (font atlas, viewport)
    pub ui_textures: Option<Rc<RefCell<Option<UITextures>>>>,
    /// Current UI frame index for buffer selection
    pub ui_frame_index: Option<Rc<RefCell<usize>>>,
    /// Skeleton descriptors for GPU skeletal animation
    pub skeleton_descriptors: Option<Rc<RefCell<Vec<Option<SkeletonDescriptorSet>>>>>,
    /// Draw list for the current frame
    pub draw_list: Option<Rc<RefCell<Option<DrawList>>>>,
    /// Device handle for Vulkan commands
    pub device: Option<ash::Device>,
}

impl RendererContext {
    /// Create a new empty RendererContext.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if storage uniform system is available.
    pub fn has_storage(&self) -> bool {
        self.storage_manager.is_some() && self.storage_descriptor_set.is_some()
    }

    /// Get storage descriptor set for binding (set 0).
    pub fn storage_descriptor(&self) -> Option<crate::sync::VkDescriptorSet> {
        self.storage_descriptor_set
            .as_ref()?
            .borrow()
            .as_ref()
            .map(|ds| ds.set())
    }

    /// Get storage descriptor set as raw vk handle (for internal use).
    pub fn vk_storage_descriptor(&self) -> Option<vk::DescriptorSet> {
        self.storage_descriptor_set
            .as_ref()?
            .borrow()
            .as_ref()
            .map(|ds| ds.vk_set())
    }

    /// Check if asset registry is available.
    pub fn has_asset_registry(&self) -> bool {
        self.asset_registry.is_some()
    }

    /// Check if sky pipeline is available.
    pub fn has_sky_pipeline(&self) -> bool {
        self.sky_pipeline
            .as_ref()
            .map(|p| p.borrow().is_some())
            .unwrap_or(false)
    }
}

