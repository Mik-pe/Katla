use super::CORE_LOADER;

use erupt::{
    cstr,
    extensions::{ext_debug_utils::*, khr_surface::*, khr_swapchain::*},
    utils::{
        allocator::{Allocator, AllocatorCreateInfo},
        surface,
    },
    vk1_0::*,
    DeviceLoader, InstanceLoader,
};
use std::{
    ffi::{c_void, CStr, CString},
    os::raw::c_char,
};
use winit::window::Window;

const LAYER_KHRONOS_VALIDATION: *const c_char = cstr!("VK_LAYER_KHRONOS_validation");

pub struct VulkanCtx {
    instance: InstanceLoader,
    pub device: DeviceLoader,
    pub physical_device: PhysicalDevice,
    pub allocator: Allocator,
    pub surface: SurfaceKHR,
    pub surface_caps: SurfaceCapabilitiesKHR,
    pub surface_format: SurfaceFormatKHR,
    pub swapchain_image_views: Vec<ImageView>,
    pub swapchain: SwapchainKHR,
    pub swapchain_images: Vec<Image>,
    pub command_pool: CommandPool,
    pub command_buffers: Vec<CommandBuffer>,
    pub queue: Queue,
    messenger: DebugUtilsMessengerEXT,
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

impl VulkanCtx {
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
                .queue_submit(self.queue, &vec![submit_info], Fence::null())
                .unwrap();
            self.device.queue_wait_idle(self.queue).unwrap();
            self.device
                .free_command_buffers(self.command_pool, &command_buffers);
        }
    }

    pub fn find_depth_format(&self, candidates: Vec<Format>) -> Format {
        let mut depth_format = None;
        for candidate in candidates {
            unsafe {
                let format_props = self.instance.get_physical_device_format_properties(
                    self.physical_device,
                    candidate,
                    None,
                );
                if (format_props.optimal_tiling_features
                    & FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
                    == FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT
                {
                    depth_format = Some(candidate);
                    break;
                }
            }
        }
        dbg!(depth_format);
        depth_format.expect("No acceptable depth formats found!")
    }

    pub fn init(
        window: &Window,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let mut instance =
            Self::create_instance(with_validation_layers, &app_name, &engine_name, window);

        let messenger = if with_validation_layers {
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

            unsafe { instance.create_debug_utils_messenger_ext(&create_info, None, None) }.unwrap()
        } else {
            Default::default()
        };

        let surface = unsafe { surface::create_surface(&mut instance, window, None) }.unwrap();

        let (mut device, queue_family, format, present_mode, surface_caps, physical_device) = {
            let device_extensions = vec![KHR_SWAPCHAIN_EXTENSION_NAME];
            let mut device_layers = vec![];
            if with_validation_layers {
                device_layers.push(LAYER_KHRONOS_VALIDATION);
            }
            // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Physical_devices_and_queue_families
            let (physical_device, queue_family, format, present_mode, properties) =
                unsafe { instance.enumerate_physical_devices(None) }
                    .unwrap()
                    .into_iter()
                    .filter_map(|physical_device| unsafe {
                        let queue_family = match instance
                            .get_physical_device_queue_family_properties(physical_device, None)
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
                            Some(queue_family) => queue_family as u32,
                            None => return None,
                        };

                        let formats = instance
                            .get_physical_device_surface_formats_khr(physical_device, surface, None)
                            .unwrap();
                        let format = match formats
                            .iter()
                            .find(|surface_format| {
                                surface_format.format == Format::B8G8R8A8_SRGB
                                    && surface_format.color_space
                                        == ColorSpaceKHR::SRGB_NONLINEAR_KHR
                            })
                            .and_then(|_| formats.get(0))
                        {
                            Some(surface_format) => surface_format.clone(),
                            None => return None,
                        };

                        let present_mode = instance
                            .get_physical_device_surface_present_modes_khr(
                                physical_device,
                                surface,
                                None,
                            )
                            .unwrap()
                            .into_iter()
                            .find(|present_mode| present_mode == &PresentModeKHR::MAILBOX_KHR)
                            .unwrap_or(PresentModeKHR::FIFO_KHR);

                        let supported_extensions = instance
                            .enumerate_device_extension_properties(physical_device, None, None)
                            .unwrap();
                        if !device_extensions.iter().all(|device_extension| {
                            let device_extension = CStr::from_ptr(*device_extension);

                            supported_extensions.iter().any(|properties| {
                                CStr::from_ptr(properties.extension_name.as_ptr())
                                    == device_extension
                            })
                        }) {
                            return None;
                        }

                        let properties =
                            instance.get_physical_device_properties(physical_device, None);
                        Some((
                            physical_device,
                            queue_family,
                            format,
                            present_mode,
                            properties,
                        ))
                    })
                    .max_by_key(|(_, _, _, _, properties)| match properties.device_type {
                        PhysicalDeviceType::DISCRETE_GPU => 2,
                        PhysicalDeviceType::INTEGRATED_GPU => 1,
                        _ => 0,
                    })
                    .expect("No suitable physical device found");

            println!("Using physical device: {:?}", unsafe {
                CStr::from_ptr(properties.device_name.as_ptr())
            });
            // https://vulkan-tutorial.com/Drawing_a_triangle/Setup/Logical_device_and_queues
            let queue_create_info = vec![DeviceQueueCreateInfoBuilder::new()
                .queue_family_index(queue_family)
                .queue_priorities(&[1.0])];
            let features = PhysicalDeviceFeaturesBuilder::new();

            let create_info = DeviceCreateInfoBuilder::new()
                .queue_create_infos(&queue_create_info)
                .enabled_features(&features)
                .enabled_extension_names(&device_extensions)
                .enabled_layer_names(&device_layers);
            let device = DeviceLoader::new(
                &instance,
                unsafe { instance.create_device(physical_device, &create_info, None, None) }
                    .unwrap(),
            )
            .unwrap();
            let surface_caps = unsafe {
                instance.get_physical_device_surface_capabilities_khr(
                    physical_device,
                    surface,
                    None,
                )
            }
            .unwrap();
            (
                device,
                queue_family,
                format,
                present_mode,
                surface_caps,
                physical_device,
            )
        };
        device.load_vk1_0().unwrap();
        device.load_vk1_1().unwrap();
        device
            .load_khr_swapchain()
            .expect("Couldn't load swapchain!");

        let queue = unsafe { device.get_device_queue(queue_family, 0, None) };
        let allocator =
            Allocator::new(&instance, physical_device, AllocatorCreateInfo::default()).unwrap();

        // https://vulkan-tutorial.com/Drawing_a_triangle/Presentation/Swap_chain
        let swapchain = {
            let mut image_count = surface_caps.min_image_count + 1;
            if surface_caps.max_image_count > 0 && image_count > surface_caps.max_image_count {
                image_count = surface_caps.max_image_count;
            }

            let create_info = SwapchainCreateInfoKHRBuilder::new()
                .surface(surface)
                .min_image_count(image_count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(surface_caps.current_extent)
                .image_array_layers(1)
                .image_usage(ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(SharingMode::EXCLUSIVE)
                .pre_transform(surface_caps.current_transform)
                .composite_alpha(CompositeAlphaFlagBitsKHR::OPAQUE_KHR)
                .present_mode(present_mode)
                .clipped(true)
                .old_swapchain(SwapchainKHR::null());
            let swapchain =
                unsafe { device.create_swapchain_khr(&create_info, None, None) }.unwrap();
            swapchain
        };
        let swapchain_images = unsafe { device.get_swapchain_images_khr(swapchain, None) }.unwrap();

        // https://vulkan-tutorial.com/Drawing_a_triangle/Presentation/Image_views
        let swapchain_image_views: Vec<_> = swapchain_images
            .iter()
            .map(|swapchain_image| {
                let create_info = ImageViewCreateInfoBuilder::new()
                    .image(*swapchain_image)
                    .view_type(ImageViewType::_2D)
                    .format(format.format)
                    .components(ComponentMapping {
                        r: ComponentSwizzle::IDENTITY,
                        g: ComponentSwizzle::IDENTITY,
                        b: ComponentSwizzle::IDENTITY,
                        a: ComponentSwizzle::IDENTITY,
                    })
                    .subresource_range(unsafe {
                        ImageSubresourceRangeBuilder::new()
                            .aspect_mask(ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1)
                            .discard()
                    });
                unsafe { device.create_image_view(&create_info, None, None) }.unwrap()
            })
            .collect();
        // https://vulkan-tutorial.com/Drawing_a_triangle/Drawing/Framebuffers

        // https://vulkan-tutorial.com/Drawing_a_triangle/Drawing/Command_buffers
        let create_info = CommandPoolCreateInfoBuilder::new()
            .queue_family_index(queue_family)
            .flags(CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&create_info, None, None) }.unwrap();

        let command_buffers = {
            let allocate_info = CommandBufferAllocateInfoBuilder::new()
                .command_pool(command_pool)
                .level(CommandBufferLevel::PRIMARY)
                .command_buffer_count(swapchain_image_views.len() as _);
            unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap()
        };

        let ctx = Self {
            instance,
            allocator,
            device,
            physical_device,
            surface,
            surface_caps,
            surface_format: format,
            swapchain,
            swapchain_image_views,
            swapchain_images,
            command_pool,
            command_buffers,
            queue,
            messenger,
        };
        ctx
    }

    pub fn pre_destroy(&self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
        }
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            for &image_view in &self.swapchain_image_views {
                self.device.destroy_image_view(image_view, None);
            }
            self.device.destroy_swapchain_khr(self.swapchain, None);

            self.device.destroy_device(None);
            self.instance.destroy_surface_khr(self.surface, None);

            if !self.messenger.is_null() {
                self.instance
                    .destroy_debug_utils_messenger_ext(self.messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}
