use ash::{
    ext::debug_utils::Instance as DebugInstance,
    khr::{
        push_descriptor::Device as PushDescriptorDevice, surface::Instance as SurfaceInstance,
        swapchain::Device as SwapchainDevice,
    },
    vk::{self},
    Device, Entry, Instance,
};
use gpu_allocator::{
    vulkan::{Allocation, AllocationScheme, Allocator, AllocatorCreateDesc},
    AllocationSizes, AllocatorDebugSettings,
};
use log::{debug, info};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    cell::RefCell,
    ffi::{c_void, CStr, CString},
    mem::ManuallyDrop,
    rc::Rc,
    sync::{Arc, Mutex},
};

use super::SwapchainInfo;

const LAYER_KHRONOS_VALIDATION: &str = concat!("VK_LAYER_KHRONOS_validation", "\0");

use crate::sync::{VkImage, VkImageView, VkSampler};

/// Validation message severity level (internal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ValidationSeverity {
    Verbose,
    Info,
    Warning,
    Error,
}

impl std::fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationSeverity::Verbose => write!(f, "VERBOSE"),
            ValidationSeverity::Info => write!(f, "INFO"),
            ValidationSeverity::Warning => write!(f, "WARNING"),
            ValidationSeverity::Error => write!(f, "ERROR"),
        }
    }
}

impl From<vk::DebugUtilsMessageSeverityFlagsEXT> for ValidationSeverity {
    fn from(severity: vk::DebugUtilsMessageSeverityFlagsEXT) -> Self {
        if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            ValidationSeverity::Error
        } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
            ValidationSeverity::Warning
        } else if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
            ValidationSeverity::Info
        } else {
            ValidationSeverity::Verbose
        }
    }
}

/// Validation message type (internal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValidationMessageType {
    General,
    Validation,
    Performance,
}

impl From<vk::DebugUtilsMessageTypeFlagsEXT> for ValidationMessageType {
    fn from(message_type: vk::DebugUtilsMessageTypeFlagsEXT) -> Self {
        if message_type.contains(vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION) {
            ValidationMessageType::Validation
        } else if message_type.contains(vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE) {
            ValidationMessageType::Performance
        } else {
            ValidationMessageType::General
        }
    }
}

/// A validation message from Vulkan validation layers (internal).
#[derive(Debug, Clone)]
pub(crate) struct ValidationMessage {
    pub severity: ValidationSeverity,
    pub message: String,
    /// VUID (Vulkan Unique ID) if present, e.g., "VUID-vkCmdDraw-None-02700"
    pub vuid: Option<String>,
}

/// Type for validation callbacks (internal).
///
/// The callback receives a reference to the validation message and should return
/// `false` to continue execution or `true` to trigger a breakpoint.
pub(crate) type ValidationCallback = dyn FnMut(&ValidationMessage) -> bool + Send + Sync;

/// Validation message level for user callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    Error,
    Warning,
    Info,
    Debug,
}

impl From<ValidationSeverity> for ValidationLevel {
    fn from(severity: ValidationSeverity) -> Self {
        match severity {
            ValidationSeverity::Error => ValidationLevel::Error,
            ValidationSeverity::Warning => ValidationLevel::Warning,
            ValidationSeverity::Info => ValidationLevel::Info,
            ValidationSeverity::Verbose => ValidationLevel::Debug,
        }
    }
}

/// Internal storage for validation callbacks.
struct ValidationCallbackStorage {
    callback: Option<Box<ValidationCallback>>,
    simplified_callback: Option<Box<dyn FnMut(&str, ValidationLevel) + Send + Sync>>,
    messages: Vec<ValidationMessage>,
}

impl ValidationCallbackStorage {
    fn new() -> Self {
        Self {
            callback: None,
            simplified_callback: None,
            messages: Vec::new(),
        }
    }

    fn call(&mut self, msg: &ValidationMessage) -> bool {
        // Store all messages
        self.messages.push(msg.clone());

        // Call the simplified callback if registered
        if let Some(ref mut cb) = self.simplified_callback {
            cb(&msg.message, ValidationLevel::from(msg.severity));
        }

        // Call the detailed user callback if one is registered
        if let Some(ref mut cb) = self.callback {
            cb(msg)
        } else {
            false
        }
    }

    fn set_callback(&mut self, callback: Box<ValidationCallback>) {
        self.callback = Some(callback);
    }

