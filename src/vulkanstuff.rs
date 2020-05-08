use pipeline::*;
use swapdata::*;
mod pipeline;
mod swapdata;

use std::{
    ffi::{c_void, CStr, CString},
    os::raw::c_char,
    sync::Mutex,
};

use erupt::{
    cstr,
    extensions::{ext_debug_utils::*, khr_surface::*, khr_swapchain::*},
    utils::surface,
    vk1_0::*,
    CoreLoader, DeviceLoader, InstanceLoader,
};
use lazy_static::lazy_static;

use winit::event::Event;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

lazy_static! {
    static ref CORE_LOADER: Mutex<CoreLoader<libloading::Library>> = {
        let core = Mutex::new(CoreLoader::new().unwrap());
        core.lock().unwrap().load_vk1_0().unwrap();
        core
    };
}
pub struct VulkanCtx {
    instance: InstanceLoader,
    pub device: DeviceLoader,
    pub surface: SurfaceKHR,
    pub window: Window,
    messenger: DebugUtilsMessengerEXT,
    swapchain: SwapchainKHR,
    swapchain_framebuffers: Vec<Framebuffer>,
    swapchain_image_views: Vec<ImageView>,
    command_pool: CommandPool,
    command_buffers: Vec<CommandBuffer>,
    render_pass: RenderPass,
    queue: Queue,
    pipeline: RenderPipeline,
    swap_data: SwapData,
}

