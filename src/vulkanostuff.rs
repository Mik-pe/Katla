use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, DeviceExtensions, Features};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::SwapchainImage;
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice};
use vulkano::pipeline::viewport::Viewport;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::swapchain;
use vulkano::swapchain::{
    AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};
use vulkano::sync;
use vulkano::sync::{FlushError, GpuFuture};
use vulkano_win::VkSurfaceBuild;

use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

use crate::rendering::pipeline as my_pipeline;

#[derive(Default, Debug, Clone)]
struct Vertex {
    position: [f32; 2],
}
vulkano::impl_vertex!(Vertex, position);
pub struct VulkanoCtx {
    id: i32,
    instance: Arc<Instance>,
}

impl VulkanoCtx {
    pub fn init() -> Self {
        // We now create a buffer that will store the shape of our triangle.

        let instance = {
            let extensions = vulkano_win::required_extensions();
            Instance::new(None, &extensions, None).expect("failed to create Vulkan instance")
        };
        let physical = PhysicalDevice::enumerate(&instance)
            .next()
            .expect("no device available");
        for family in physical.queue_families() {
            println!(
                "Found a queue family with {:?} queue(s)",
                family.queues_count()
            );
        }

        println!("Physical device: {:?}", physical.name());
        let queue_family = physical
            .queue_families()
            .find(|&q| q.supports_graphics())
            .expect("couldn't find a graphical queue family");

        let (device, mut queues) = {
            let device_ext = vulkano::device::DeviceExtensions {
                khr_swapchain: true,
                ..vulkano::device::DeviceExtensions::none()
            };

            Device::new(
                physical,
                physical.supported_features(),
                &device_ext,
                [(queue_family, 0.5)].iter().cloned(),
            )
            .expect("failed to create device")
        };
        let queue = queues.next().unwrap();

        let events_loop = EventLoop::new();
        let surface = WindowBuilder::new()
            .build_vk_surface(&events_loop, instance.clone())
            .unwrap();

        let (mut swapchain, images) = {
            let caps = surface.capabilities(physical).unwrap();
            let usage = caps.supported_usage_flags;

            let alpha = caps.supported_composite_alpha.iter().next().unwrap();
            let format = caps.supported_formats[0].0;
            let dimensions: [u32; 2] = surface.window().inner_size().into();

            Swapchain::new(
                device.clone(),
                surface.clone(),
                caps.min_image_count,
                format,
                dimensions,
                1,
                caps.supported_usage_flags,
                &queue,
                SurfaceTransform::Identity,
                alpha,
                PresentMode::Fifo,
                FullscreenExclusive::Default,
                true,
                ColorSpace::SrgbNonLinear,
            )
            .expect("failed to create swapchain")
        };

        let vertex_buffer = {
            CpuAccessibleBuffer::from_iter(
                device.clone(),
                BufferUsage::all(),
                false,
                [
                    Vertex {
                        position: [-0.5, -0.25],
                    },
                    Vertex {
                        position: [0.0, 0.5],
                    },
                    Vertex {
                        position: [0.25, -0.1],
                    },
                ]
                .iter()
                .cloned(),
            )
            .unwrap()
        };

        let render_pass = Arc::new(
            vulkano::single_pass_renderpass!(
                device.clone(),
                attachments: {
                    // `color` is a custom name we give to the first and only attachment.
                    color: {
                        // `load: Clear` means that we ask the GPU to clear the content of this
                        // attachment at the start of the drawing.
                        load: Clear,
                        // `store: Store` means that we ask the GPU to store the output of the draw
                        // in the actual image. We could also ask it to discard the result.
                        store: Store,
                        // `format: <ty>` indicates the type of the format of the image. This has to
                        // be one of the types of the `vulkano::format` module (or alternatively one
                        // of your structs that implements the `FormatDesc` trait). Here we use the
                        // same format as the swapchain.
                        format: swapchain.format(),
                        // TODO:
                        samples: 1,
                    }
                },
                pass: {
                    // We use the attachment named `color` as the one and only color attachment.
                    color: [color],
                    // No depth-stencil attachment is indicated with empty brackets.
                    depth_stencil: {}
                }
            )
            .unwrap(),
        );

        let pipeline = my_pipeline::create_pipeline_with_shaders::<Vertex>(
            std::path::PathBuf::new(),
            device.clone(),
            render_pass.clone(),
        );

        let mut dynamic_state = DynamicState {
            line_width: None,
            viewports: None,
            scissors: None,
            compare_mask: None,
            write_mask: None,
            reference: None,
        };

        let mut framebuffers =
            window_size_dependent_setup(&images, render_pass.clone(), &mut dynamic_state);

        let mut recreate_swapchain = false;
        let mut previous_frame_end =
            Some(Box::new(sync::now(device.clone())) as Box<dyn GpuFuture>);

        events_loop.run(move |event, _, control_flow| {
            use winit::event::{Event, VirtualKeyCode, WindowEvent};
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = winit::event_loop::ControlFlow::Exit;
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let Some(keycode) = input.virtual_keycode {
                            match keycode {
                                VirtualKeyCode::Escape => {
                                    *control_flow = winit::event_loop::ControlFlow::Exit;
                                }
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::Resized(_) => {
                        recreate_swapchain = true;
                    }
                    _ => *control_flow = winit::event_loop::ControlFlow::Poll,
                },
                Event::RedrawEventsCleared => {
                    // It is important to call this function from time to time, otherwise resources will keep
                    // accumulating and you will eventually reach an out of memory error.
                    // Calling this function polls various fences in order to determine what the GPU has
                    // already processed, and frees the resources that are no longer needed.
                    previous_frame_end.as_mut().unwrap().cleanup_finished();

                    // Whenever the window resizes we need to recreate everything dependent on the window size.
                    // In this example that includes the swapchain, the framebuffers and the dynamic state viewport.
                    if recreate_swapchain {
                        // Get the new dimensions of the window.
                        let dimensions: [u32; 2] = surface.window().inner_size().into();
                        let (new_swapchain, new_images) =
                            match swapchain.recreate_with_dimensions(dimensions) {
                                Ok(r) => r,
                                // This error tends to happen when the user is manually resizing the window.
                                // Simply restarting the loop is the easiest way to fix this issue.
                                Err(SwapchainCreationError::UnsupportedDimensions) => return,
                                Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                            };

                        swapchain = new_swapchain;
                        // Because framebuffers contains an Arc on the old swapchain, we need to
                        // recreate framebuffers as well.
                        framebuffers = window_size_dependent_setup(
                            &new_images,
                            render_pass.clone(),
                            &mut dynamic_state,
                        );
                        recreate_swapchain = false;
                    }

                    // Before we can draw on the output, we have to *acquire* an image from the swapchain. If
                    // no image is available (which happens if you submit draw commands too quickly), then the
                    // function will block.
                    // This operation returns the index of the image that we are allowed to draw upon.
                    //
                    // This function can block if no image is available. The parameter is an optional timeout
                    // after which the function call will return an error.
                    let (image_num, suboptimal, acquire_future) =
                        match swapchain::acquire_next_image(swapchain.clone(), None) {
                            Ok(r) => r,
                            Err(AcquireError::OutOfDate) => {
                                recreate_swapchain = true;
                                return;
                            }
                            Err(e) => panic!("Failed to acquire next image: {:?}", e),
                        };

                    // acquire_next_image can be successful, but suboptimal. This means that the swapchain image
                    // will still work, but it may not display correctly. With some drivers this can be when
                    // the window resizes, but it may not cause the swapchain to become out of date.
                    if suboptimal {
                        recreate_swapchain = true;
                    }

                    // Specify the color to clear the framebuffer with i.e. blue
                    let clear_values = vec![[0.3, 0.5, 0.3, 1.0].into()];

                    // In order to draw, we have to build a *command buffer*. The command buffer object holds
                    // the list of commands that are going to be executed.
                    //
                    // Building a command buffer is an expensive operation (usually a few hundred
                    // microseconds), but it is known to be a hot path in the driver and is expected to be
                    // optimized.
                    //
                    // Note that we have to pass a queue family when we create the command buffer. The command
                    // buffer will only be executable on that given queue family.
                    let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(
                        device.clone(),
                        queue.family(),
                    )
                    .unwrap()
                    // Before we can draw, we have to *enter a render pass*. There are two methods to do
                    // this: `draw_inline` and `draw_secondary`. The latter is a bit more advanced and is
                    // not covered here.
                    //
                    // The third parameter builds the list of values to clear the attachments with. The API
                    // is similar to the list of attachments when building the framebuffers, except that
                    // only the attachments that use `load: Clear` appear in the list.
                    .begin_render_pass(framebuffers[image_num].clone(), false, clear_values)
                    .unwrap()
                    // We are now inside the first subpass of the render pass. We add a draw command.
                    //
                    // The last two parameters contain the list of resources to pass to the shaders.
                    // Since we used an `EmptyPipeline` object, the objects have to be `()`.
                    .draw(
                        pipeline.clone(),
                        &dynamic_state,
                        vertex_buffer.clone(),
                        (),
                        (),
                    )
                    .unwrap()
                    // We leave the render pass by calling `draw_end`. Note that if we had multiple
                    // subpasses we could have called `next_inline` (or `next_secondary`) to jump to the
                    // next subpass.
                    .end_render_pass()
                    .unwrap()
                    // Finish building the command buffer by calling `build`.
                    .build()
                    .unwrap();

                    let future = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .then_execute(queue.clone(), command_buffer)
                        .unwrap()
                        // The color output is now expected to contain our triangle. But in order to show it on
                        // the screen, we have to *present* the image by calling `present`.
                        //
                        // This function does not actually present the image immediately. Instead it submits a
                        // present command at the end of the queue. This means that it will only be presented once
                        // the GPU has finished executing the command buffer that draws the triangle.
                        .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                        .then_signal_fence_and_flush();

                    match future {
                        Ok(future) => {
                            previous_frame_end = Some(Box::new(future) as Box<_>);
                        }
                        Err(FlushError::OutOfDate) => {
                            recreate_swapchain = true;
                            previous_frame_end =
                                Some(Box::new(sync::now(device.clone())) as Box<_>);
                        }
                        Err(e) => {
                            println!("Failed to flush future: {:?}", e);
                            previous_frame_end =
                                Some(Box::new(sync::now(device.clone())) as Box<_>);
                        }
                    }
                }
                _ => (),
            }
        });
        Self { id: 0, instance }
    }
}

fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    dynamic_state: &mut DynamicState,
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
    let dimensions = images[0].dimensions();

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
        depth_range: 0.0..1.0,
    };
    dynamic_state.viewports = Some(vec![viewport]);

    images
        .iter()
        .map(|image| {
            Arc::new(
                Framebuffer::start(render_pass.clone())
                    .add(image.clone())
                    .unwrap()
                    .build()
                    .unwrap(),
            ) as Arc<dyn FramebufferAbstract + Send + Sync>
        })
        .collect::<Vec<_>>()
}