    fn set_simplified_callback(
        &mut self,
        callback: Box<dyn FnMut(&str, ValidationLevel) + Send + Sync>,
    ) {
        self.simplified_callback = Some(callback);
    }
}

struct QueueFamilyIndices {
    pub graphics_idx: Option<u32>,
    pub transfer_idx: Option<u32>,
}

pub struct RenderTexture {
    pub extent: vk::Extent2D,
    pub(crate) image_view: VkImageView,
    pub format: vk::Format,
    pub(crate) image: VkImage,
    pub image_memory: Option<Allocation>,
    pub context: Rc<VulkanContext>,
}

impl RenderTexture {
    fn destroy(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view.vk(), None);
        }
        let image_memory = self.image_memory.take();

        self.context.free_image(self.image, image_memory.unwrap());
    }
}

impl Drop for RenderTexture {
    fn drop(&mut self) {
        self.destroy();
    }
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
/// | Register material | [`VulkanRenderer::register_material()`] |
/// | Load texture | [`TextureManager::create()`] via [`VulkanRenderer::texture_manager()`] |
/// | Render target | [`VulkanRenderer::create_viewport()`] |
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
/// # let renderer: VulkanRenderer = unsafe { std::mem::zeroed() };
/// let context: &Rc<VulkanContext> = &renderer.context;
///
/// // Escape hatch: query device limits for advanced features
/// let limits = unsafe {
///     context.instance
///         .get_physical_device_properties(context.physical_device)
///         .limits
/// };
/// let max_texture_size = limits.max_image_dimension2_d;
/// ```
///
/// [`VulkanRenderer`]: crate::renderer::VulkanRenderer
/// [`VulkanRenderer::create_mesh()`]: crate::renderer::VulkanRenderer::create_mesh
/// [`VulkanRenderer::register_material()`]: crate::renderer::VulkanRenderer::register_material
/// [`VulkanRenderer::texture_manager()`]: crate::renderer::VulkanRenderer::texture_manager
/// [`VulkanRenderer::create_viewport()`]: crate::renderer::VulkanRenderer::create_viewport
/// [`TextureManager::create()`]: crate::texture::TextureManager::create
pub struct VulkanContext {
    _entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub surface_loader: Option<SurfaceInstance>,
    pub swapchain_loader: Option<Rc<SwapchainDevice>>,
    pub push_descriptor_loader: PushDescriptorDevice,
    pub physical_device: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<RefCell<Allocator>>,
    pub surface: Option<vk::SurfaceKHR>,
    pub graphics_queue: vk::Queue,
    pub gfx_queue: super::Queue,
    pub gfx_cmdpool: super::CommandPool,
    pub transfer_command_pool: vk::CommandPool,
    pub transfer_queue: vk::Queue,
    debug_utils_loader: DebugInstance,
    debug_callback: Option<vk::DebugUtilsMessengerEXT>,
    validation_callback: Arc<Mutex<ValidationCallbackStorage>>,
}
pub struct VulkanFrameCtx {
    pub context: Rc<VulkanContext>,
    pub(crate) swapchain_image_views: Vec<VkImageView>,
    pub swapchain: super::Swapchain,
    pub(crate) swapchain_images: Vec<VkImage>,
    pub depth_render_texture: RenderTexture,
    pub command_buffers: Vec<super::CommandBuffer>,
}

impl QueueFamilyIndices {
    pub fn find_queue_families(
        instance: &Instance,
        surface_loader: &SurfaceInstance,
        surface: vk::SurfaceKHR,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let mut queue_family_indices = Self {
            graphics_idx: None,
            transfer_idx: None,
        };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device);
            info!("Num family indices: {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && surface_loader
                        .get_physical_device_surface_support(physical_device, idx as u32, surface)
                        .unwrap()
                    && queue_family_indices.graphics_idx.is_none()
                {
                    queue_family_indices.graphics_idx = Some(idx as u32);
                    continue;
                }

                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && surface_loader
                        .get_physical_device_surface_support(physical_device, idx as u32, surface)
                        .unwrap()
                    && queue_family_indices.transfer_idx.is_none()
                {
                    queue_family_indices.transfer_idx = Some(idx as u32);
                    continue;
                }
            }
        };

