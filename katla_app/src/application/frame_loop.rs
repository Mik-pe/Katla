use log::{debug, info, warn};

use katla_gfx::GpuRenderer;

use crate::application::Application;

impl Application {
    /// Cleanup resources on exit.
    /// Called both from exiting() and directly before event_loop.exit() for max_frames mode.
    pub(crate) fn cleanup_on_exit(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        // Run shutdown hook for game-side cleanup
        if let Some(hook) = self.on_shutdown.take() {
            hook(self);
        }

        // Wait for any pending async readback to complete before destroying resources
        // This must happen BEFORE wait_for_device() to ensure readback finishes
        if let Some(vulkan_renderer) = self.renderer.as_vulkan() {
            match vulkan_renderer.wait_for_pending_readback() {
                Ok(Some((frame, image_data))) => {
                    info!("Saving final frame {} before shutdown", frame);
                    let extent = self.renderer.swapchain_extent();
                    let width = extent.width as usize;
                    let height = extent.height as usize;
                    if let Err(e) = self.save_frame_as_png(frame, &image_data, width, height) {
                        log::error!("Failed to save final frame {}: {}", frame, e);
                    }
                }
                Ok(None) => {
                    log::debug!("No pending readback to complete during shutdown");
                }
                Err(e) => {
                    log::error!("Failed to wait for pending readback during shutdown: {}", e);
                }
            }
        }

        // Save preferences before exit
        if let Err(e) = self.preferences.save() {
            warn!("Failed to save preferences: {}", e);
        } else {
            info!("Saved preferences to disk");
        }

        // Save GUI state before exit
        self.save_editor_state();

        // Wait for device to ensure all GPU operations are complete
        self.renderer.wait_for_device();

        // Cleanup frame graph transient textures BEFORE destroying renderer
        // This ensures proper cleanup order and avoids heap corruption during shutdown
        self.frame_graph.cleanup();

        // Destroy renderer (which owns the particle system)
        self.renderer.destroy();
    }

    /// Handle the RedrawRequested event — the main per-frame orchestration.
    pub(crate) fn handle_redraw_requested(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if self.cleaned_up {
            return;
        }

        if self.minimized {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
            return;
        }

        self.timer.add_timestamp();
        let dt = self.timer.get_delta() as f32;

        // Sync editor camera speed to input state before systems run
        #[cfg(feature = "editor")]
        {
            if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
                input.camera_speed = self.editor.editor_ui.editor_settings().camera_speed;
            }
            if let Some(flag) = self
                .world
                .get_resource_mut::<katla_script::PopulateScriptInspector>()
            {
                flag.0 = true;
            }
        }

        // Update world (runs ECS systems in parallel where possible)
        self.world.update_parallel(dt);

