mod device;
mod memory;
mod physical_device;
mod queue_family;
mod samplers;
mod swapchain;
mod validation;

use ash::{
    Device, Entry, Instance,
    ext::debug_utils::Instance as DebugInstance,
    khr::{
        push_descriptor::Device as PushDescriptorDevice, surface::Instance as SurfaceInstance,
        swapchain::Device as SwapchainDevice,
    },
    vk,
};
use gpu_allocator::{
    AllocationSizes, AllocatorDebugSettings,
    vulkan::{Allocator, AllocatorCreateDesc},
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    ffi::{CString, c_void},
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::error::RendererError;
use crate::sync::{VkImage, VkImageView};

pub(super) const LAYER_KHRONOS_VALIDATION: &str = concat!("VK_LAYER_KHRONOS_validation", "\0");

pub use crate::error::ValidationMode;
pub(super) use swapchain::RenderTexture;
pub use validation::ValidationLevel;

pub(super) struct QueueFamilyIndices {
    pub graphics_idx: Option<u32>,
    pub transfer_idx: Option<u32>,
}

/// Low-level Vulkan context providing direct access to GPU resources.
///
/// This is an **escape hatch** for advanced use cases where the high-level
/// [`VulkanRenderer`] API is insufficient. Most applications should prefer
/// the high-level API for common operations.
///
/// # High-level alternatives
///
/// | Operation | High-level API |
/// |-----------|---------------|
/// | Create mesh | [`VulkanRenderer::create_mesh()`] |
/// | Compile material | [`crate::GpuRenderer::compile_material`] |
/// | Create texture | [`crate::GpuRenderer::create_texture`] |
/// | Render target | [`crate::FrameGraphBuilder`] |
///
/// # Escape hatch use cases
///
/// Use `VulkanContext` directly when you need to:
/// - Allocate custom GPU buffers with specific memory requirements
/// - Implement render passes outside the standard pipeline
/// - Query physical device limits and features
/// - Integrate with external Vulkan-based libraries
///
/// # Example
///
/// ```no_run
/// use katla_gfx::{VulkanContext, VulkanRenderer};
/// use std::rc::Rc;
///
/// // Normal usage: access context through renderer
/// # fn inspect_limits(renderer: &VulkanRenderer) {
/// let context: &Rc<VulkanContext> = renderer.context();
///
/// // Escape hatch: query device limits for advanced features
/// let limits = unsafe {
///     context.instance
///         .get_physical_device_properties(context.physical_device)
///         .limits
/// };
/// let max_texture_size = limits.max_image_dimension2_d;
/// # }
/// ```
///
/// [`VulkanRenderer`]: crate::renderer::VulkanRenderer
/// [`VulkanRenderer::create_mesh()`]: crate::renderer::VulkanRenderer::create_mesh
/// [`VulkanRenderer::texture_manager()`]: crate::renderer::VulkanRenderer::texture_manager
/// [`TextureManager::create()`]: crate::texture::TextureManager::create
pub struct VulkanContext {
    pub(super) _entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub surface_loader: Option<SurfaceInstance>,
    pub swapchain_loader: Option<Rc<SwapchainDevice>>,
    pub push_descriptor_loader: PushDescriptorDevice,
    pub physical_device: vk::PhysicalDevice,
    pub allocator: memory::GpuAllocator,
    surface: std::cell::Cell<Option<vk::SurfaceKHR>>,
    pub graphics_queue: vk::Queue,
    pub gfx_queue: super::Queue,
    pub gfx_cmdpool: super::CommandPool,
    pub transfer_command_pool: vk::CommandPool,
    pub transfer_queue: vk::Queue,
    pub(super) debug_utils_loader: DebugInstance,
    pub(super) debug_callback: Option<vk::DebugUtilsMessengerEXT>,
    pub(crate) validation_callback: Arc<Mutex<validation::ValidationCallbackStorage>>,
    pub(super) gpu_assisted_validation: bool,
    /// Whether VK_KHR_push_descriptor is enabled for per-draw texture binding in UI.
    pub push_descriptor_enabled: bool,
    /// Cached KHR push descriptor function pointer for efficient access.
    pub push_descriptor_khr: Option<ash::khr::push_descriptor::Device>,
    /// Cached non-coherent atom size for aligned memory flushes.
    pub non_coherent_atom_size: vk::DeviceSize,
    /// Submitted one-time commands and optional staging allocations retained
    /// until a fence proves completion.
    pub(crate) pending_submissions: std::cell::RefCell<Vec<PendingSubmission>>,
    pub(crate) graph_buffer_history:
        std::cell::RefCell<crate::render_graph::BufferExecutionHistory>,
}

/// One submitted command buffer awaiting fence completion.
pub(crate) struct PendingSubmission {
    fence: vk::Fence,
    command_buffer: super::CommandBuffer,
    staging: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
}

pub struct VulkanFrameCtx {
    pub context: Rc<VulkanContext>,
    pub(crate) swapchain_image_views: Vec<VkImageView>,
    pub swapchain: Option<super::Swapchain>,
    pub(crate) extent: vk::Extent2D,
    pub(crate) offscreen_targets: Vec<RenderTexture>,
    pub(crate) swapchain_images: Vec<VkImage>,
    pub(crate) swapchain_image_layouts: Vec<std::cell::Cell<vk::ImageLayout>>,
    pub(crate) swapchain_image_contents: Vec<std::cell::Cell<bool>>,
    pub(crate) pending_output_contents: std::cell::Cell<Option<(usize, bool)>>,
    pub(crate) pending_transient_layouts:
        std::cell::RefCell<crate::render_graph::ImageLayoutJournal>,
    pub command_buffers: Vec<super::CommandBuffer>,
}

impl VulkanContext {
    pub fn pre_destroy(&self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }

    /// Get window surface resources (swapchain loader, surface loader, surface).
    ///
    /// Returns `None` in headless mode where no window surface exists.
    /// Use this instead of repeatedly unwrapping the individual Option fields.
    pub(crate) fn window_resources(
        &self,
    ) -> Option<(&Rc<SwapchainDevice>, &SurfaceInstance, vk::SurfaceKHR)> {
        Some((
            self.swapchain_loader.as_ref()?,
            self.surface_loader.as_ref()?,
            self.surface.get()?,
        ))
    }

    /// Begin a one-time command buffer for transfer operations.
    /// NOTE: For better performance in multi-threaded scenarios, consider using
    /// per-thread command pools and dedicated transfer queues.
    pub fn begin_single_time_commands(
        &self,
    ) -> Result<super::CommandBuffer, crate::error::RendererError> {
        let command_buffer = super::CommandBuffer::new(&self.device, &self.gfx_cmdpool);
        if let Err(error) = command_buffer.begin_single_time_command() {
            command_buffer.return_to_pool();
            return Err(error);
        }
        Ok(command_buffer)
    }

    pub fn end_single_time_commands(
        &self,
        command_buffer: super::CommandBuffer,
    ) -> Result<(), crate::error::RendererError> {
        if let Err(error) = command_buffer.end_single_time_command() {
            command_buffer.return_to_pool();
            return Err(error);
        }
        let fence = match unsafe {
            self.device
                .create_fence(&vk::FenceCreateInfo::default(), None)
        } {
            Ok(fence) => fence,
            Err(error) => {
                command_buffer.return_to_pool();
                return Err(RendererError::VulkanError(
                    "Failed to create submission fence".into(),
                    error,
                ));
            }
        };
        if let Err(error) = self.gfx_queue.submit(&[&command_buffer], &[], &[], fence) {
            command_buffer.return_to_pool();
            unsafe {
                self.device.destroy_fence(fence, None);
            }
            return Err(error);
        }
        match unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) } {
            Ok(()) => {
                command_buffer.return_to_pool();
                unsafe {
                    self.device.destroy_fence(fence, None);
                }
                Ok(())
            }
            Err(error) => {
                self.defer_submission(fence, command_buffer, None);
                Err(RendererError::VulkanError(
                    "Failed to wait for submission fence".into(),
                    error,
                ))
            }
        }
    }

    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        validation_mode: ValidationMode,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let entry = unsafe { Entry::load() }.map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to load Vulkan entry: {:?}", e))
        })?;
        let (instance, validation_layers_active) = Self::create_instance(
            validation_mode,
            &app_name,
            &engine_name,
            Some(display),
            &entry,
        )?;
        let debug_utils_loader = DebugInstance::new(&entry, &instance);

        // Create validation callback storage
        let validation_callback =
            Arc::new(Mutex::new(validation::ValidationCallbackStorage::new()));
        let user_data = Arc::into_raw(validation_callback.clone()) as *mut c_void;

        let debug_callback = validation::create_debug_messenger(
            &debug_utils_loader,
            validation_layers_active,
            user_data,
        );
        let surface_loader = SurfaceInstance::new(&entry, &instance);
        let display_handle = display.display_handle().map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to get display handle: {:?}", e))
        })?;
        let window_handle = window.window_handle().map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to get window handle: {:?}", e))
        })?;
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                display_handle.as_raw(),
                window_handle.as_raw(),
                None,
            )
        }
        .map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to create surface: {:?}", e))
        })?;

        let physical_device = unsafe {
            physical_device::pick_physical_device(&instance, Some((&surface_loader, surface)))
        }?;

        let queue_indices = QueueFamilyIndices::find_queue_families(
            &instance,
            &surface_loader,
            surface,
            physical_device,
        )?;

        let graphics_queue_idx = queue_indices.graphics_idx.ok_or_else(|| {
            RendererError::InitializationFailed("No graphics queue family found".to_string())
        })?;
        let transfer_queue_idx = queue_indices.transfer_idx.unwrap_or(graphics_queue_idx);

        let queue_create_infos = if transfer_queue_idx != graphics_queue_idx {
            vec![
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_queue_idx)
                    .queue_priorities(&[1.0]),
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(transfer_queue_idx)
                    .queue_priorities(&[1.0]),
            ]
        } else {
            vec![
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_queue_idx)
                    .queue_priorities(&[1.0]),
            ]
        };

        let device = device::create_device(&instance, physical_device, &queue_create_infos, true)?;

        let swapchain_loader = Rc::new(SwapchainDevice::new(&instance, &device));
        let push_descriptor_loader = PushDescriptorDevice::new(&instance, &device);

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let gfx_queue = super::Queue::new(device.clone(), graphics_queue_idx, 0);
        let gfx_cmdpool = super::CommandPool::new(device.clone(), graphics_queue_idx);

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool = unsafe { device.create_command_pool(&create_info, None) }
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create transfer command pool: {:?}",
                    e
                ))
            })?;

        let mut debug_settings = AllocatorDebugSettings::default();
        debug_settings.log_leaks_on_shutdown = true;
        let allocator_create_info = AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings,
            buffer_device_address: true,
            allocation_sizes: AllocationSizes::default(),
        };

        let allocator = memory::GpuAllocator::new(
            Allocator::new(&allocator_create_info)
                .map_err(|e| RendererError::from_allocation_error("GPU", e))?,
        );

        let push_descriptor_khr = Some(ash::khr::push_descriptor::Device::new(&instance, &device));

        let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let non_coherent_atom_size = device_properties.limits.non_coherent_atom_size;

        Ok(Self {
            _entry: entry,
            instance,
            device,
            surface_loader: Some(surface_loader),
            swapchain_loader: Some(swapchain_loader),
            push_descriptor_loader,
            physical_device,
            allocator,
            surface: std::cell::Cell::new(Some(surface)),
            graphics_queue,
            gfx_queue,
            gfx_cmdpool,
            transfer_command_pool,
            transfer_queue,
            debug_utils_loader,
            debug_callback,
            validation_callback,
            gpu_assisted_validation: validation_layers_active && validation_mode.is_gpu_assisted(),
            push_descriptor_enabled: true,
            push_descriptor_khr,
            non_coherent_atom_size,
            pending_submissions: std::cell::RefCell::new(Vec::new()),
            graph_buffer_history: Default::default(),
        })
    }

    /// Initialize VulkanContext for testing/headless rendering.
    ///
    /// This creates a VulkanContext without a surface or swapchain, enabling:
    /// - Automated testing in CI/CD pipelines
    /// - Render graph validation without windows
    /// - Offline rendering and compute workloads
    /// - Tests can create their own VkImage render targets
    ///
    /// # Arguments
    /// * `validation_mode` - Validation mode (Disabled, Enabled, or GpuAssisted)
    /// * `app_name` - Application name for Vulkan identification
    /// * `engine_name` - Engine name for Vulkan identification
    ///
    /// # Returns
    /// A fully-initialized VulkanContext without a surface or swapchain
    ///
    /// # Errors
    /// Returns `RendererError::InitializationFailed` if:
    /// - Vulkan is not available
    /// - No suitable physical device is found
    /// - Device creation fails
    ///
    /// # Example
    /// ```no_run
    /// use katla_gfx::{VulkanContext, ValidationMode};
    /// use std::ffi::CString;
    ///
    /// let context = VulkanContext::init_headless(
    ///     ValidationMode::GpuAssisted,  // enable GPU-assisted validation
    ///     CString::new("My App").unwrap(),
    ///     CString::new("My Engine").unwrap(),
    /// ).expect("Failed to create headless Vulkan context");
    /// ```
    pub fn init_headless(
        validation_mode: ValidationMode,
        app_name: CString,
        engine_name: CString,
    ) -> Result<Self, RendererError> {
        let entry = unsafe { Entry::load() }.map_err(|e| {
            RendererError::InitializationFailed(format!("Failed to load Vulkan entry: {:?}", e))
        })?;
        let (instance, validation_layers_active) =
            Self::create_instance(validation_mode, &app_name, &engine_name, None, &entry)?;
        let debug_utils_loader = DebugInstance::new(&entry, &instance);

        // Create validation callback storage
        let validation_callback =
            Arc::new(Mutex::new(validation::ValidationCallbackStorage::new()));
        let user_data = Arc::into_raw(validation_callback.clone()) as *mut c_void;

        let debug_callback = validation::create_debug_messenger(
            &debug_utils_loader,
            validation_layers_active,
            user_data,
        );

        // Pick physical device (no swapchain requirement)
        let physical_device = unsafe { physical_device::pick_physical_device(&instance, None) }?;

        // Find queue families (no surface support required)
        let queue_indices =
            QueueFamilyIndices::find_queue_families_headless(&instance, physical_device);

        let graphics_queue_idx = queue_indices.graphics_idx.ok_or_else(|| {
            RendererError::InitializationFailed("No graphics queue family found".to_string())
        })?;
        let transfer_queue_idx = queue_indices.transfer_idx.unwrap_or(graphics_queue_idx);

        let queue_create_infos = if transfer_queue_idx != graphics_queue_idx {
            vec![
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_queue_idx)
                    .queue_priorities(&[1.0]),
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(transfer_queue_idx)
                    .queue_priorities(&[1.0]),
            ]
        } else {
            vec![
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_queue_idx)
                    .queue_priorities(&[1.0]),
            ]
        };

        // Create device WITHOUT swapchain extension
        let device = device::create_device(&instance, physical_device, &queue_create_infos, false)?;

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let push_descriptor_loader = PushDescriptorDevice::new(&instance, &device);

        let gfx_queue = super::Queue::new(device.clone(), graphics_queue_idx, 0);
        let gfx_cmdpool = super::CommandPool::new(device.clone(), graphics_queue_idx);

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool = unsafe { device.create_command_pool(&create_info, None) }
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create transfer command pool: {:?}",
                    e
                ))
            })?;

        let mut debug_settings = AllocatorDebugSettings::default();
        debug_settings.log_leaks_on_shutdown = true;
        let allocator_create_info = AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings,
            buffer_device_address: true,
            allocation_sizes: AllocationSizes::default(),
        };

        let allocator = memory::GpuAllocator::new(
            Allocator::new(&allocator_create_info)
                .map_err(|e| RendererError::from_allocation_error("GPU", e))?,
        );

        let push_descriptor_khr = Some(ash::khr::push_descriptor::Device::new(&instance, &device));

        let device_properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let non_coherent_atom_size = device_properties.limits.non_coherent_atom_size;

        Ok(Self {
            _entry: entry,
            instance,
            device,
            surface_loader: None,
            swapchain_loader: None,
            push_descriptor_loader,
            physical_device,
            allocator,
            surface: std::cell::Cell::new(None),
            graphics_queue,
            gfx_queue,
            gfx_cmdpool,
            transfer_command_pool,
            transfer_queue,
            debug_utils_loader,
            debug_callback,
            validation_callback,
            gpu_assisted_validation: validation_layers_active && validation_mode.is_gpu_assisted(),
            push_descriptor_enabled: true,
            push_descriptor_khr,
            non_coherent_atom_size,
            pending_submissions: std::cell::RefCell::new(Vec::new()),
            graph_buffer_history: Default::default(),
        })
    }
}