        queue_family_indices
    }

    /// Find queue families for headless rendering (without surface support check).
    /// This is used when VK_EXT_headless_surface is available and we don't need
    /// presentation capabilities.
    pub fn find_queue_families_headless(
        instance: &Instance,
        physical_device: vk::PhysicalDevice,
    ) -> Self {
        let mut queue_family_indices = Self {
            graphics_idx: None,
            transfer_idx: None,
        };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device);
            info!("Num family indices (headless): {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                // Prioritize graphics queue
                if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && queue_family_indices.graphics_idx.is_none()
                {
                    queue_family_indices.graphics_idx = Some(idx as u32);
                    continue;
                }

                // Look for dedicated transfer queue
                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && !properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && queue_family_indices.transfer_idx.is_none()
                {
                    queue_family_indices.transfer_idx = Some(idx as u32);
                    continue;
                }
            }

            // If no dedicated transfer queue found, use graphics queue for transfers
            if queue_family_indices.transfer_idx.is_none() {
                queue_family_indices.transfer_idx = queue_family_indices.graphics_idx;
            }
        };

        queue_family_indices
    }
}

impl VulkanContext {
    pub fn allocate_buffer(
        &self,
        buffer_info: &vk::BufferCreateInfo,
        location: gpu_allocator::MemoryLocation,
    ) -> (vk::Buffer, Allocation) {
        let buffer = unsafe { self.device.create_buffer(buffer_info, None) }.unwrap();
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let allocation_info = gpu_allocator::vulkan::AllocationCreateDesc {
            name: "Buffer Allocation",
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let mut allocator = self.allocator.borrow_mut();
        let allocation = allocator.allocate(&allocation_info).unwrap();

        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .unwrap()
        };
        (buffer, allocation)
    }

    /// Free a buffer and its allocation.
    pub(crate) fn free_buffer(&self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        allocator.free(allocation).unwrap();
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    /// Map a buffer allocation to host memory.
    /// Currently maps the entire buffer; partial mapping could be added as an optimization.
    pub fn map_buffer(&self, allocation: &Allocation) -> *mut u8 {
        allocation.mapped_ptr().unwrap().cast().as_ptr()
    }

    pub fn create_image(
        &self,
        image_create_info: vk::ImageCreateInfo,
        location: gpu_allocator::MemoryLocation,
    ) -> (vk::Image, Allocation) {
        let image = unsafe { self.device.create_image(&image_create_info, None) }.unwrap();
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation_info = gpu_allocator::vulkan::AllocationCreateDesc {
            name: "Image Allocation",
            requirements,
            location,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        };

        let mut allocator = self.allocator.borrow_mut();
        let allocation = allocator.allocate(&allocation_info).unwrap();

        unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .unwrap();
        }
        (image, allocation)
    }

    /// Free an image and its allocation.
    /// Uses wrapper type to avoid exposing vk::Image in public API.
    pub(crate) fn free_image(&self, image: VkImage, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        allocator.free(allocation).unwrap();
        unsafe {
            self.device.destroy_image(image.vk(), None);
        }
    }

    /// Create a CLAMP_TO_EDGE sampler suitable for UI/2D rendering.
    ///
    /// Uses LINEAR filtering with no anisotropy.
    pub(crate) fn create_sampler_clamp_to_edge(&self) -> Result<VkSampler, vk::Result> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .border_color(vk::BorderColor::INT_TRANSPARENT_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(0.0);

        let sampler = unsafe { self.device.create_sampler(&create_info, None)? };
        Ok(VkSampler::new(sampler))
    }

