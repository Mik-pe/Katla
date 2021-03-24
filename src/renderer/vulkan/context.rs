use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0};

use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain},
    },
    vk, Device, Entry, Instance,
};
use vk_mem::{Allocation, Allocator};

use std::{
    ffi::{c_void, CStr, CString},
    sync::{Arc, Mutex},
};
use winit::window::Window;

const LAYER_KHRONOS_VALIDATION: &str = concat!("VK_LAYER_KHRONOS_validation", "\0");

struct SwapChainSupportDetails {
    pub surface_caps: vk::SurfaceCapabilitiesKHR,
    pub surface_formats: Vec<vk::SurfaceFormatKHR>,
    pub present_modes: Vec<vk::PresentModeKHR>,
}

struct QueueFamilyIndices {
    pub graphics_idx: Option<u32>,
    pub transfer_idx: Option<u32>,
}

pub struct RenderTexture {
    pub extent: vk::Extent2D,
    pub image_view: vk::ImageView,
    pub format: vk::Format,
    image: vk::Image,
    image_memory: Option<Allocation>,
    context: Arc<VulkanContext>,
}

impl RenderTexture {
    fn destroy(&mut self) {
        unsafe {
            self.context
                .device
                .destroy_image_view(self.image_view, None);
        }
        let image_memory = self.image_memory.take();

        self.context.free_image(self.image, &image_memory.unwrap());
    }
}

impl Drop for RenderTexture {
    fn drop(&mut self) {
        self.destroy();
    }
}

pub struct VulkanContext {
    entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub surface_loader: Surface,
    pub swapchain_loader: Swapchain,
    pub physical_device: vk::PhysicalDevice,
    pub allocator: Allocator,
    pub surface: vk::SurfaceKHR,
    pub graphics_command_pool: vk::CommandPool,
    pub graphics_queue: vk::Queue,
    pub transfer_command_pool: vk::CommandPool,
    pub transfer_queue: vk::Queue,
    debug_utils_loader: DebugUtils,
    debug_callback: Option<vk::DebugUtilsMessengerEXT>,
}
pub struct VulkanFrameCtx {
    pub context: Arc<VulkanContext>,
    pub current_extent: vk::Extent2D,
    pub current_surface_format: vk::SurfaceFormatKHR,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub depth_render_texture: RenderTexture,
    pub command_buffers: Vec<vk::CommandBuffer>,
}

impl SwapChainSupportDetails {
    pub fn choose_present_mode(&self) -> vk::PresentModeKHR {
        self.present_modes
            .iter()
            .find(|format| match **format {
                vk::PresentModeKHR::MAILBOX => true,
                _ => false,
            })
            .cloned()
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }

    pub fn choose_surface_format(&self) -> Option<vk::SurfaceFormatKHR> {
        if self.surface_formats.is_empty() {
            None
        } else {
            for surface_format in &self.surface_formats {
                if surface_format.format == vk::Format::B8G8R8A8_SRGB
                    && surface_format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                {
                    return Some(*surface_format);
                }
            }

            Some(self.surface_formats[0])
        }
    }

    pub unsafe fn query_swapchain_support(
        surface_loader: &Surface,
        physical_device: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> SwapChainSupportDetails {
        let surface_caps = surface_loader
            .get_physical_device_surface_capabilities(physical_device, surface)
            .unwrap();
        let surface_formats = surface_loader
            .get_physical_device_surface_formats(physical_device, surface)
            .unwrap();
        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)
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
        instance: &Instance,
        surface_loader: &Surface,
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
            println!("Num family indices: {}", family_props.len());
            for (idx, properties) in family_props.iter().enumerate() {
                if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && surface_loader
                        .get_physical_device_surface_support(physical_device, idx as u32, surface)
                        .unwrap()
                {
                    if queue_family_indices.graphics_idx.is_none() {
                        queue_family_indices.graphics_idx = Some(idx as u32);
                        continue;
                    }
                }

                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER)
                    && surface_loader
                        .get_physical_device_surface_support(physical_device, idx as u32, surface)
                        .unwrap()
                {
                    if queue_family_indices.transfer_idx.is_none() {
                        queue_family_indices.transfer_idx = Some(idx as u32);
                        continue;
                    }
                }
            }
        };

        queue_family_indices
    }
}