impl VulkanContext {
    /// Retain submitted commands and optional staging memory until completion.
    pub(crate) fn defer_submission(
        &self,
        fence: vk::Fence,
        command_buffer: super::CommandBuffer,
        staging: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation)>,
    ) {
        self.pending_submissions
            .borrow_mut()
            .push(PendingSubmission {
                fence,
                command_buffer,
                staging,
            });
    }

    /// Release one-time submissions whose fences prove completion.
    ///
    /// Called at frame boundaries (the entries' fences are typically long
    /// signaled) and after device-wide idle waits.
    pub(crate) fn drain_completed_submissions(&self) {
        let mut pending = self.pending_submissions.borrow_mut();
        let mut index = 0;
        while index < pending.len() {
            let entry = &pending[index];
            let completed = unsafe { self.device.get_fence_status(entry.fence) }.unwrap_or(false);
            if completed {
                self.release_submission(pending.swap_remove(index));
            } else {
                index += 1;
            }
        }
    }

    /// Release every retained submission after device-wide idle.
    pub(crate) fn drain_all_submissions(&self) {
        for entry in std::mem::take(&mut *self.pending_submissions.borrow_mut()) {
            self.release_submission(entry);
        }
    }

    fn release_submission(&self, entry: PendingSubmission) {
        entry.command_buffer.return_to_pool();
        unsafe {
            self.device.destroy_fence(entry.fence, None);
        }
        if let Some((buffer, allocation)) = entry.staging {
            self.free_buffer(buffer, allocation);
        }
    }

    /// Number of staged uploads awaiting completion (diagnostics, tests).
    pub(crate) fn pending_staged_uploads(&self) -> usize {
        self.pending_submissions
            .borrow()
            .iter()
            .filter(|submission| submission.staging.is_some())
            .count()
    }

    /// Release presentation resources while the native window and display still exist.
    pub(crate) fn destroy_surface(&self) {
        if let Some(surface) = self.surface.take()
            && let Some(loader) = &self.surface_loader
        {
            unsafe {
                loader.destroy_surface(surface, None);
            }
        }
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.drain_all_submissions();

            self.device
                .destroy_command_pool(self.transfer_command_pool, None);
            self.gfx_cmdpool.destroy();
            self.allocator.destroy();
            self.device.destroy_device(None);

            self.destroy_surface();

            if let Some(messenger) = self.debug_callback {
                self.debug_utils_loader
                    .destroy_debug_utils_messenger(messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}