    /// Create a REPEAT sampler with anisotropy for 3D textures.
    ///
    /// Uses LINEAR filtering with 16x anisotropy and mipmaps.
    pub(crate) fn create_sampler_repeat_anisotropic(&self) -> Result<VkSampler, vk::Result> {
        let create_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(16.0)
            .border_color(vk::BorderColor::INT_OPAQUE_WHITE)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::NEVER)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0.0)
            .min_lod(0.0)
            .max_lod(vk::LOD_CLAMP_NONE);

        let sampler = unsafe { self.device.create_sampler(&create_info, None)? };
        Ok(VkSampler::new(sampler))
    }

    fn create_instance(
        with_validation_layers: bool,
        app_name: &CStr,
        engine_name: &CStr,
        display: Option<&dyn HasDisplayHandle>,
        entry: &Entry,
    ) -> Instance {
        if with_validation_layers && !check_validation_support(entry) {
            panic!("Validation layers requested, but unavailable!");
        }

        let mut extension_names_raw = if let Some(d) = display {
            // Windowed mode - use surface extensions from display
            ash_window::enumerate_required_extensions(d.display_handle().unwrap().as_raw())
                .unwrap()
                .to_vec()
        } else {
            // Headless/testing mode - no surface extensions needed
            vec![]
        };

        let mut instance_layers = vec![];
        if with_validation_layers {
            extension_names_raw.push(ash::ext::debug_utils::NAME.as_ptr());
            instance_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        }
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(engine_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 3, 0));
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names_raw)
            .enabled_layer_names(&instance_layers);

        unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Vk Instance creation error")
        }
    }

    //https://vulkan-tutorial.com/Depth_buffering
    pub fn find_supported_format(
        &self,
        candidates: Vec<vk::Format>,
        tiling: vk::ImageTiling,
        features: vk::FormatFeatureFlags,
    ) -> vk::Format {
        let mut format = None;
        for candidate in candidates {
            let format_props = unsafe {
                self.instance
                    .get_physical_device_format_properties(self.physical_device, candidate)
            };

            let has_features = format_props.optimal_tiling_features & features == features;

            if has_features
                && (tiling == vk::ImageTiling::LINEAR || tiling == vk::ImageTiling::OPTIMAL)
            {
                format = Some(candidate);
                break;
            }
        }

        format.expect("No acceptable format found!")
    }

    pub fn find_depth_format(&self) -> vk::Format {
        let candidates = vec![
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::D32_SFLOAT,
            vk::Format::D24_UNORM_S8_UINT,
        ];
        let tiling = vk::ImageTiling::OPTIMAL;
        let features = vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
        self.find_supported_format(candidates, tiling, features)
    }

    pub fn pre_destroy(&self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
    }

    /// Begin a one-time command buffer for transfer operations.
    /// NOTE: For better performance in multi-threaded scenarios, consider using
    /// per-thread command pools and dedicated transfer queues.
    pub fn begin_single_time_commands(&self) -> super::CommandBuffer {
        let command_buffer = super::CommandBuffer::new(&self.device, &self.gfx_cmdpool);
        command_buffer.begin_single_time_command();
        command_buffer
    }

    pub fn end_single_time_commands(&self, command_buffer: super::CommandBuffer) {
        command_buffer.end_single_time_command();
        let command_buffers = vec![&command_buffer];
        // NOTE: This submits to the graphics queue synchronously. For multi-frame
        // concurrency, use a separate transfer queue or proper synchronization.
        self.gfx_queue
            .submit(&command_buffers, &[], &[], vk::Fence::null());
        self.gfx_queue.wait_idle();
        command_buffer.return_to_pool();
    }

    pub fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let entry = unsafe { Entry::load() }.unwrap();
        let instance = Self::create_instance(
            with_validation_layers,
            &app_name,
            &engine_name,
            Some(display),
            &entry,
        );
        let debug_utils_loader = DebugInstance::new(&entry, &instance);

        // Create validation callback storage
        let validation_callback = Arc::new(Mutex::new(ValidationCallbackStorage::new()));
        let user_data = Arc::into_raw(validation_callback.clone()) as *mut c_void;

        let debug_callback =
            create_debug_messenger(&debug_utils_loader, with_validation_layers, user_data);
        let surface_loader = SurfaceInstance::new(&entry, &instance);
        let surface = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                display.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )
        }
        .unwrap();

        let physical_device =
            unsafe { pick_physical_device(&instance, &surface_loader, surface) }.unwrap();

        let queue_indices = QueueFamilyIndices::find_queue_families(
            &instance,
            &surface_loader,
            surface,
            physical_device,
        );

        let queue_create_infos = vec![
            vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_indices.graphics_idx.unwrap())
                .queue_priorities(&[1.0]),
            // vk::DeviceQueueCreateInfo::builder()
            //     .queue_family_index(queue_indices.transfer_idx.unwrap())
            //     .queue_priorities(&[0.5])
            //     .build(),
        ];
        let graphics_queue_idx = queue_indices.graphics_idx.unwrap();
        let transfer_queue_idx = 0; //queue_indices.transfer_idx.unwrap();

        let device = create_device(
            &instance,
            physical_device,
            queue_create_infos,
            with_validation_layers,
            true, // enable_swapchain = true
        );

        let swapchain_loader = Rc::new(SwapchainDevice::new(&instance, &device));
        let push_descriptor_loader = PushDescriptorDevice::new(&instance, &device);

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let gfx_queue = super::Queue::new(device.clone(), graphics_queue_idx, 0);
        let gfx_cmdpool = super::CommandPool::new(device.clone(), graphics_queue_idx);

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool =
            unsafe { device.create_command_pool(&create_info, None) }.unwrap();

        let mut debug_settings = AllocatorDebugSettings::default();
        debug_settings.log_leaks_on_shutdown = true;
        let create_info = AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings,
            buffer_device_address: true, // Enable BDA for allocator
            allocation_sizes: AllocationSizes::default(),
        };

        let allocator = ManuallyDrop::new(RefCell::new(Allocator::new(&create_info).unwrap()));

        Self {
            _entry: entry,
            instance,
            device,
            surface_loader: Some(surface_loader),
            swapchain_loader: Some(swapchain_loader),
            push_descriptor_loader,
            physical_device,
            allocator,
            surface: Some(surface),
            graphics_queue,
            gfx_queue,
            gfx_cmdpool,
            transfer_command_pool,
            transfer_queue,
            debug_utils_loader,
            debug_callback,
            validation_callback,
        }
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
    /// * `with_validation_layers` - Enable validation layers for debugging
    /// * `app_name` - Application name for Vulkan identification
    /// * `engine_name` - Engine name for Vulkan identification
    ///
    /// # Returns
    /// A fully-initialized VulkanContext without a surface or swapchain
    ///
    /// # Panics
    /// - If Vulkan is not available
    /// - If no suitable physical device is found
    /// - If device creation fails
    ///
    /// # Example
    /// ```no_run
    /// use katla_gfx::VulkanContext;
    /// use std::ffi::CString;
    ///
    /// let context = VulkanContext::init_headless(
    ///     true,  // enable validation layers
    ///     CString::new("My App").unwrap(),
    ///     CString::new("My Engine").unwrap(),
    /// );
    /// ```
    pub fn init_headless(
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let entry = unsafe { Entry::load() }.unwrap();
        let instance = Self::create_instance(
            with_validation_layers,
            &app_name,
            &engine_name,
            None, // No display
            &entry,
        );
        let debug_utils_loader = DebugInstance::new(&entry, &instance);

        // Create validation callback storage
        let validation_callback = Arc::new(Mutex::new(ValidationCallbackStorage::new()));
        let user_data = Arc::into_raw(validation_callback.clone()) as *mut c_void;

        let debug_callback =
            create_debug_messenger(&debug_utils_loader, with_validation_layers, user_data);

        // Pick physical device (no swapchain requirement)
        let physical_device = unsafe { pick_physical_device_headless(&instance) }
            .expect("No suitable physical device found for headless rendering");

        // Find queue families (no surface support required)
        let queue_indices =
            QueueFamilyIndices::find_queue_families_headless(&instance, physical_device);

        let queue_create_infos = vec![vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_indices.graphics_idx.unwrap())
            .queue_priorities(&[1.0])];

        let graphics_queue_idx = queue_indices.graphics_idx.unwrap();
        let transfer_queue_idx = queue_indices.transfer_idx.unwrap_or(0);

        // Create device WITHOUT swapchain extension
        let device = create_device(
            &instance,
            physical_device,
            queue_create_infos,
            with_validation_layers,
            false, // enable_swapchain = false
        );

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let push_descriptor_loader = PushDescriptorDevice::new(&instance, &device);

        let gfx_queue = super::Queue::new(device.clone(), graphics_queue_idx, 0);
        let gfx_cmdpool = super::CommandPool::new(device.clone(), graphics_queue_idx);

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool =
            unsafe { device.create_command_pool(&create_info, None) }.unwrap();

        let mut debug_settings = AllocatorDebugSettings::default();
        debug_settings.log_leaks_on_shutdown = true;
        let create_info = AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings,
            buffer_device_address: true, // Enable BDA for allocator
            allocation_sizes: AllocationSizes::default(),
        };

        let allocator = ManuallyDrop::new(RefCell::new(Allocator::new(&create_info).unwrap()));

        Self {
            _entry: entry,
            instance,
            device,
            surface_loader: None,
            swapchain_loader: None,
            push_descriptor_loader,
            physical_device,
            allocator,
            surface: None, // No surface!
            graphics_queue,
            gfx_queue,
            gfx_cmdpool,
            transfer_command_pool,
            transfer_queue,
            debug_utils_loader,
            debug_callback,
            validation_callback,
        }
    }

    /// Set a callback for validation messages.
    ///
    /// The callback receives the message text and severity level.
    ///
    /// # Example
    /// ```no_run
    /// use katla_gfx::{VulkanContext, ValidationLevel};
    ///
    /// # let context: VulkanContext = unsafe { std::mem::zeroed() };
    /// context.set_validation_callback(|message: &str, level: ValidationLevel| {
    ///     println!("[{:?}] {}", level, message);
    /// });
    /// ```
    pub fn set_validation_callback<F>(&self, callback: F)
    where
        F: FnMut(&str, ValidationLevel) + Send + Sync + 'static,
    {
        let mut storage = self.validation_callback.lock().unwrap();
        storage.set_simplified_callback(Box::new(callback));
    }

    /// Set a detailed validation callback (internal use).
    ///
    /// This allows tests to receive full validation message details including VUIDs.
    pub(crate) fn set_validation_callback_detailed(&self, callback: Box<ValidationCallback>) {
        let mut storage = self.validation_callback.lock().unwrap();
        storage.set_callback(callback);
    }

    /// Set up default logging callback that logs validation messages at appropriate levels.
    ///
    /// - Error messages → `error!`
    /// - Warning messages → `warn!`
    /// - Info messages → `info!`
    /// - Verbose messages → `debug!`
    pub fn setup_validation_logging(&self) {
        self.set_validation_callback_detailed(Box::new(|msg| {
            let prefix = if let Some(ref vuid) = msg.vuid {
                format!("[{}]", vuid)
            } else {
                String::new()
            };

            match msg.severity {
                ValidationSeverity::Error => {
                    log::error!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Warning => {
                    log::warn!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Info => {
                    log::info!("{} {}", prefix, msg.message);
                }
                ValidationSeverity::Verbose => {
                    log::debug!("{} {}", prefix, msg.message);
                }
            }
            false // Don't break on any message
        }));
    }
}
impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device
                .destroy_command_pool(self.transfer_command_pool, None);
            self.gfx_cmdpool.destroy();
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);

            // Destroy surface if it exists
            if let Some(surface) = self.surface {
                if let Some(surface_loader) = &self.surface_loader {
                    surface_loader.destroy_surface(surface, None);
                }
            }

            if let Some(messenger) = self.debug_callback {
                self.debug_utils_loader
                    .destroy_debug_utils_messenger(messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}

impl VulkanFrameCtx {
    pub fn create_image_view(
        device: &Device,
        image: vk::Image,
        format: vk::Format,
        aspect_mask: vk::ImageAspectFlags,
    ) -> vk::ImageView {
        let subresource_range = vk::ImageSubresourceRange::default()
            .aspect_mask(aspect_mask)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(subresource_range);
        unsafe { device.create_image_view(&create_info, None) }.unwrap()
    }

    pub fn init(context: &Rc<VulkanContext>) -> Self {
        let swapchain_loader = context
            .swapchain_loader
            .as_ref()
            .expect("Swapchain loader required for VulkanFrameCtx::init");
        let surface_loader = context
            .surface_loader
            .as_ref()
            .expect("Surface loader required for VulkanFrameCtx::init");
        let surface = context
            .surface
            .expect("Surface required for VulkanFrameCtx::init");

        let swapchain = super::Swapchain::create_swapchain(
            swapchain_loader.clone(),
            surface_loader,
            context.physical_device,
            surface,
            None, // No old swapchain
        );

        let swapchain_images = swapchain.get_swapchain_images();

        let swapchain_image_views: Vec<VkImageView> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                VkImageView::new(Self::create_image_view(
                    &context.device,
                    *swapchain_image,
                    swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                ))
            })
            .collect();
        let swapchain_images_wrapped: Vec<VkImage> = swapchain_images
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();
        let depth_render_texture =
            create_depth_render_texture(context.clone(), swapchain.get_extent());

        let command_buffers = context
            .gfx_cmdpool
            .create_command_buffers(swapchain_image_views.len() as _);

        Self {
            context: context.clone(),
            swapchain,
            swapchain_image_views,
            swapchain_images: swapchain_images_wrapped,
            depth_render_texture,
            command_buffers,
        }
    }

    pub fn recreate_swapchain(&mut self) {
        let swapchain_loader = self
            .context
            .swapchain_loader
            .as_ref()
            .expect("Swapchain loader required for recreate_swapchain");
        let surface_loader = self
            .context
            .surface_loader
            .as_ref()
            .expect("Surface loader required for recreate_swapchain");
        let surface = self
            .context
            .surface
            .expect("Surface required for recreate_swapchain");

        let swapchain = super::Swapchain::create_swapchain(
            swapchain_loader.clone(),
            surface_loader,
            self.context.physical_device,
            surface,
            Some(self.swapchain.swapchain),
        );
        self.destroy();
        self.swapchain = swapchain;

        self.swapchain_images = self
            .swapchain
            .get_swapchain_images()
            .iter()
            .map(|img| VkImage::new(*img))
            .collect();

        self.swapchain_image_views = self
            .swapchain
            .get_swapchain_images()
            .iter()
            .map(|swapchain_image| {
                VkImageView::new(Self::create_image_view(
                    &self.context.device,
                    *swapchain_image,
                    self.swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                ))
            })
            .collect();
        self.depth_render_texture =
            create_depth_render_texture(self.context.clone(), self.swapchain.get_extent());
    }

    pub fn destroy(&mut self) {
        unsafe {
            for image_view in &self.swapchain_image_views {
                self.context
                    .device
                    .destroy_image_view(image_view.vk(), None);
            }
            self.swapchain.destroy();
            // self.depth_render_texture.destroy();
        }
    }
}