impl VulkanContext {
    pub fn allocate_buffer(
        &self,
        buffer_info: &vk::BufferCreateInfo,
        usage: vk_mem::MemoryUsage,
    ) -> (vk::Buffer, vk_mem::Allocation) {
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage,
            ..Default::default()
        };
        let (buffer, allocation, _) = self
            .allocator
            .create_buffer(buffer_info, &allocation_info)
            .unwrap();
        (buffer, allocation)
    }

    pub fn free_buffer(&self, buffer: vk::Buffer, allocation: vk_mem::Allocation) {
        self.allocator
            .destroy_buffer(buffer, &allocation)
            .expect("Could not destroy buffer!");
    }

    //TODO: Enable mapping of part of buffers
    pub fn map_buffer(&self, allocation: &vk_mem::Allocation) -> *mut u8 {
        self.allocator.map_memory(allocation).unwrap()
    }

    pub fn unmap_buffer(&self, allocation: &vk_mem::Allocation) {
        self.allocator
            .unmap_memory(allocation)
            .expect("Could not unmap memory!");
    }

    pub fn create_image(
        &self,
        image_create_info: vk::ImageCreateInfo,
        usage: vk_mem::MemoryUsage,
    ) -> (vk::Image, vk_mem::Allocation) {
        let allocation_info = vk_mem::AllocationCreateInfo {
            usage,
            ..Default::default()
        };
        let (image, allocation, _) = self
            .allocator
            .create_image(&image_create_info, &allocation_info)
            .unwrap();

        (image, allocation)
    }

    pub fn free_image(&self, image: vk::Image, allocation: &vk_mem::Allocation) {
        self.allocator
            .destroy_image(image, allocation)
            .expect("Could not free image!");
    }

    fn create_instance(
        with_validation_layers: bool,
        app_name: &CStr,
        engine_name: &CStr,
        window: &Window,
        entry: &Entry,
    ) -> Instance {
        if with_validation_layers && !check_validation_support(entry) {
            panic!("Validation layers requested, but unavailable!");
        }
        let surface_extensions = ash_window::enumerate_required_extensions(window).unwrap();
        let mut extension_names_raw = surface_extensions
            .iter()
            .map(|ext| ext.as_ptr())
            .collect::<Vec<_>>();
        let mut instance_layers = vec![];
        if with_validation_layers {
            extension_names_raw.push(DebugUtils::name().as_ptr());
            instance_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        }
        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name)
            .application_version(0)
            .engine_name(engine_name)
            .engine_version(0)
            .api_version(vk::make_version(1, 1, 0));

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names_raw)
            .enabled_layer_names(&instance_layers);

        let instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Vk Instance creation error")
        };

        instance
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

            if tiling == vk::ImageTiling::LINEAR
                && (format_props.linear_tiling_features & features) == features
            {
                format = Some(candidate);
                break;
            } else if tiling == vk::ImageTiling::OPTIMAL
                && (format_props.optimal_tiling_features & features) == features
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

    unsafe fn query_swapchain_support(&self) -> SwapChainSupportDetails {
        SwapChainSupportDetails::query_swapchain_support(
            &self.surface_loader,
            self.physical_device,
            self.surface,
        )
    }

    //TODO: Make a per-thread command pool and queue for upload purposes!
    pub fn begin_single_time_commands(&self) -> vk::CommandBuffer {
        let create_info = vk::CommandBufferAllocateInfo::builder()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(self.graphics_command_pool)
            .command_buffer_count(1);
        unsafe {
            let command_buffer: vk::CommandBuffer =
                self.device.allocate_command_buffers(&create_info).unwrap()[0];
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .unwrap();
            command_buffer
        }
    }

    pub fn end_single_time_commands(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            let command_buffers = vec![command_buffer];
            self.device.end_command_buffer(command_buffer).unwrap();
            let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);
            //TODO: This cannot be used by multiple frames at once,
            //ensure that we're either using another queue/commandpool, or
            //that we are doing this in a locked manner
            self.device
                .queue_submit(
                    self.graphics_queue,
                    &[submit_info.build()],
                    vk::Fence::null(),
                )
                .unwrap();
            self.device.queue_wait_idle(self.graphics_queue).unwrap();
            self.device
                .free_command_buffers(self.graphics_command_pool, &command_buffers);
        }
    }

    //FIXME: this might be the incorrect way of doing this...
    pub fn begin_transfer_commands(&self) -> vk::CommandBuffer {
        let create_info = vk::CommandBufferAllocateInfo::builder()
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_pool(self.transfer_command_pool)
            .command_buffer_count(1);
        unsafe {
            let command_buffer: vk::CommandBuffer =
                self.device.allocate_command_buffers(&create_info).unwrap()[0];
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .unwrap();
            command_buffer
        }
    }

    pub fn end_transfer_commands(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            let command_buffers = vec![command_buffer];
            self.device.end_command_buffer(command_buffer).unwrap();
            let submit_info = vk::SubmitInfo::builder().command_buffers(&command_buffers);
            //TODO: This cannot be used by multiple frames at once,
            //ensure that we're either using another queue/commandpool, or
            //that we are doing this in a locked manner
            self.device
                .queue_submit(
                    self.transfer_queue,
                    &[submit_info.build()],
                    vk::Fence::null(),
                )
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
        let entry = unsafe { Entry::new() }.unwrap();
        let instance = Self::create_instance(
            with_validation_layers,
            &app_name,
            &engine_name,
            window,
            &entry,
        );
        let debug_utils_loader = DebugUtils::new(&entry, &instance);
        let debug_callback = create_debug_messenger(&debug_utils_loader, with_validation_layers);
        let surface_loader = Surface::new(&entry, &instance);
        let surface =
            unsafe { ash_window::create_surface(&entry, &instance, window, None) }.unwrap();

        let physical_device =
            unsafe { pick_physical_device(&instance, &surface_loader, surface) }.unwrap();

        let queue_indices = QueueFamilyIndices::find_queue_families(
            &instance,
            &surface_loader,
            surface,
            physical_device,
        );

        let queue_create_infos = vec![
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(queue_indices.graphics_idx.unwrap())
                .queue_priorities(&[1.0])
                .build(),
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
        );

        let swapchain_loader = Swapchain::new(&instance, &device);

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(graphics_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let graphics_command_pool =
            unsafe { device.create_command_pool(&create_info, None) }.unwrap();

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool =
            unsafe { device.create_command_pool(&create_info, None) }.unwrap();

        //TODO: Read up on the actual fields in this CreateInfo
        let create_info = vk_mem::AllocatorCreateInfo {
            physical_device,
            device: device.clone(),
            instance: instance.clone(),
            //FIXME: Replace following with  ..Default::default() once vk_mem-rs bumps to 0.2.3
            flags: vk_mem::AllocatorCreateFlags::NONE,
            preferred_large_heap_block_size: 0,
            frame_in_use_count: 0,
            heap_size_limits: None,
        };

        let allocator = vk_mem::Allocator::new(&create_info).unwrap();

        Self {
            entry,
            instance,
            device,
            surface_loader,
            swapchain_loader,
            physical_device,
            allocator,
            surface,
            graphics_command_pool,
            graphics_queue,
            transfer_command_pool,
            transfer_queue,
            debug_utils_loader,
            debug_callback,
        }
    }
}
impl Drop for VulkanContext {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.device
                .destroy_command_pool(self.graphics_command_pool, None);
            self.device
                .destroy_command_pool(self.transfer_command_pool, None);
            self.allocator.destroy();
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);

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
        let subresource_range = vk::ImageSubresourceRange::builder()
            .aspect_mask(aspect_mask)
            .base_mip_level(0)
            .level_count(1)
            .base_array_layer(0)
            .layer_count(1);
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            })
            .subresource_range(subresource_range.build());
        unsafe { device.create_image_view(&create_info, None) }.unwrap()
    }

    pub fn init(context: &Arc<VulkanContext>) -> Self {
        let swapchain_info = unsafe { context.query_swapchain_support() };

        let current_extent = swapchain_info.surface_caps.current_extent;
        let (swapchain, current_surface_format) = create_swapchain(
            &context.swapchain_loader,
            context.surface,
            &swapchain_info,
            None,
        );

        let swapchain_images =
            unsafe { context.swapchain_loader.get_swapchain_images(swapchain) }.unwrap();

        let swapchain_image_views: Vec<_> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                Self::create_image_view(
                    &context.device,
                    *swapchain_image,
                    current_surface_format.format,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        let depth_render_texture = create_depth_render_texture(context.clone(), current_extent);

        let command_buffers = {
            let allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_pool(context.graphics_command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
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
            &self.context.swapchain_loader,
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
                .swapchain_loader
                .get_swapchain_images(self.swapchain)
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
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        self.depth_render_texture =
            create_depth_render_texture(self.context.clone(), self.current_extent);
    }

    pub fn destroy(&mut self) {
        unsafe {
            for &image_view in &self.swapchain_image_views {
                self.context.device.destroy_image_view(image_view, None);
            }
            self.context
                .swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            // self.depth_render_texture.destroy();
        }
    }
}

unsafe fn pick_physical_device(
    instance: &Instance,
    surface_loader: &Surface,
    surface: vk::SurfaceKHR,
) -> Option<vk::PhysicalDevice> {
    let physical_devices = instance.enumerate_physical_devices().unwrap();

    let physical_device = physical_devices.into_iter().max_by_key(|physical_device| {
        is_physical_device_suitable(instance, surface_loader, *physical_device, surface)
    });
    if let Some(device) = physical_device {
        let properties = instance.get_physical_device_properties(device);
        println!(
            "Picking physical device: {:?}",
            CStr::from_ptr(properties.device_name.as_ptr())
        );
    }
    physical_device
}

unsafe fn is_physical_device_suitable(
    instance: &Instance,
    surface_loader: &Surface,
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
        SwapChainSupportDetails::query_swapchain_support(surface_loader, physical_device, surface);

    if swapchain_support.surface_formats.is_empty() && swapchain_support.present_modes.is_empty() {
        score = 0;
    }

    score
}

fn create_depth_render_texture(context: Arc<VulkanContext>, extent: vk::Extent2D) -> RenderTexture {
    let depth_format = context.find_depth_format();
    let extent_3d = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let create_info = vk::ImageCreateInfo::builder()
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
        context.create_image(create_info.build(), vk_mem::MemoryUsage::GpuOnly);

    let image_view = VulkanFrameCtx::create_image_view(
        &context.device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH,
    );
    RenderTexture {
        extent,
        image_view,
        image: depth_image,
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
) -> Device {
    let device_extensions = [Swapchain::name().as_ptr()];
    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
    }

    // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
    let features = vk::PhysicalDeviceFeatures::builder().sampler_anisotropy(true);

    let create_info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&device_extensions)
        .enabled_layer_names(&device_layers)
        .queue_create_infos(&queue_create_infos)
        .enabled_features(&features);
    let device = unsafe {
        instance
            .create_device(physical_device, &create_info, None)
            .unwrap()
    };

    device
}

fn create_swapchain(
    swapchain_loader: &Swapchain,
    surface: vk::SurfaceKHR,
    swapchain_info: &SwapChainSupportDetails,
    old_swapchain: Option<vk::SwapchainKHR>,
) -> (vk::SwapchainKHR, vk::SurfaceFormatKHR) {
    let surface_caps = &swapchain_info.surface_caps;
    let format = swapchain_info.choose_surface_format().unwrap();

    let present_mode = swapchain_info.choose_present_mode();

    let current_extent = surface_caps.current_extent;

    let mut image_count = surface_caps.min_image_count + 1;

    if surface_caps.max_image_count > 0 && image_count > surface_caps.max_image_count {
        image_count = surface_caps.max_image_count;
    }
    let old_swapchain = old_swapchain.unwrap_or(vk::SwapchainKHR::null());
    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(current_extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(surface_caps.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain);
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None) }.unwrap();
    (swapchain, format)
}

fn create_debug_messenger(
    debug_utils_loader: &DebugUtils,
    with_validation_layers: bool,
) -> Option<vk::DebugUtilsMessengerEXT> {
    if with_validation_layers {
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback));

        Some(
            unsafe { debug_utils_loader.create_debug_utils_messenger(&create_info, None) }.unwrap(),
        )
    } else {
        None
    }
}

unsafe extern "system" fn debug_callback(
    _message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    println!(
        "{}",
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy()
    );

    vk::FALSE
}

fn check_validation_support(entry: &Entry) -> bool {
    unsafe {
        let available_layers = entry.enumerate_instance_layer_properties().unwrap();
        let validation_name = CStr::from_ptr(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
        println!("Validation name: {:?}", validation_name);
        for layer in available_layers {
            let layer_name = std::ffi::CStr::from_ptr(layer.layer_name.as_ptr() as _);
            if layer_name == validation_name {
                return true;
            }
        }
    }

    return false;
}
