use crate::renderer::ENTRY_LOADER;

use erupt::{
    cstr,
    extensions::{ext_debug_utils, khr_surface::*, khr_swapchain::*},
    utils::allocator::AllocationObject,
    utils::VulkanResult,
    utils::{
        allocator::{Allocation, Allocator, AllocatorCreateInfo, MemoryTypeFinder},
        surface,
    },
    vk1_0::{
        self, ApplicationInfoBuilder, CommandBuffer, CommandBufferAllocateInfoBuilder,
        CommandBufferBeginInfoBuilder, CommandBufferLevel, CommandPool, Extent2D, Format,
        FormatFeatureFlags, Image, ImageTiling, ImageView, InstanceCreateInfoBuilder,
        PhysicalDevice, Queue, QueueFlags,
    },
    DeviceLoader, InstanceLoader,
};
use std::{
    ffi::{c_void, CStr, CString},
    os::raw::c_char,
    sync::{Arc, Mutex},
};
use winit::window::Window;

const LAYER_KHRONOS_VALIDATION: *const c_char = cstr!("VK_LAYER_KHRONOS_validation");

struct SwapChainSupportDetails {
    pub surface_caps: SurfaceCapabilitiesKHR,
    pub surface_formats: Vec<SurfaceFormatKHR>,
    pub present_modes: Vec<PresentModeKHR>,
}

struct QueueFamilyIndices {
    pub graphics_idx: Option<u32>,
    pub transfer_idx: Option<u32>,
}

pub struct RenderTexture {
    pub extent: Extent2D,
    pub image_view: ImageView,
    pub format: Format,
    image_memory: Option<Allocation<Image>>,
    context: Arc<VulkanContext>,
}
impl RenderTexture {
    fn destroy(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(Some(self.image_view), None);
        }
        let image_memory = self.image_memory.take();

        self.context.free_object(image_memory.unwrap());
    }
}

// impl Drop for RenderTexture {
//     fn drop(&mut self) {
//         self.destroy();
//     }
// }

pub struct VulkanContext {
    pub instance: InstanceLoader,
    pub device: DeviceLoader,
    pub physical_device: PhysicalDevice,
    pub allocator: Mutex<Allocator>,
    pub surface: SurfaceKHR,
    pub graphics_command_pool: CommandPool,
    pub graphics_queue: Queue,
    pub transfer_command_pool: CommandPool,
    pub transfer_queue: Queue,
    messenger: Option<ext_debug_utils::DebugUtilsMessengerEXT>,
}
pub struct VulkanFrameCtx {
    pub context: Arc<VulkanContext>,
    pub current_extent: Extent2D,
    pub current_surface_format: SurfaceFormatKHR,
    pub swapchain_image_views: Vec<ImageView>,
    pub swapchain: SwapchainKHR,
    pub swapchain_images: Vec<Image>,
    pub depth_render_texture: RenderTexture,
    pub command_buffers: Vec<CommandBuffer>,
}

impl SwapChainSupportDetails {
    pub fn choose_present_mode(&self) -> PresentModeKHR {
        self.present_modes
            .iter()
            .find(|format| match **format {
                PresentModeKHR::MAILBOX_KHR => true,
                _ => false,
            })
            .cloned()
            .unwrap_or(PresentModeKHR::FIFO_KHR)
    }

    pub fn choose_surface_format(&self) -> Option<SurfaceFormatKHR> {
        if self.surface_formats.is_empty() {
            None
        } else {
            for surface_format in &self.surface_formats {
                if surface_format.format == Format::B8G8R8A8_SRGB
                    && surface_format.color_space == ColorSpaceKHR::SRGB_NONLINEAR_KHR
                {
                    return Some(*surface_format);
                }
            }

            Some(self.surface_formats[0])
        }
    }

    pub unsafe fn query_swapchain_support(
        instance: &InstanceLoader,
        physical_device: PhysicalDevice,
        surface: SurfaceKHR,
    ) -> SwapChainSupportDetails {
        let surface_caps = instance
            .get_physical_device_surface_capabilities_khr(physical_device, surface, None)
            .unwrap();
        let surface_formats = instance
            .get_physical_device_surface_formats_khr(physical_device, surface, None)
            .unwrap();

        let present_modes = instance
            .get_physical_device_surface_present_modes_khr(physical_device, surface, None)
            .unwrap();

        SwapChainSupportDetails {
            surface_caps,
            surface_formats,
            present_modes,
        }
    }
}

