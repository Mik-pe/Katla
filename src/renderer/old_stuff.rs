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
            use katla_math::Mat4;

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