unsafe fn pick_physical_device(
    instance: &Instance,
    surface_loader: &SurfaceInstance,
    surface: vk::SurfaceKHR,
) -> Option<vk::PhysicalDevice> {
    let physical_devices = instance.enumerate_physical_devices().unwrap();

    let physical_device = physical_devices.into_iter().max_by_key(|physical_device| {
        is_physical_device_suitable(instance, surface_loader, *physical_device, surface)
    });
    if let Some(device) = physical_device {
        let properties = instance.get_physical_device_properties(device);
        info!(
            "Picking physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }
    physical_device
}

unsafe fn is_physical_device_suitable(
    instance: &Instance,
    surface_loader: &SurfaceInstance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> u32 {
    let properties = instance.get_physical_device_properties(physical_device);
    let mut score = 0;

    match properties.device_type {
        vk::PhysicalDeviceType::DISCRETE_GPU => score += 1000,
        vk::PhysicalDeviceType::INTEGRATED_GPU => score += 100,
        vk::PhysicalDeviceType::CPU => score += 10,
        _ => {}
    }

    score += properties.limits.max_image_dimension2_d;

    let swapchain_support =
        SwapchainInfo::query_swapchain_support(surface_loader, physical_device, surface);

    if swapchain_support.surface_formats.is_empty() && swapchain_support.present_modes.is_empty() {
        score = 0;
    }

    score
}

/// Pick a physical device for headless rendering.
/// Simplified version that doesn't require swapchain support.
unsafe fn pick_physical_device_headless(instance: &Instance) -> Option<vk::PhysicalDevice> {
    let physical_devices = instance.enumerate_physical_devices().unwrap();

    // Score devices based on type and capabilities (no swapchain requirement)
    let physical_device = physical_devices.into_iter().max_by_key(|physical_device| {
        let properties = instance.get_physical_device_properties(*physical_device);
        let mut score = 0u32;

        match properties.device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => score += 1000,
            vk::PhysicalDeviceType::INTEGRATED_GPU => score += 100,
            vk::PhysicalDeviceType::CPU => score += 10,
            _ => {}
        }

        score += properties.limits.max_image_dimension2_d;
        score
    });

    if let Some(device) = physical_device {
        let properties = instance.get_physical_device_properties(device);
        info!(
            "Picking physical device (headless): {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }
    physical_device
}

fn create_depth_render_texture(context: Rc<VulkanContext>, extent: vk::Extent2D) -> RenderTexture {
    let depth_format = context.find_depth_format();
    let extent_3d = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .mip_levels(1)
        .array_layers(1)
        .format(depth_format)
        .extent(extent_3d)
        .tiling(vk::ImageTiling::OPTIMAL)
        .samples(vk::SampleCountFlags::TYPE_1)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);

    //https://vulkan-tutorial.com/Depth_buffering
    let (depth_image, image_memory) =
        context.create_image(create_info, gpu_allocator::MemoryLocation::GpuOnly);

    let image_view = VulkanFrameCtx::create_image_view(
        &context.device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH,
    );
    RenderTexture {
        extent,
        image_view: VkImageView::new(image_view),
        image: VkImage::new(depth_image),
        image_memory: Some(image_memory),
        format: depth_format,
        context,
    }
}

fn create_device(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    queue_create_infos: Vec<vk::DeviceQueueCreateInfo>,
    with_validation_layers: bool,
    enable_swapchain: bool,
) -> Device {
    let device_extensions = if enable_swapchain {
        vec![
            ash::khr::swapchain::NAME.as_ptr(),
            ash::khr::push_descriptor::NAME.as_ptr(), // For dynamic texture binding in UI
        ]
    } else {
        vec![ash::khr::push_descriptor::NAME.as_ptr()] // Always enable for UI textures
    };

    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
    }

    // Enable Vulkan 1.3 features (Dynamic Rendering, Synchronization2)
    let vk13_features = vk::PhysicalDeviceVulkan13Features {
        s_type: vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
        p_next: std::ptr::null_mut(),
        dynamic_rendering: vk::TRUE,
        synchronization2: vk::TRUE,
        ..Default::default()
    };

    // Enable Vulkan 1.2 features (Buffer Device Address, Descriptor Indexing for bindless)
    let mut vk12_features = vk::PhysicalDeviceVulkan12Features {
        s_type: vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
        p_next: &vk13_features as *const _ as *mut _,
        buffer_device_address: vk::TRUE,
        descriptor_indexing: vk::TRUE,
        shader_sampled_image_array_non_uniform_indexing: vk::TRUE,
        descriptor_binding_sampled_image_update_after_bind: vk::TRUE,
        descriptor_binding_partially_bound: vk::TRUE,
        descriptor_binding_variable_descriptor_count: vk::TRUE,
        runtime_descriptor_array: vk::TRUE,
        ..Default::default()
    };

    // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
    let features = vk::PhysicalDeviceFeatures {
        sampler_anisotropy: 1,
        ..Default::default()
    };

    let create_info = vk::DeviceCreateInfo::default()
        .enabled_extension_names(&device_extensions)
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&features)
        .push_next(&mut vk12_features);

    unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .unwrap()
    }
}