impl QueueFamilyIndices {
    pub fn find_queue_families(
        instance: &InstanceLoader,
        surface: SurfaceKHR,
        physical_device: PhysicalDevice,
    ) -> Self {
        let mut queue_family_indices = Self {
            graphics_idx: None,
            transfer_idx: None,
        };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device, None);
            println!("Num family indices: {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                if properties.queue_flags.contains(QueueFlags::GRAPHICS)
                    && instance
                        .get_physical_device_surface_support_khr(
                            physical_device,
                            idx as u32,
                            surface,
                            None,
                        )
                        .unwrap()
                {
                    if queue_family_indices.graphics_idx.is_none() {
                        queue_family_indices.graphics_idx = Some(idx as u32);
                        continue;
                    }
                }

                if properties.queue_flags.contains(QueueFlags::TRANSFER)
                    && instance
                        .get_physical_device_surface_support_khr(
                            physical_device,
                            idx as u32,
                            surface,
                            None,
                        )
                        .unwrap()
                {
                    if queue_family_indices.transfer_idx.is_none() {
                        queue_family_indices.transfer_idx = Some(idx as u32);
                        continue;
                    }
                }
            }
        };
        println!(
            "Graphics idx {}, Transfer idx {}",
            queue_family_indices.graphics_idx.unwrap(),
            queue_family_indices.transfer_idx.unwrap()
        );

        queue_family_indices
    }
}

