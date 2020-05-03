use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuBufferPool};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::device::{Device, DeviceExtensions, Features, Queue};
use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{AttachmentImage, SwapchainImage};
use vulkano::instance::{Instance, InstanceExtensions, PhysicalDevice};
use vulkano::pipeline::viewport::Viewport;
use vulkano::swapchain;
use vulkano::swapchain::{
    AcquireError, ColorSpace, FullscreenExclusive, PresentMode, Surface, SurfaceTransform,
    Swapchain, SwapchainCreationError,
};
use vulkano::sync;
use vulkano::sync::{FlushError, GpuFuture};
use vulkano_win::VkSurfaceBuild;

use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

use crate::rendering::pipeline as my_pipeline;
use crate::rendering::vertextypes;
use crate::rendering::MeshData;

pub struct VulkanoCtx {
    instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub surface: Arc<Surface<Window>>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    renderpipeline: my_pipeline::RenderPipeline,
    swapchain: Arc<Swapchain<Window>>,
    command_queue: Arc<Queue>,
    internal_state: InternalState,
}

struct InternalState {
    pub recreate_swapchain: bool,
    pub previous_frame_end: Option<Box<dyn GpuFuture>>,
    pub framebuffers: Vec<Arc<dyn FramebufferAbstract + Send + Sync>>,
    pub uniform_buffer: CpuBufferPool<my_pipeline::vs::ty::Data>,
    pub angle: f32,
    dynamic_state: DynamicState,
}

impl VulkanoCtx {
    pub fn init(event_loop: &EventLoop<()>) -> Self {
        let instance = {
            let extensions = vulkano_win::required_extensions();
            Instance::new(None, &extensions, None).expect("failed to create Vulkan instance")
        };
        let physical = PhysicalDevice::enumerate(&instance)
            .next()
            .expect("no device available");
        println!(
            "Using device: {} (type: {:?})",
            physical.name(),
            physical.ty()
        );
        for family in physical.queue_families() {
            println!(
                "Found a queue family with {:?} queue(s)",
                family.queues_count()
            );
        }

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
        let command_queue = queues.next().unwrap();

        let surface = WindowBuilder::new()
            .build_vk_surface(&event_loop, instance.clone())
            .unwrap();

        let (swapchain, images) = {
            let caps = surface.capabilities(physical).unwrap();

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
                &command_queue,
                // Vulkan uses upper-left as 0,0 of the framebuffer, keep this in mind!
                SurfaceTransform::Identity,
                alpha,
                PresentMode::Fifo,
                FullscreenExclusive::Default,
                true,
                ColorSpace::SrgbNonLinear,
            )
            .expect("failed to create swapchain")
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
                    },
                    depth: {
                        load: Clear,
                        store: DontCare,
                        format: Format::D16Unorm,
                        samples: 1,
                    }
                },
                pass: {
                    // We use the attachment named `color` as the one and only color attachment.
                    color: [color],
                    depth_stencil: {depth}
                }
            )
            .unwrap(),
        );

        let uniform_buffer =
            CpuBufferPool::<my_pipeline::vs::ty::Data>::new(device.clone(), BufferUsage::all());

        let renderpipeline =
            my_pipeline::RenderPipeline::new_with_shaders::<vertextypes::VertexNormal>(
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
        let internal_state = InternalState {
            recreate_swapchain: false,
            previous_frame_end: Some(Box::new(sync::now(device.clone())) as Box<dyn GpuFuture>),
            framebuffers: window_size_dependent_setup(
                &images,
                device.clone(),
                render_pass.clone(),
                &mut dynamic_state,
            ),
            uniform_buffer,
            angle: 0.0,
            dynamic_state,
        };

        Self {
            instance,
            device,
            surface,
            render_pass,
            renderpipeline,
            swapchain,
            command_queue,
            internal_state,
        }
    }

    // TODO: Make designated functions for drawing, updating stuff, etc.
    // rather than sending the winit event here
    pub fn handle_event(
        &mut self,
        event: &Event<()>,
        delta_time: f32,
        projection: &mikpe_math::Mat4,
        view: &mikpe_math::Mat4,
        meshbuffers: &Vec<Box<dyn MeshData>>,
    ) -> () {
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
                        self.renderpipeline.pipeline.clone(),
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
    }
}

fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    device: Arc<Device>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    dynamic_state: &mut DynamicState,
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
    let dimensions = images[0].dimensions();
    let depth_buffer =
        AttachmentImage::transient(device.clone(), dimensions, Format::D16Unorm).unwrap();

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
                    .add(depth_buffer.clone())
                    .unwrap()
                    .build()
                    .unwrap(),
            ) as Arc<dyn FramebufferAbstract + Send + Sync>
        })
        .collect::<Vec<_>>()
}