fn create_debug_messenger(
    debug_utils_loader: &DebugInstance,
    with_validation_layers: bool,
    user_data: *mut c_void,
) -> Option<vk::DebugUtilsMessengerEXT> {
    if with_validation_layers {
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback))
            .user_data(user_data);

        Some(
            unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None) }.unwrap(),
        )
    } else {
        None
    }
}

unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    p_user_data: *mut c_void,
) -> vk::Bool32 {
    unsafe {
        let callback_data = &*p_callback_data;

        // Convert to Rust types
        let severity = ValidationSeverity::from(message_severity);
        let message = CStr::from_ptr(callback_data.p_message)
            .to_string_lossy()
            .to_string();

        // Extract VUID from p_message_id_name if available (this is the canonical source)
        // Otherwise try to find it in the message text (fallback for older validation layers)
        let vuid = if !callback_data.p_message_id_name.is_null() {
            let id_name = unsafe { CStr::from_ptr(callback_data.p_message_id_name) };
            let id_str = id_name.to_string_lossy();
            // Check if it's a VUID (starts with "VUID-")
            if id_str.starts_with("VUID-") {
                Some(id_str.to_string())
            } else {
                None
            }
        } else {
            // Fallback: try to find VUID in message text
            message
                .split_whitespace()
                .find(|s| s.starts_with("VUID-"))
                .map(|s| s.to_string())
        };

        let validation_msg = ValidationMessage {
            severity,
            message,
            vuid,
        };

        // Reconstruct the Arc<Mutex<ValidationCallbackStorage>> from the raw pointer
        let storage = Arc::from_raw(p_user_data as *const Mutex<ValidationCallbackStorage>);
        let mut storage_guard = storage.lock().unwrap();
        let should_break = storage_guard.call(&validation_msg);
        drop(storage_guard);
        let _ = Arc::into_raw(storage); // Don't drop the Arc

        // Always log the message (for backwards compatibility and visibility)
        debug!(
            "{}",
            CStr::from_ptr(callback_data.p_message).to_string_lossy()
        );

        if should_break {
            vk::TRUE
        } else {
            vk::FALSE
        }
    }
}

fn check_validation_support(entry: &Entry) -> bool {
    unsafe {
        let available_layers = entry.enumerate_instance_layer_properties().unwrap();
        let validation_name = CStr::from_ptr(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        for layer in available_layers {
            let layer_name = std::ffi::CStr::from_ptr(layer.layer_name.as_ptr() as _);
            if layer_name == validation_name {
                return true;
            }
        }
    }

    false
}
