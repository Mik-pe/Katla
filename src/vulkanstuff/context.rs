use super::CORE_LOADER;

use erupt::{
    cstr,
    extensions::{ext_debug_utils::*, khr_surface::*, khr_swapchain::*},
    utils::{
        allocator::{Allocation, Allocator, AllocatorCreateInfo, MemoryTypeFinder},
        surface,
    },
    vk1_0::*,
    DeviceLoader, InstanceLoader,
};
use std::{
    cell::RefCell,
    ffi::{c_void, CStr, CString},
    os::raw::c_char,
    rc::Rc,
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
}

pub struct RenderTexture {
    pub extent: Extent2D,
    pub image_view: ImageView,
    pub format: Format,
    image_memory: Option<Allocation<Image>>,
    context: Rc<VulkanContext>,
}

impl Drop for RenderTexture {
    fn drop(&mut self) {
        println!("Destroying depth image");
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view, None);
        }
        let image_memory = self.image_memory.take();

        self.context
            .allocator
            .borrow_mut()
            .free(&self.context.device, image_memory.unwrap());
    }
}

pub struct VulkanContext {
    pub instance: InstanceLoader,
    pub device: DeviceLoader,
    pub physical_device: PhysicalDevice,
    pub allocator: RefCell<Allocator>,
    pub surface: SurfaceKHR,
    pub command_pool: CommandPool,
    pub graphics_queue: Queue,
    messenger: Option<DebugUtilsMessengerEXT>,
}
pub struct VulkanFrameCtx {
    pub context: Rc<VulkanContext>,
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
        let mut queue_family_indices = Self { graphics_idx: None };
        unsafe {
            let family_props =
                instance.get_physical_device_queue_family_properties(physical_device, None);
            queue_family_indices.graphics_idx =
                match family_props
                    .into_iter()
                    .enumerate()
                    .position(|(i, properties)| {
                        properties.queue_flags.contains(QueueFlags::GRAPHICS)
                            && instance
                                .get_physical_device_surface_support_khr(
                                    physical_device,
                                    i as u32,
                                    surface,
                                    None,
                                )
                                .unwrap()
                                == true
                    }) {
                    Some(idx) => Some(idx as u32),
                    None => None,
                };
        };

        queue_family_indices
    }
}

impl VulkanContext {
    fn create_instance(
        with_validation_layers: bool,
        app_name: &CStr,
        engine_name: &CStr,
        window: &Window,
    ) -> InstanceLoader {
        if with_validation_layers && !check_validation_support() {
            panic!("Validation layers requested, but unavailable!");
        }

        let api_version = CORE_LOADER.lock().unwrap().instance_version();
        println!(
            "Mikpe erupt test: - Vulkan {}.{}.{}",
            erupt::version_major(api_version),
            erupt::version_minor(api_version),
            erupt::version_patch(api_version)
        );
        let mut instance_extensions = surface::enumerate_required_extensions(window).unwrap();
        let mut instance_layers = vec![];
        if with_validation_layers {
            instance_extensions.push(EXT_DEBUG_UTILS_EXTENSION_NAME);
            instance_layers.push(LAYER_KHRONOS_VALIDATION);
        }
        let app_info = ApplicationInfoBuilder::new()
            .application_name(app_name)
            .application_version(erupt::make_version(1, 0, 0))
            .engine_name(engine_name)
            .engine_version(erupt::make_version(1, 0, 0))
            .api_version(erupt::make_version(1, 1, 0));

        let create_info = InstanceCreateInfoBuilder::new()
            .application_info(&app_info)
            .enabled_extension_names(&instance_extensions)
            .enabled_layer_names(&instance_layers);
        let instance = unsafe {
            CORE_LOADER
                .lock()
                .unwrap()
                .create_instance(&create_info, None, None)
        }
        .unwrap();
        let mut instance = InstanceLoader::new(&CORE_LOADER.lock().unwrap(), instance).unwrap();
        instance.load_vk1_0().unwrap();
        instance.load_vk1_1().unwrap();
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

        dbg!(format);
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

    pub fn begin_single_time_commands(&self) -> CommandBuffer {
        let create_info = CommandBufferAllocateInfoBuilder::new()
            .level(CommandBufferLevel::PRIMARY)
            .command_pool(self.command_pool)
            .command_buffer_count(1);
        unsafe {
            let command_buffer: CommandBuffer =
                self.device.allocate_command_buffers(&create_info).unwrap()[0];
            let begin_info = CommandBufferBeginInfoBuilder::new()
                .flags(CommandBufferUsageFlags::ONE_TIME_SUBMIT);
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
            let submit_info = SubmitInfoBuilder::new().command_buffers(&command_buffers);
            self.device
                .queue_submit(self.graphics_queue, &vec![submit_info], Fence::null())
                .unwrap();
            self.device.queue_wait_idle(self.graphics_queue).unwrap();
            self.device
                .free_command_buffers(self.command_pool, &command_buffers);
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

        let allocator = RefCell::new(
            Allocator::new(&instance, physical_device, AllocatorCreateInfo::default()).unwrap(),
        );

        let queue_indices =
            QueueFamilyIndices::find_queue_families(&instance, surface, physical_device);

        let graphics_queue_idx = queue_indices.graphics_idx.unwrap();

        let device = create_device(
            &instance,
            physical_device,
            graphics_queue_idx,
            with_validation_layers,
        );

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0, None) };

        let create_info = CommandPoolCreateInfoBuilder::new()
            .queue_family_index(graphics_queue_idx)
            .flags(CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&create_info, None, None) }.unwrap();