impl VulkanContext {
    pub fn allocate_object<'a, T>(
        &'a self,
        object: T,
        memory_type: MemoryTypeFinder,
    ) -> VulkanResult<Allocation<T>>
    where
        T: AllocationObject + 'static,
    {
        self.allocator
            .lock()
            .unwrap()
            .allocate(&self.device, object, memory_type)
    }

    pub fn free_object<T>(&self, object: Allocation<T>)
    where
        T: AllocationObject,
    {
        self.allocator.lock().unwrap().free(&self.device, object);
    }

    fn create_instance(
        with_validation_layers: bool,
        app_name: &CStr,
        engine_name: &CStr,
        window: &Window,
    ) -> InstanceLoader {
        if with_validation_layers && !check_validation_support() {
            panic!("Validation layers requested, but unavailable!");
        }

        let api_version = ENTRY_LOADER.lock().unwrap().instance_version();
        println!(
            "Mikpe erupt test: - Vulkan {}.{}.{}",
            vk1_0::version_major(api_version),
            vk1_0::version_minor(api_version),
            vk1_0::version_patch(api_version)
        );
        let mut instance_extensions = surface::enumerate_required_extensions(window).unwrap();
        let mut instance_layers = vec![];
        if with_validation_layers {
            instance_extensions.push(ext_debug_utils::EXT_DEBUG_UTILS_EXTENSION_NAME);
            instance_layers.push(LAYER_KHRONOS_VALIDATION);
        }
        let app_info = ApplicationInfoBuilder::new()
            .application_name(app_name)
            .application_version(vk1_0::make_version(1, 0, 0))
            .engine_name(engine_name)
            .engine_version(vk1_0::make_version(1, 0, 0))
            .api_version(vk1_0::make_version(1, 1, 0));

        let create_info = InstanceCreateInfoBuilder::new()
            .application_info(&app_info)
            .enabled_extension_names(&instance_extensions)
            .enabled_layer_names(&instance_layers);

        let instance =
            InstanceLoader::new(&ENTRY_LOADER.lock().unwrap(), &create_info, None).unwrap();
        instance
    }

    //https://vulkan-tutorial.com/Depth_buffering
    pub fn find_supported_format(
        &self,
        candidates: Vec<Format>,
        tiling: ImageTiling,
        features: FormatFeatureFlags,
    ) -> Format {
        let mut format = None;
        for candidate in candidates {
            let format_props = unsafe {
                self.instance.get_physical_device_format_properties(
                    self.physical_device,
                    candidate,
                    None,
                )
            };

            if tiling == ImageTiling::LINEAR
                && (format_props.linear_tiling_features & features) == features
            {
                format = Some(candidate);
                break;
            } else if tiling == ImageTiling::OPTIMAL
                && (format_props.optimal_tiling_features & features) == features
            {
                format = Some(candidate);
                break;
            }
        }

        format.expect("No acceptable format found!")
    }

    pub fn find_depth_format(&self) -> Format {
        let candidates = vec![
            Format::D32_SFLOAT_S8_UINT,
            Format::D32_SFLOAT,
            Format::D24_UNORM_S8_UINT,
        ];
        let tiling = ImageTiling::OPTIMAL;
        let features = FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT;
        self.find_supported_format(candidates, tiling, features)
    }

    pub fn pre_destroy(&self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
    }

    unsafe fn query_swapchain_support(&self) -> SwapChainSupportDetails {
        SwapChainSupportDetails::query_swapchain_support(
            &self.instance,
            self.physical_device,
            self.surface,
        )
    }

    //TODO: Make a per-thread command pool and queue for upload purposes!
    pub fn begin_single_time_commands(&self) -> CommandBuffer {
        let create_info = CommandBufferAllocateInfoBuilder::new()
            .level(CommandBufferLevel::PRIMARY)
            .command_pool(self.graphics_command_pool)
            .command_buffer_count(1);
        unsafe {
            let command_buffer: CommandBuffer =
                self.device.allocate_command_buffers(&create_info).unwrap()[0];
            let begin_info = CommandBufferBeginInfoBuilder::new()
                .flags(vk1_0::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .unwrap();
            command_buffer
        }
    }

    pub fn end_single_time_commands(&self, command_buffer: CommandBuffer) {
        unsafe {
            let command_buffers = vec![command_buffer];
            self.device.end_command_buffer(command_buffer).unwrap();
            let submit_info = vk1_0::SubmitInfoBuilder::new().command_buffers(&command_buffers);
            //TODO: This cannot be used by multiple frames at once,
            //ensure that we're either using another queue/commandpool, or
            //that we are doing this in a locked manner
            self.device
                .queue_submit(self.graphics_queue, &vec![submit_info], None)
                .unwrap();
            self.device.queue_wait_idle(self.graphics_queue).unwrap();
            self.device
                .free_command_buffers(self.graphics_command_pool, &command_buffers);
        }
    }

    //FIXME: this might be the incorrect way of doing this...
    pub fn begin_transfer_commands(&self) -> CommandBuffer {
        let create_info = CommandBufferAllocateInfoBuilder::new()
            .level(CommandBufferLevel::PRIMARY)
            .command_pool(self.transfer_command_pool)
            .command_buffer_count(1);
        unsafe {
            let command_buffer: CommandBuffer =
                self.device.allocate_command_buffers(&create_info).unwrap()[0];
            let begin_info = CommandBufferBeginInfoBuilder::new()
                .flags(vk1_0::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .unwrap();
            command_buffer
        }
    }

    pub fn end_transfer_commands(&self, command_buffer: CommandBuffer) {
        unsafe {
            let command_buffers = vec![command_buffer];
            self.device.end_command_buffer(command_buffer).unwrap();
            let submit_info = vk1_0::SubmitInfoBuilder::new().command_buffers(&command_buffers);
            //TODO: This cannot be used by multiple frames at once,
            //ensure that we're either using another queue/commandpool, or
            //that we are doing this in a locked manner
            self.device
                .queue_submit(self.transfer_queue, &vec![submit_info], None)
                .unwrap();
            self.device.queue_wait_idle(self.transfer_queue).unwrap();
            self.device
                .free_command_buffers(self.transfer_command_pool, &command_buffers);
        }
    }

    pub fn init(
        window: &Window,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let mut instance =
            Self::create_instance(with_validation_layers, &app_name, &engine_name, window);
        let messenger = create_debug_messenger(&mut instance, with_validation_layers);

        let surface = unsafe { surface::create_surface(&mut instance, window, None) }.unwrap();

        let physical_device = unsafe { pick_physical_device(&instance, surface) }.unwrap();

        let allocator = Mutex::new(
            //For high-dpi displays we need this to be larger, let's start with 64 MiB
            Allocator::new(
                &instance,
                physical_device,
                AllocatorCreateInfo {
                    block_size: 64 * 1024u64.pow(2),
                },
            )
            .unwrap(),
        );

        let queue_indices =
            QueueFamilyIndices::find_queue_families(&instance, surface, physical_device);

        let queue_create_infos = vec![
            vk1_0::DeviceQueueCreateInfoBuilder::new()
                .queue_family_index(queue_indices.graphics_idx.unwrap())
                .queue_priorities(&[1.0]),
            vk1_0::DeviceQueueCreateInfoBuilder::new()
                .queue_family_index(queue_indices.transfer_idx.unwrap())
                .queue_priorities(&[0.5]),
        ];
        let graphics_queue_idx = queue_indices.graphics_idx.unwrap();
        let transfer_queue_idx = queue_indices.transfer_idx.unwrap();

        let device = create_device(
            &instance,
            physical_device,
            queue_create_infos,
            with_validation_layers,
        );

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0, None) };

        let create_info = vk1_0::CommandPoolCreateInfoBuilder::new()
            .queue_family_index(graphics_queue_idx)
            .flags(vk1_0::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let graphics_command_pool =
            unsafe { device.create_command_pool(&create_info, None, None) }.unwrap();

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0, None) };
        let create_info = vk1_0::CommandPoolCreateInfoBuilder::new()
            .queue_family_index(transfer_queue_idx)
            .flags(vk1_0::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool =
            unsafe { device.create_command_pool(&create_info, None, None) }.unwrap();

        Self {
            instance,
            device,
            physical_device,
            allocator,
            surface,
            graphics_command_pool,
            graphics_queue,
            transfer_command_pool,
            transfer_queue,
            messenger,
        }
    }
}
impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device
                .destroy_command_pool(Some(self.graphics_command_pool), None);
            self.device.destroy_device(None);
            self.instance.destroy_surface_khr(Some(self.surface), None);

            if self.messenger.is_some() {
                self.instance
                    .destroy_debug_utils_messenger_ext(self.messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}

impl VulkanFrameCtx {
    pub fn create_image_view(
        device: &DeviceLoader,
        image: Image,
        format: Format,
        aspect_mask: vk1_0::ImageAspectFlags,
    ) -> ImageView {
        let subresource_range = vk1_0::ImageSubresourceRangeBuilder::new()
            .aspect_mask(aspect_mask)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let create_info = vk1_0::ImageViewCreateInfoBuilder::new()
            .image(image)
            .view_type(vk1_0::ImageViewType::_2D)
            .format(format)
            .components(vk1_0::ComponentMapping {
                r: vk1_0::ComponentSwizzle::IDENTITY,
                g: vk1_0::ComponentSwizzle::IDENTITY,
                b: vk1_0::ComponentSwizzle::IDENTITY,
                a: vk1_0::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(subresource_range.build());
        unsafe { device.create_image_view(&create_info, None, None) }.unwrap()
    }

    pub fn init(context: &Arc<VulkanContext>) -> Self {
        let swapchain_info = unsafe { context.query_swapchain_support() };

        let current_extent = swapchain_info.surface_caps.current_extent;
        let (swapchain, current_surface_format) =
            create_swapchain(&context.device, context.surface, &swapchain_info, None);

        let swapchain_images =
            unsafe { context.device.get_swapchain_images_khr(swapchain, None) }.unwrap();

        let swapchain_image_views: Vec<_> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                Self::create_image_view(
                    &context.device,
                    *swapchain_image,
                    current_surface_format.format,
                    vk1_0::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        let depth_render_texture = create_depth_render_texture(context.clone(), current_extent);

        let command_buffers = {
            let allocate_info = CommandBufferAllocateInfoBuilder::new()
                .command_pool(context.graphics_command_pool)
                .level(CommandBufferLevel::PRIMARY)
                .command_buffer_count(swapchain_image_views.len() as _);
            unsafe { context.device.allocate_command_buffers(&allocate_info) }.unwrap()
        };

        let ctx = Self {
            context: context.clone(),
            current_extent,
            current_surface_format,
            swapchain,
            swapchain_image_views,
            swapchain_images,
            depth_render_texture,
            command_buffers,
        };
        ctx
    }

    pub fn recreate_swapchain(&mut self) {
        let swapchain_info = unsafe { self.context.query_swapchain_support() };

        let (swapchain, current_surface_format) = create_swapchain(
            &self.context.device,
            self.context.surface,
            &swapchain_info,
            Some(self.swapchain),
        );
        self.destroy();
        self.current_extent = swapchain_info.surface_caps.current_extent;
        self.swapchain = swapchain;
        self.current_surface_format = current_surface_format;

        self.swapchain_images = unsafe {
            self.context
                .device
                .get_swapchain_images_khr(self.swapchain, None)
        }
        .unwrap();

        self.swapchain_image_views = self
            .swapchain_images
            .iter()
            .map(|swapchain_image| {
                Self::create_image_view(
                    &self.context.device,
                    *swapchain_image,
                    self.current_surface_format.format,
                    vk1_0::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        self.depth_render_texture =
            create_depth_render_texture(self.context.clone(), self.current_extent);
    }

    pub fn destroy(&mut self) {
        unsafe {
            for &image_view in &self.swapchain_image_views {
                self.context
                    .device
                    .destroy_image_view(Some(image_view), None);
            }
            self.context
                .device
                .destroy_swapchain_khr(Some(self.swapchain), None);
            self.depth_render_texture.destroy();
        }
    }
}

unsafe fn pick_physical_device(
    instance: &InstanceLoader,
    surface: SurfaceKHR,
) -> Option<PhysicalDevice> {
    let physical_devices = instance.enumerate_physical_devices(None).unwrap();

    let physical_device = physical_devices.into_iter().max_by_key(|physical_device| {
        is_physical_device_suitable(instance, *physical_device, surface)
    });
    if let Some(device) = physical_device {
        let properties = instance.get_physical_device_properties(device, None);
        println!(
            "Picking physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }
    physical_device
}

unsafe fn is_physical_device_suitable(
    instance: &InstanceLoader,
    physical_device: PhysicalDevice,
    surface: SurfaceKHR,
) -> u32 {
    let properties = instance.get_physical_device_properties(physical_device, None);
    let mut score = 0;

    match properties.device_type {
        vk1_0::PhysicalDeviceType::DISCRETE_GPU => score += 1000,
        vk1_0::PhysicalDeviceType::INTEGRATED_GPU => score += 100,
        vk1_0::PhysicalDeviceType::CPU => score += 10,
        _ => {}
    }

    score += properties.limits.max_image_dimension2_d;

    let swapchain_support =
        SwapChainSupportDetails::query_swapchain_support(instance, physical_device, surface);

    if swapchain_support.surface_formats.is_empty() && swapchain_support.present_modes.is_empty() {
        score = 0;
    }

    score
}

fn create_depth_render_texture(context: Arc<VulkanContext>, extent: Extent2D) -> RenderTexture {
    let depth_format = context.find_depth_format();
    let extent_3d = vk1_0::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let create_info = vk1_0::ImageCreateInfoBuilder::new()
        .image_type(vk1_0::ImageType::_2D)
        .mip_levels(1)
        .array_layers(1)
        .format(depth_format)
        .extent(extent_3d)
        .tiling(ImageTiling::OPTIMAL)
        .samples(vk1_0::SampleCountFlagBits::_1)
        .usage(vk1_0::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);

    //https://vulkan-tutorial.com/Depth_buffering
    let depth_image = unsafe {
        context
            .device
            .create_image(&create_info, None, None)
            .unwrap()
    };

    let image_memory = Some(
        context
            .allocate_object(depth_image, MemoryTypeFinder::gpu_only())
            .unwrap(),
    );
    let image_view = VulkanFrameCtx::create_image_view(
        &context.device,
        depth_image,
        depth_format,
        vk1_0::ImageAspectFlags::DEPTH,
    );
    RenderTexture {
        extent,
        image_view,
        image_memory,
        format: depth_format,
        context,
    }
}

fn create_device(
    instance: &InstanceLoader,
    physical_device: PhysicalDevice,
    queue_create_infos: Vec<vk1_0::DeviceQueueCreateInfoBuilder>,
    with_validation_layers: bool,
) -> DeviceLoader {
    let device_extensions = vec![KHR_SWAPCHAIN_EXTENSION_NAME];
    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION);
    }

    // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
    let features = vk1_0::PhysicalDeviceFeaturesBuilder::new().sampler_anisotropy(true);

    let create_info = vk1_0::DeviceCreateInfoBuilder::new()
        .enabled_extension_names(&device_extensions)
        .enabled_layer_names(&device_layers)
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&features);

    let device = DeviceLoader::new(&instance, physical_device, &create_info, None).unwrap();

    device
}

fn create_swapchain(
    device: &DeviceLoader,
    surface: SurfaceKHR,
    swapchain_info: &SwapChainSupportDetails,
    old_swapchain: Option<SwapchainKHR>,
) -> (SwapchainKHR, SurfaceFormatKHR) {
    let surface_caps = &swapchain_info.surface_caps;
    let format = swapchain_info.choose_surface_format().unwrap();

    let present_mode = swapchain_info.choose_present_mode();

    let current_extent = surface_caps.current_extent;

    let mut image_count = surface_caps.min_image_count + 1;

    if surface_caps.max_image_count > 0 && image_count > surface_caps.max_image_count {
        image_count = surface_caps.max_image_count;
    }
    let old_swapchain = old_swapchain.unwrap_or(SwapchainKHR::null());
    let create_info = SwapchainCreateInfoKHRBuilder::new()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(current_extent)
        .image_array_layers(1)
        .image_usage(vk1_0::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk1_0::SharingMode::EXCLUSIVE)
        .pre_transform(surface_caps.current_transform)
        .composite_alpha(CompositeAlphaFlagBitsKHR::OPAQUE_KHR)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain);
    let swapchain = unsafe { device.create_swapchain_khr(&create_info, None, None) }.unwrap();
    (swapchain, format)
}

fn create_debug_messenger(
    instance: &mut InstanceLoader,
    with_validation_layers: bool,
) -> Option<ext_debug_utils::DebugUtilsMessengerEXT> {
    if with_validation_layers {
        let create_info = ext_debug_utils::DebugUtilsMessengerCreateInfoEXTBuilder::new()
            .message_severity(
                ext_debug_utils::DebugUtilsMessageSeverityFlagsEXT::VERBOSE_EXT
                    | ext_debug_utils::DebugUtilsMessageSeverityFlagsEXT::WARNING_EXT
                    | ext_debug_utils::DebugUtilsMessageSeverityFlagsEXT::ERROR_EXT,
            )
            .message_type(
                ext_debug_utils::DebugUtilsMessageTypeFlagsEXT::GENERAL_EXT
                    | ext_debug_utils::DebugUtilsMessageTypeFlagsEXT::VALIDATION_EXT
                    | ext_debug_utils::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE_EXT,
            )
            .pfn_user_callback(Some(debug_callback));

        Some(
            unsafe { instance.create_debug_utils_messenger_ext(&create_info, None, None) }.unwrap(),
        )
    } else {
        None
    }
}

unsafe extern "system" fn debug_callback(
    _message_severity: ext_debug_utils::DebugUtilsMessageSeverityFlagBitsEXT,
    _message_types: ext_debug_utils::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const ext_debug_utils::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk1_0::Bool32 {
    println!(
        "{}",
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy()
    );

    vk1_0::FALSE
}

fn check_validation_support() -> bool {
    let mut layer_count = 0u32;
    let commands = &ENTRY_LOADER.lock().unwrap();
    unsafe {
        commands.enumerate_instance_layer_properties.unwrap()(&mut layer_count, 0 as _);
        let mut available_layers: Vec<vk1_0::LayerProperties> = Vec::new();
        available_layers.resize(layer_count as usize, vk1_0::LayerProperties::default());
        commands.enumerate_instance_layer_properties.unwrap()(
            &mut layer_count,
            available_layers.as_mut_ptr(),
        );
        let validation_name = std::ffi::CStr::from_ptr(LAYER_KHRONOS_VALIDATION as _);
        for layer in available_layers {
            let layer_name = std::ffi::CStr::from_ptr(layer.layer_name.as_ptr() as _);
            if layer_name == validation_name {
                return true;
            }
        }
    }

    return false;
}