const LAYER_KHRONOS_VALIDATION: *const c_char = cstr!("VK_LAYER_KHRONOS_validation");
const FRAMES_IN_FLIGHT: usize = 2;

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
        (commands.enumerate_instance_layer_properties)(&mut layer_count, 0 as _);
        let mut available_layers: Vec<LayerProperties> = Vec::new();
        available_layers.resize(layer_count as usize, LayerProperties::default());
        (commands.enumerate_instance_layer_properties)(
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
            .api_version(erupt::make_version(1, 0, 0));

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
        instance
    }

    pub fn init(
        event_loop: &EventLoop<()>,
        with_validation_layers: bool,
        app_name: CString,
        engine_name: CString,
    ) -> Self {
        let window = WindowBuilder::new()
            .with_title("Erupt_Test_Mikpe")
            .with_resizable(true)
            .build(event_loop)
            .unwrap();

        let mut instance =
            Self::create_instance(with_validation_layers, &app_name, &engine_name, &window);

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

        let surface = unsafe { surface::create_surface(&mut instance, &window, None) }.unwrap();

        let (mut device, queue_family, format, present_mode, surface_caps) = {
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
            (device, queue_family, format, present_mode, surface_caps)
        };
        device.load_vk1_0().unwrap();
        device.load_khr_swapchain().unwrap();

        let queue = unsafe { device.get_device_queue(queue_family, 0, None) };

        // https://vulkan-tutorial.com/Drawing_a_triangle/Graphics_pipeline_basics/Render_passes
        let render_pass = {
            let attachments = vec![AttachmentDescriptionBuilder::new()
                .format(format.format)
                .samples(SampleCountFlagBits::_1)
                .load_op(AttachmentLoadOp::CLEAR)
                .store_op(AttachmentStoreOp::STORE)
                .stencil_load_op(AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(AttachmentStoreOp::DONT_CARE)
                .initial_layout(ImageLayout::UNDEFINED)
                .final_layout(ImageLayout::PRESENT_SRC_KHR)];

            let color_attachment_refs = vec![AttachmentReferenceBuilder::new()
                .attachment(0)
                .layout(ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
            let subpasses = vec![SubpassDescriptionBuilder::new()
                .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_attachment_refs)];
            let dependencies = vec![SubpassDependencyBuilder::new()
                .src_subpass(SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(AccessFlags::empty())
                .dst_stage_mask(PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(AccessFlags::COLOR_ATTACHMENT_WRITE)];

            let create_info = RenderPassCreateInfoBuilder::new()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies);

            unsafe { device.create_render_pass(&create_info, None, None) }.unwrap()
        };
        let pipeline = RenderPipeline::new(&device, render_pass, surface_caps);

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
        let swapchain_framebuffers: Vec<_> = swapchain_image_views
            .iter()
            .map(|image_view| {
                let attachments = vec![*image_view];
                let create_info = FramebufferCreateInfoBuilder::new()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(surface_caps.current_extent.width)
                    .height(surface_caps.current_extent.height)
                    .layers(1);

                unsafe { device.create_framebuffer(&create_info, None, None) }.unwrap()
            })
            .collect();

        // https://vulkan-tutorial.com/Drawing_a_triangle/Drawing/Command_buffers
        let create_info = CommandPoolCreateInfoBuilder::new().queue_family_index(queue_family);
        let command_pool = unsafe { device.create_command_pool(&create_info, None, None) }.unwrap();

        let command_buffers = {
            let allocate_info = CommandBufferAllocateInfoBuilder::new()
                .command_pool(command_pool)
                .level(CommandBufferLevel::PRIMARY)
                .command_buffer_count(swapchain_framebuffers.len() as _);
            unsafe { device.allocate_command_buffers(&allocate_info) }.unwrap()
        };

        for (&command_buffer, &framebuffer) in
            command_buffers.iter().zip(swapchain_framebuffers.iter())
        {
            let begin_info = CommandBufferBeginInfoBuilder::new();
            unsafe { device.begin_command_buffer(command_buffer, &begin_info) }.unwrap();

            let clear_values = vec![ClearValue {
                color: ClearColorValue {
                    float32: [0.3, 0.5, 0.3, 1.0],
                },
            }];
            let begin_info = RenderPassBeginInfoBuilder::new()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(Rect2D {
                    offset: Offset2D { x: 0, y: 0 },
                    extent: surface_caps.current_extent,
                })
                .clear_values(&clear_values);

            unsafe {
                device.cmd_begin_render_pass(command_buffer, &begin_info, SubpassContents::INLINE);
                device.cmd_bind_pipeline(
                    command_buffer,
                    PipelineBindPoint::GRAPHICS,
                    pipeline.pipeline,
                );
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    PipelineBindPoint::GRAPHICS,
                    pipeline.pipeline_layout,
                    0,
                    &[pipeline.desc_set],
                    &[],
                );

                //TODO: Add buffer data to draw!
                device.cmd_draw(command_buffer, 3, 1, 0, 0);
                device.cmd_end_render_pass(command_buffer);

                device.end_command_buffer(command_buffer).unwrap();
            }
        }
        let swap_data = SwapData::new(&device, &swapchain_images, FRAMES_IN_FLIGHT);

        let ctx = Self {
            instance,
            device,
            surface,
            window,
            messenger,
            swapchain,
            swapchain_framebuffers,
            swapchain_image_views,
            command_pool,
            command_buffers,
            render_pass,
            queue,
            pipeline,
            swap_data,
        };
        ctx
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.swap_data.destroy(&self.device);

            self.device.destroy_command_pool(self.command_pool, None);

            for &framebuffer in &self.swapchain_framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }

            self.pipeline.destroy(&self.device);
            self.device.destroy_render_pass(self.render_pass, None);

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
        println!("Clean shutdown!");
    }

    // TODO: Make designated functions for drawing, updating stuff, etc.
    // rather than sending the winit event here
    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        _delta_time: f32,
        _projection: &mikpe_math::Mat4,
        _view: &mikpe_math::Mat4,
    ) {
        match event {
            Event::MainEventsCleared => {
                self.swap_data.wait_for_fence(&self.device);

                let (available_sem, finished_sem, in_flight_fence, image_index) =
                    self.swap_data.swap_images(&self.device, self.swapchain);

                let wait_semaphores = vec![available_sem];
                let command_buffers = vec![self.command_buffers[image_index as usize]];
                let signal_semaphores = vec![finished_sem];
                let submit_info = SubmitInfoBuilder::new()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&[PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores);
                unsafe {
                    let in_flight_fence = in_flight_fence;
                    self.device.reset_fences(&[in_flight_fence]).unwrap();
                    self.device
                        .queue_submit(self.queue, &[submit_info], in_flight_fence)
                        .unwrap()
                }

                let swapchains = vec![self.swapchain];
                let image_indices = vec![image_index];
                let present_info = PresentInfoKHRBuilder::new()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&image_indices);

                unsafe { self.device.queue_present_khr(self.queue, &present_info) }.unwrap();

                self.swap_data.step_frame();
            }
            _ => {}
        }
        /*
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Resized(_) => {
                    self.internal_state.recreate_swapchain = true;
                }
                _ => {}
            },
            Event::RedrawEventsCleared => {
                // It is important to call this function from time to time, otherwise resources will keep
                // accumulating and you will eventually reach an out of memory error.
                // Calling this function polls various fences in order to determine what the GPU has
                // already processed, and frees the resources that are no longer needed.
                self.internal_state
                    .previous_frame_end
                    .as_mut()
                    .unwrap()
                    .cleanup_finished();
                self.internal_state.angle += delta_time;

                // Whenever the window resizes we need to recreate everything dependent on the window size.
                // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
                if self.internal_state.recreate_swapchain {
                    // Get the new dimensions of the window.
                    let dimensions: [u32; 2] = self.surface.window().inner_size().into();
                    let (new_swapchain, new_images) =
                        match self.swapchain.recreate_with_dimensions(dimensions) {
                            Ok(r) => r,
                            // This error tends to happen when the user is manually resizing the window.
                            // Simply restarting the loop is the easiest way to fix this issue.
                            Err(SwapchainCreationError::UnsupportedDimensions) => return,
                            Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                        };

                    self.swapchain = new_swapchain;
                    self.internal_state.framebuffers = window_size_dependent_setup(
                        &new_images,
                        self.device.clone(),
                        self.render_pass.clone(),
                        &mut self.internal_state.dynamic_state,
                    );
                    self.internal_state.recreate_swapchain = false;
                }

                let uniform_buffer_subbuffer = {
                    use mikpe_math::Mat4;

                    let world = Mat4::from_rotaxis(&self.internal_state.angle, [0.0, 1.0, 0.0]);

                    let uniform_data = my_pipeline::vs::ty::Data {
                        world: world.into(),
                        view: view.clone().into(),
                        proj: projection.clone().into(),
                    };

                    self.internal_state
                        .uniform_buffer
                        .next(uniform_data)
                        .unwrap()
                };

                let layout = self
                    .renderpipeline
                    .pipeline
                    .descriptor_set_layout(0)
                    .unwrap();

                let set = Arc::new(
                    PersistentDescriptorSet::start(layout.clone())
                        .add_buffer(uniform_buffer_subbuffer)
                        .unwrap()
                        .build()
                        .unwrap(),
                );

                // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
                // no image is available (which happens if you submit draw commands too quickly), then the
                // function will block.
                // This operation returns the index of the image that we are allowed to draw upon.
                //
                // This function can block if no image is available. The parameter is an optional timeout
                // after which the function call will return an error.
                let (image_num, suboptimal, acquire_future) =
                    match swapchain::acquire_next_image(self.swapchain.clone(), None) {
                        Ok(r) => r,
                        Err(AcquireError::OutOfDate) => {
                            self.internal_state.recreate_swapchain = true;
                            return;
                        }
                        Err(e) => panic!("Failed to acquire next image: {:?}", e),
                    };

                if suboptimal {
                    self.internal_state.recreate_swapchain = true;
                }

                let clear_values = vec![[0.3, 0.5, 0.3, 1.0].into(), 1f32.into()];

                // In order to draw, we have to build a *command buffer*. The command buffer object holds
                // the list of commands that are going to be executed.
                //
                // Building a command buffer is an expensive operation (usually a few hundred
                // microseconds), but it is known to be a hot path in the driver and is expected to be
                // optimized.
                //
                // Note that we have to pass a queue family when we create the command buffer. The command
                // buffer will only be executable on that given queue family.
                let mut cmd_buffer_builder = AutoCommandBufferBuilder::primary_one_time_submit(
                    self.device.clone(),
                    self.command_queue.family(),
                )
                .unwrap()
                // Before we can draw, we have to *enter a render pass*. There are two methods to do
                // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
                // not covered here.
                //
                // The third parameter builds the list of values to clear the attachments with. The API
                // is similar to the list of attachments when building the framebuffers, except that
                // only the attachments that use `load: Clear` appear in the list.
                .begin_render_pass(
                    self.internal_state.framebuffers[image_num].clone(),
                    false,
                    clear_values,
                )
                .unwrap();
                for meshbuffer in meshbuffers {
                    cmd_buffer_builder = meshbuffer.draw_data(
                        cmd_buffer_builder,
                        set.clone(),
                        &self.internal_state.dynamic_state,
                    );
                }
                let command_buffer = cmd_buffer_builder
                    .end_render_pass()
                    .unwrap()
                    .build() // Finish building the command buffer by calling `build`.
                    .unwrap();

                let future = self
                    .internal_state
                    .previous_frame_end
                    .take()
                    .unwrap()
                    .join(acquire_future)
                    .then_execute(self.command_queue.clone(), command_buffer)
                    .unwrap()
                    .then_swapchain_present(
                        self.command_queue.clone(),
                        self.swapchain.clone(),
                        image_num,
                    )
                    .then_signal_fence_and_flush();

                match future {
                    Ok(future) => {
                        self.internal_state.previous_frame_end = Some(Box::new(future) as Box<_>);
                    }
                    Err(FlushError::OutOfDate) => {
                        self.internal_state.recreate_swapchain = true;
                        self.internal_state.previous_frame_end =
                            Some(Box::new(sync::now(self.device.clone())) as Box<_>);
                    }
                    Err(e) => {
                        println!("Failed to flush future: {:?}", e);
                        self.internal_state.previous_frame_end =
                            Some(Box::new(sync::now(self.device.clone())) as Box<_>);
                    }
                }
            }
            _ => (),
        }
        */
    }
}