        Self {
            instance,
            device,
            physical_device,
            allocator,
            surface,
            command_pool,
            graphics_queue,
            messenger,
        }
    }
}
impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_surface_khr(self.surface, None);

            if self.messenger.is_some() {
                self.instance
                    .destroy_debug_utils_messenger_ext(self.messenger.unwrap(), None);
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
        aspect_mask: ImageAspectFlags,
    ) -> ImageView {
        let create_info = ImageViewCreateInfoBuilder::new()
            .image(image)
            .view_type(ImageViewType::_2D)
            .format(format)
            .components(ComponentMapping {
                r: ComponentSwizzle::IDENTITY,
                g: ComponentSwizzle::IDENTITY,
                b: ComponentSwizzle::IDENTITY,
                a: ComponentSwizzle::IDENTITY,
            })
            .subresource_range(unsafe {
                ImageSubresourceRangeBuilder::new()
                    .aspect_mask(aspect_mask)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .discard()
            });
        unsafe { device.create_image_view(&create_info, None, None) }.unwrap()
    }

    pub fn init(context: &Rc<VulkanContext>) -> Self {
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
                    ImageAspectFlags::COLOR,
                )
            })
            .collect();
        let depth_render_texture = create_depth_render_texture(context.clone(), current_extent);

        let command_buffers = {
            let allocate_info = CommandBufferAllocateInfoBuilder::new()
                .command_pool(context.command_pool)
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

        let current_extent = swapchain_info.surface_caps.current_extent;
        let (swapchain, current_surface_format) = create_swapchain(
            &self.context.device,
            self.context.surface,
            &swapchain_info,
            Some(self.swapchain),
        );
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
                    ImageAspectFlags::COLOR,
                )
            })
            .collect();
        self.depth_render_texture =
            create_depth_render_texture(self.context.clone(), current_extent);
    }

    pub fn destroy(&mut self) {
        unsafe {
            for &image_view in &self.swapchain_image_views {
                self.context.device.destroy_image_view(image_view, None);
            }
            self.context
                .device
                .destroy_swapchain_khr(self.swapchain, None);
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
        PhysicalDeviceType::DISCRETE_GPU => score += 1000,
        PhysicalDeviceType::INTEGRATED_GPU => score += 100,
        PhysicalDeviceType::CPU => score += 10,
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

fn create_depth_render_texture(context: Rc<VulkanContext>, extent: Extent2D) -> RenderTexture {
    let depth_format = context.find_depth_format();
    let extent_3d = Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let create_info = ImageCreateInfoBuilder::new()
        .image_type(ImageType::_2D)
        .mip_levels(1)
        .array_layers(1)
        .format(depth_format)
        .extent(extent_3d)
        .tiling(ImageTiling::OPTIMAL)
        .samples(SampleCountFlagBits::_1)
        .usage(ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT);

    //https://vulkan-tutorial.com/Depth_buffering
    let depth_image = unsafe {
        context
            .device
            .create_image(&create_info, None, None)
            .unwrap()
    };

    let image_memory = Some(
        context
            .allocator
            .borrow_mut()
            .allocate(&context.device, depth_image, MemoryTypeFinder::gpu_only())
            .unwrap(),
    );
    let image_view = VulkanFrameCtx::create_image_view(
        &context.device,
        depth_image,
        depth_format,
        ImageAspectFlags::DEPTH,
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
    graphics_queue: u32,
    with_validation_layers: bool,
) -> DeviceLoader {
    let device_extensions = vec![KHR_SWAPCHAIN_EXTENSION_NAME];
    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION);
    }

    // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
    let queue_create_info = vec![DeviceQueueCreateInfoBuilder::new()
        .queue_family_index(graphics_queue)
        .queue_priorities(&[1.0])];
    let features = PhysicalDeviceFeaturesBuilder::new().sampler_anisotropy(true);

    let create_info = DeviceCreateInfoBuilder::new()
        .enabled_extension_names(&device_extensions)
        .enabled_layer_names(&device_layers)
        .queue_create_infos(&queue_create_info)
        .enabled_features(&features);

    let mut device = DeviceLoader::new(
        &instance,
        unsafe { instance.create_device(physical_device, &create_info, None, None) }.unwrap(),
    )
    .unwrap();
    device.load_vk1_0().unwrap();
    device.load_vk1_1().unwrap();
    device
        .load_khr_swapchain()
        .expect("Couldn't load swapchain!");
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
    println!("Swapchain extent: {:?}", current_extent);

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
        .image_usage(ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(SharingMode::EXCLUSIVE)
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
) -> Option<DebugUtilsMessengerEXT> {
    if with_validation_layers {
        instance.load_ext_debug_utils().unwrap();

        let create_info = DebugUtilsMessengerCreateInfoEXTBuilder::new()
            .message_severity(
                DebugUtilsMessageSeverityFlagsEXT::VERBOSE_EXT
                    | DebugUtilsMessageSeverityFlagsEXT::WARNING_EXT
                    | DebugUtilsMessageSeverityFlagsEXT::ERROR_EXT,
            )
            .message_type(
                DebugUtilsMessageTypeFlagsEXT::GENERAL_EXT
                    | DebugUtilsMessageTypeFlagsEXT::VALIDATION_EXT
                    | DebugUtilsMessageTypeFlagsEXT::PERFORMANCE_EXT,
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
    _message_severity: DebugUtilsMessageSeverityFlagBitsEXT,
    _message_types: DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> Bool32 {
    println!(
        "{}",
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy()
    );

    FALSE
}

fn check_validation_support() -> bool {
    let mut layer_count = 0u32;
    let commands = Vk10CoreCommands::load(&CORE_LOADER.lock().unwrap()).unwrap();
    unsafe {
        commands.enumerate_instance_layer_properties.unwrap()(&mut layer_count, 0 as _);
        let mut available_layers: Vec<LayerProperties> = Vec::new();
        available_layers.resize(layer_count as usize, LayerProperties::default());
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