        // Clear per-frame mouse delta after the tick.
        if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
            input.mouse_delta = (0.0, 0.0);
            input.mouse_wheel_delta = 0.0;
        }

        // Forward script audio commands to AudioSystem
        {
            let mut audio_cmds = self
                .world
                .get_resource_mut::<katla_script::PendingAudioCommands>()
                .map(|r| std::mem::take(&mut r.0))
                .unwrap_or_default();
            if !audio_cmds.is_empty()
                && let Some(ref mut audio) = self.audio_system
            {
                audio.process_script_audio_commands(&mut audio_cmds);
            }
        }

        // Process script raycast commands against PhysicsWorld
        {
            let raycast_cmds: Vec<_> = self
                .world
                .get_resource_mut::<katla_script::PendingRaycastCommands>()
                .map(|r| std::mem::take(&mut r.0))
                .unwrap_or_default();
            if !raycast_cmds.is_empty() {
                let mut results = std::collections::HashMap::new();
                if let Some(physics) = self.world.get_resource::<katla_physics::PhysicsWorld>() {
                    for cmd in raycast_cmds {
                        if let katla_script::bindings::world::ScriptCommand::Raycast {
                            origin,
                            direction,
                            max_distance,
                            return_index,
                        } = cmd
                            && let Some(hit) = physics.raycast(origin, direction, max_distance)
                        {
                            results.insert(
                                return_index,
                                katla_script::bindings::script_world::RaycastResult {
                                    entity: hit.entity,
                                    point: hit.point,
                                    normal: hit.normal,
                                    distance: hit.distance,
                                },
                            );
                        }
                    }
                }
                if !results.is_empty()
                    && let Some(pending) = self
                        .world
                        .get_resource_mut::<katla_script::PendingRaycastResults>()
                {
                    pending.0.extend(results);
                }
            }
        }

        // Run per-frame update hook (after ECS systems, before rendering)
        if let Some(ref mut hook) = self.on_update {
            hook(&mut self.world, dt);
        }

        // Process ECS events to clean up GPU resources for destroyed entities
        crate::gpu_cleanup::process_gpu_cleanup_events(
            &self.world,
            &mut self.gpu_resource_tracker,
            &mut self.renderer,
        );

        let uses_katla_scene = self.frame_graph_runtime.uses_katla_scene();

        // Built-in scene subsystems must not run for an application-owned graph.
        // A custom graph may own entirely different compute/animation work.
        if uses_katla_scene
            && let katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) = &mut self.renderer
            && let Some(ref mut ps) = vulkan_renderer.particle_system
        {
            self.particle_system.update(&mut self.world, ps, dt);
        }

        // Update audio system — process AudioEmitter components
        if let Some(ref mut audio) = self.audio_system {
            audio.update(&mut self.world, dt);
        }

        // Update GPU animation: prepare data and upload per-frame params
        if uses_katla_scene
            && let katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) = &mut self.renderer
            && let (Some(gpu_anim), Some(pipeline), Some(buffers)) = (
                &mut self.gpu_animation_system,
                &mut vulkan_renderer.animation_pipeline,
                &mut vulkan_renderer.animation_buffers,
            )
        {
            gpu_anim
                .prepare(&mut self.world, pipeline, buffers)
                .unwrap_or_else(|e| {
                    log::error!("GPU animation prepare failed: {:?}", e);
                });
            gpu_anim.update_params(&mut self.world, buffers);
            self.frame_graph
                .as_vulkan_mut()
                .set_animation_skeleton_count(gpu_anim.skeleton_count() as u32);

            // Build per-entity skeleton copy commands:
            // (skeleton_handle_index, joint_offset, joint_count)
            use crate::components::DrawableComponent;
            let mut copy_cmds = Vec::new();
            for entity in gpu_anim.entities() {
                if let Some(drawable) = self.world.get_component::<DrawableComponent>(entity)
                    && let Some(info) = gpu_anim.entity_info(entity)
                {
                    copy_cmds.push((
                        drawable.skeleton_handle.index(),
                        info.joint_offset,
                        info.joint_count,
                    ));
                }
            }
            self.frame_graph
                .as_vulkan_mut()
                .set_skeleton_copy_commands(copy_cmds);
        }

        // Poll background loader for completed asset loads
        self.poll_background_loader();

        // Poll asset watcher for shader/texture changes
        self.poll_asset_watcher();

        // Note: Transient textures are double-buffered (one per FRAMES_IN_FLIGHT).
        // The viewport bindless index must be updated BEFORE generating the UI
        // draw list so the UI samples from the correct per-frame texture.
        // Doing it after would cause an off-by-one mismatch: the UI would
        // sample from the previous frame's stale texture.
        self.render_editor_frame(dt);

        // Layout dump: if requested, serialize the UI tree and write to stdout/file, then exit.
        self.dump_layout_if_needed();

        // Asynchronous black frame checking:
        // - On frame N: Queue async readback (non-blocking)
        // - On frame N+1: Check if readback from frame N is complete and save to disk
        // This allows us to catch synchronization issues that synchronous readback would mask
        if self.info.check_black_frames && self.frame_count > 0 {
            let extent = self.renderer.swapchain_extent();
            let width = extent.width as usize;
            let height = extent.height as usize;

            // Collect readback result and queue next readback in a single mutable borrow scope
            let readback_result = match &mut self.renderer {
                katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) => {
                    let result = vulkan_renderer.check_pending_readback();
                    if let Err(e) = vulkan_renderer.queue_async_readback(self.frame_count) {
                        log::error!(
                            "Frame {} - Failed to queue async readback: {}",
                            self.frame_count,
                            e
                        );
                    }
                    Some(result)
                }
                #[cfg(target_os = "macos")]
                katla_gfx::AnyRenderer::Metal(_) => None,
            };

            if let Some(Ok(Some((prev_frame, image_data)))) = readback_result {
                // Save frame as PNG for visual inspection
                if let Err(e) = self.save_frame_as_png(prev_frame, &image_data, width, height) {
                    log::error!("Failed to save frame {}: {}", prev_frame, e);
                }

                // Check 9 pixels in a 3x3 grid to detect if ANY pixel has color
                let mut all_pixels_black = true;
                let mut first_non_black_pixel = None;

                // Sample positions: center, corners, and mid-edges
                let sample_positions = [
                    (width / 2, height / 2),         // Center
                    (width / 4, height / 4),         // Top-left
                    (3 * width / 4, height / 4),     // Top-right
                    (width / 4, 3 * height / 4),     // Bottom-left
                    (3 * width / 4, 3 * height / 4), // Bottom-right
                    (width / 2, height / 4),         // Top-middle
                    (width / 2, 3 * height / 4),     // Bottom-middle
                    (width / 4, height / 2),         // Middle-left
                    (3 * width / 4, height / 2),     // Middle-right
                ];

                for (i, (x, y)) in sample_positions.iter().enumerate() {
                    let pixel_offset = (y * width + x) * 4;

                    if pixel_offset + 3 < image_data.len() {
                        let r = image_data[pixel_offset];
                        let g = image_data[pixel_offset + 1];
                        let b = image_data[pixel_offset + 2];

                        // Check if pixel has any color (any channel >= 10)
                        if r >= 10 || g >= 10 || b >= 10 {
                            all_pixels_black = false;
                            if first_non_black_pixel.is_none() {
                                first_non_black_pixel = Some((i, r, g, b, *x, *y));
                            }
                        }
                    }
                }

                if all_pixels_black {
                    log::error!(
                        "BLACK FRAME DETECTED at frame {}! All 9 sampled pixels are black",
                        prev_frame
                    );
                } else if let Some((i, r, g, b, x, y)) = first_non_black_pixel {
                    log::info!(
                        "Frame {} has color! Sample #{} at ({},{}): RGB({},{},{})",
                        prev_frame,
                        i,
                        x,
                        y,
                        r,
                        g,
                        b
                    );
                }
            } else if let Some(Err(e)) = readback_result {
                log::error!("Failed to check pending readback: {}", e);
            }
        }

        // Handle max_frames limit (after readback to ensure last frame's readback is queued)
        self.frame_count += 1;

        if let Some(max) = self.info.max_frames
            && self.frame_count >= max
        {
            info!("Rendered {} frames, exiting", self.frame_count);
            // Call cleanup directly since exiting() may not be triggered
            self.cleanup_on_exit();
            event_loop.exit();
        }

        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// Save frame data as PNG file for visual inspection
    pub(crate) fn save_frame_as_png(
        &self,
        frame: usize,
        bgra_data: &[u8],
        width: usize,
        height: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;
        use std::path::PathBuf;

        // Create frames directory if it doesn't exist
        let frames_dir = PathBuf::from("frames");
        fs::create_dir_all(&frames_dir)?;

        // Save as PNG using the image library
        let filename = frames_dir.join(format!("frame_{:04}.png", frame));

        // Convert from BGRA (swapchain format) to RGBA (PNG format)
        // The swapchain uses B8G8R8A8_SRGB format, so we need to swap channels
        // IMPORTANT: Force alpha to 255 (fully opaque) since swapchain is OPAQUE
        let rgba_data: Vec<u8> = bgra_data
            .chunks_exact(4)
            .flat_map(|bgra| {
                // BGRA -> RGBA conversion, force alpha to 255
                [bgra[2], bgra[1], bgra[0], 255]
            })
            .collect();

        // Create RGBA image buffer from the converted data
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(width as u32, height as u32, rgba_data)
                .ok_or("Failed to create image buffer from raw data")?;

        // Save to file (image crate will handle sRGB properly based on the ColorType)
        img.save(&filename)?;

        info!(
            "Saved frame {} to {:?} ({}x{} pixels, converted from BGRA_sRGB to RGBA, alpha forced to 255)",
            frame, filename, width, height
        );
        Ok(())
    }

    #[cfg(feature = "editor")]
    fn poll_asset_watcher(&mut self) {
        let Some(ref mut watcher) = self.asset_watcher else {
            return;
        };

        for change in watcher.poll_changes() {
            match change.kind {
                crate::util::AssetChangeKind::Shader => {
                    let count = self.renderer.recompile_materials_for_shader(&change.path);
                    if count > 0 {
                        info!(
                            "Hot reloaded shader: {} ({} material(s) recompiled)",
                            change.path.display(),
                            count
                        );
                    } else {
                        debug!(
                            "Shader changed: {} (no matching materials)",
                            change.path.display()
                        );
                    }
                }
                crate::util::AssetChangeKind::Texture => {
                    self.reload_texture(&change.path);
                }
                crate::util::AssetChangeKind::Script => {
                    // Script hot reload is handled by ScriptWatcher in katla_script
                }
            }
        }
    }

    /// Reload a texture from disk and update the GPU resource in-place.
    #[cfg(feature = "editor")]
    fn reload_texture(&mut self, path: &std::path::Path) {
        let handle = match self.editor.texture_paths.get(path).copied() {
            Some(h) => h,
            None => {
                debug!("Texture changed but not tracked: {}", path.display());
                return;
            }
        };

        let img = match image::open(path) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                warn!("Failed to reload texture '{}': {}", path.display(), e);
                return;
            }
        };

        match self.renderer.update_texture(handle, img.as_raw()) {
            Ok(()) => {
                info!(
                    "Hot reloaded texture: {} -> handle {}",
                    path.display(),
                    handle.index()
                );
            }
            Err(e) => {
                warn!(
                    "Failed to upload reloaded texture '{}': {}",
                    path.display(),
                    e
                );
            }
        }
    }

    #[cfg(not(feature = "editor"))]
    fn poll_asset_watcher(&mut self) {}
}
