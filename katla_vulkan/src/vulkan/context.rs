use ash::{
    ext::debug_utils::Instance as DebugInstance,
    khr::{surface::Instance as SurfaceInstance, swapchain::Device as SwapchainDevice},
    vk::{self},
    Device, Entry, Instance,
};
use gpu_allocator::{
    vulkan::{Allocation, AllocationScheme, Allocator, AllocatorCreateDesc},
    AllocationSizes, AllocatorDebugSettings,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    cell::RefCell,
    ffi::{c_void, CStr, CString},
    mem::ManuallyDrop,
    sync::Arc,
};
// use winit::{
//     raw_window_handle::{HasDisplayHandle, HasRawWindowHandle, HasWindowHandle},
//     window::Window,
// };

use super::SwapchainInfo;

const LAYER_KHRONOS_VALIDATION: &str = concat!("VK_LAYER_KHRONOS_validation", "\0");

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

        self.context.free_image(self.image, image_memory.unwrap());
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
    pub surface_loader: SurfaceInstance,
    pub swapchain_loader: Arc<SwapchainDevice>,
    pub physical_device: vk::PhysicalDevice,
    pub allocator: ManuallyDrop<RefCell<Allocator>>,
    pub surface: vk::SurfaceKHR,
    pub graphics_queue: vk::Queue,
    pub gfx_queue: super::Queue,
    pub gfx_cmdpool: super::CommandPool,
    pub transfer_command_pool: vk::CommandPool,
    pub transfer_queue: vk::Queue,
    debug_utils_loader: DebugInstance,
    debug_callback: Option<vk::DebugUtilsMessengerEXT>,
}
pub struct VulkanFrameCtx {
    pub context: Arc<VulkanContext>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain: super::Swapchain,
    pub swapchain_images: Vec<vk::Image>,
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
        location: gpu_allocator::MemoryLocation,
    ) -> (vk::Buffer, Allocation) {
        let buffer = unsafe { self.device.create_buffer(&buffer_info, None) }.unwrap();
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        //TODO: Find better names...
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

    pub fn free_buffer(&self, buffer: vk::Buffer, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        allocator.free(allocation).unwrap();
        unsafe { self.device.destroy_buffer(buffer, None) };
    }

    //TODO: Enable mapping of part of buffers
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

    pub fn free_image(&self, image: vk::Image, allocation: Allocation) {
        let mut allocator = self.allocator.borrow_mut();
        allocator.free(allocation).unwrap();
        unsafe {
            self.device.destroy_image(image, None);
        }
    }

    fn create_instance(
        with_validation_layers: bool,
        app_name: &CStr,
        engine_name: &CStr,
        display: &dyn HasDisplayHandle,
        entry: &Entry,
    ) -> Instance {
        if with_validation_layers && !check_validation_support(entry) {
            panic!("Validation layers requested, but unavailable!");
        }
        let surface_extensions =
            ash_window::enumerate_required_extensions(display.display_handle().unwrap().as_raw())
                .unwrap();
        let mut extension_names_raw = surface_extensions
            .iter()
            .map(|ext| *ext)
            .collect::<Vec<_>>();
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
            .api_version(vk::make_api_version(0, 1, 2, 0));
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names_raw.as_slice())
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

    //TODO: Make a per-thread command pool and queue for upload purposes!
    pub fn begin_single_time_commands(&self) -> super::CommandBuffer {
        let command_buffer = super::CommandBuffer::new(&self.device, &self.gfx_cmdpool);
        command_buffer.begin_single_time_command();
        command_buffer
    }

    pub fn end_single_time_commands(&self, command_buffer: super::CommandBuffer) {
        command_buffer.end_single_time_command();
        let command_buffers = vec![&command_buffer];
        //TODO: This cannot be used by multiple frames at once,
        //ensure that we're either using another queue/commandpool, or
        //that we are doing this in a locked manner
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
            display,
            &entry,
        );
        let debug_utils_loader = DebugInstance::new(&entry, &instance);
        let debug_callback = create_debug_messenger(&debug_utils_loader, with_validation_layers);
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
        );

        let swapchain_loader = Arc::new(SwapchainDevice::new(&instance, &device));

        let graphics_queue = unsafe { device.get_device_queue(graphics_queue_idx, 0) };

        let gfx_queue = super::Queue::new(device.clone(), graphics_queue_idx, 0);
        let gfx_cmdpool = super::CommandPool::new(device.clone(), graphics_queue_idx);

        let transfer_queue = unsafe { device.get_device_queue(transfer_queue_idx, 0) };
        let create_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(transfer_queue_idx)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let transfer_command_pool =
            unsafe { device.create_command_pool(&create_info, None) }.unwrap();

        let debug_settings = AllocatorDebugSettings {
            log_leaks_on_shutdown: true,
            ..Default::default()
        };
        let create_info = AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings,
            buffer_device_address: false,
            allocation_sizes: AllocationSizes::default(),
        };

        let allocator = ManuallyDrop::new(RefCell::new(Allocator::new(&create_info).unwrap()));

        Self {
            entry,
            instance,
            device,
            surface_loader,
            swapchain_loader,
            physical_device,
            allocator,
            surface,
            graphics_queue,
            gfx_queue,
            gfx_cmdpool,
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
                .destroy_command_pool(self.transfer_command_pool, None);
            self.gfx_cmdpool.destroy();
            ManuallyDrop::drop(&mut self.allocator);
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

    pub fn init(context: &Arc<VulkanContext>) -> Self {
        let swapchain = super::Swapchain::create_swapchain(
            context.swapchain_loader.clone(),
            &context.surface_loader,
            context.physical_device,
            context.surface,
            None,
        );

        let swapchain_images = swapchain.get_swapchain_images();

        let swapchain_image_views: Vec<_> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                Self::create_image_view(
                    &context.device,
                    *swapchain_image,
                    swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        let depth_render_texture =
            create_depth_render_texture(context.clone(), swapchain.get_extent());

        let command_buffers = context
            .gfx_cmdpool
            .create_command_buffers(swapchain_image_views.len() as _);

        let ctx = Self {
            context: context.clone(),
            swapchain,
            swapchain_image_views,
            swapchain_images,
            depth_render_texture,
            command_buffers,
        };
        ctx
    }

    pub fn recreate_swapchain(&mut self) {
        let swapchain = super::Swapchain::create_swapchain(
            self.context.swapchain_loader.clone(),
            &self.context.surface_loader,
            self.context.physical_device,
            self.context.surface,
            Some(self.swapchain.swapchain),
        );
        self.destroy();
        self.swapchain = swapchain;

        self.swapchain_images = self.swapchain.get_swapchain_images();

        self.swapchain_image_views = self
            .swapchain_images
            .iter()
            .map(|swapchain_image| {
                Self::create_image_view(
                    &self.context.device,
                    *swapchain_image,
                    self.swapchain.format.format,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect();
        self.depth_render_texture =
            create_depth_render_texture(self.context.clone(), self.swapchain.get_extent());
    }

    pub fn destroy(&mut self) {
        unsafe {
            for &image_view in &self.swapchain_image_views {
                self.context.device.destroy_image_view(image_view, None);
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
        println!(
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

fn create_depth_render_texture(context: Arc<VulkanContext>, extent: vk::Extent2D) -> RenderTexture {
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
    let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];
    let mut device_layers = vec![];
    if with_validation_layers {
        device_layers.push(LAYER_KHRONOS_VALIDATION.as_ptr() as *const i8);
    }

    // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
    let features = vk::PhysicalDeviceFeatures {
        sampler_anisotropy: 1,
        ..Default::default()
    };

    let create_info = vk::DeviceCreateInfo::default()
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

fn create_debug_messenger(
    debug_utils_loader: &DebugInstance,
    with_validation_layers: bool,
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
