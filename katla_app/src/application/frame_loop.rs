#[cfg(feature = "editor")]
use log::debug;
use log::{info, warn};

use katla_gfx::GpuRenderer;

use crate::application::Application;

impl Application {
    /// Export the compiled graph and native execution capture after the first frame.
    pub(crate) fn dump_render_graph_if_needed(&mut self) {
        if self.render_graph_dumped {
            return;
        }

        let Some(target) = &self.info.dump_render_graph else {
            return;
        };

        let mut capture = match self.frame_graph.capture() {
            Ok(capture) => capture,
            Err(error) => {
                log::error!("Failed to capture render graph: {error}");
                self.render_graph_dumped = true;
                return;
            }
        };
        if let Some(snapshot) = self.renderer.capture_submission_snapshot() {
            capture.backend_execution.frame = Some(snapshot);
        }
        for failure in &capture.comparison {
            log::error!("Render-graph capture divergence: {failure}");
        }
        let output = match target {
            super::DumpLayoutTarget::File(path)
                if std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext == "json") =>
            {
                match capture.to_json_pretty() {
                    Ok(json) => json,
                    Err(error) => {
                        log::error!("Failed to serialize render-graph capture: {error}");
                        return;
                    }
                }
            }
            super::DumpLayoutTarget::File(path)
                if std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext == "dot") =>
            {
                capture.to_dot()
            }
            _ => capture.to_string(),
        };

        match target {
            super::DumpLayoutTarget::Stdout => {
                println!("{output}");
            }
            super::DumpLayoutTarget::File(path) => {
                if let Err(error) = std::fs::write(path, &output) {
                    log::error!("Failed to write render-graph dump to {path}: {error}");
                } else {
                    log::info!("Render-graph dump written to {path}");
                }
            }
        }

        self.render_graph_dumped = true;
    }

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

        self.renderer.wait_for_device();
        match self.poll_frame_readback() {
            Ok(Some((frame, data))) => {
                if let Err(error) = self.save_frame_as_png(
                    frame,
                    &data.bytes,
                    data.size.width as usize,
                    data.size.height as usize,
                ) {
                    log::error!("Failed to save final frame {frame}: {error}");
                }
            }
            Ok(None) => {}
            Err(error) => log::error!("Failed to finish frame readback: {error}"),
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

        // Destroy the remaining device resources
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

        // Update audio system — process AudioEmitter components
        if let Some(ref mut audio) = self.audio_system {
            audio.update(&mut self.world, dt);
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

        // Render-graph dump: if requested, serialize the compiled graph, then exit.
        self.dump_render_graph_if_needed();

        // Asynchronous black frame checking:
        // - On frame N: Queue async readback (non-blocking)
        // - On frame N+1: Check if readback from frame N is complete and save to disk
        // This allows us to catch synchronization issues that synchronous readback would mask
        if self.info.check_black_frames && self.frame_count > 0 {
            let readback_result = self.poll_frame_readback();
            if self.frame_readback.is_none() {
                match self.queue_frame_readback() {
                    Ok(ticket) => self.frame_readback = Some((self.frame_count, ticket)),
                    Err(error) => log::error!("Failed to queue frame readback: {error}"),
                }
            }
            if let Ok(Some((prev_frame, data))) = &readback_result {
                let prev_frame = *prev_frame;
                let image_data = &data.bytes;
                let width = data.size.width as usize;
                let height = data.size.height as usize;
                // Save frame as PNG for visual inspection
                if let Err(e) = self.save_frame_as_png(prev_frame, image_data, width, height) {
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
            } else if let Err(e) = readback_result {
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

    pub(crate) fn queue_frame_readback(
        &mut self,
    ) -> crate::AppResult<katla_gfx::TextureReadbackTicket> {
        let source = self
            .frame_graph
            .resource_id("backbuffer")
            .and_then(|resource| self.renderer.graph_texture_source(resource))
            .ok_or_else(|| crate::AppError::Other {
                message: "Frame capture requires an exported committed backbuffer".into(),
            })?;
        Ok(self.renderer.queue_texture_readback(
            source,
            katla_gfx::TextureReadbackRegion {
                origin: [0, 0],
                size: self.renderer.swapchain_extent(),
                mip_level: 0,
                array_layer: 0,
            },
        )?)
    }

    fn poll_frame_readback(
        &mut self,
    ) -> crate::AppResult<Option<(usize, katla_gfx::TextureReadbackData)>> {
        let Some((frame, ticket)) = self.frame_readback else {
            return Ok(None);
        };
        match self.renderer.poll_texture_readback(ticket) {
            Ok(Some(data)) => {
                self.frame_readback = None;
                Ok(Some((frame, data)))
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.frame_readback = None;
                Err(error.into())
            }
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
            .as_chunks::<4>()
            .0
            .iter()
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
                            "Shader reload requested: {} ({} materials)",
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

impl Application {
    pub(crate) fn prepare_scene_gpu(
        &mut self,
        token: &katla_gfx::renderer::frame_scope::FrameToken,
        dt: f32,
        uniforms: &crate::rendering::FrameUniforms,
        draws: &katla_gfx::renderer::DrawList,
    ) -> crate::AppResult<()> {
        let Some(features) = &mut self.scene_features else {
            return Ok(());
        };
        if let Some(cpu) = &mut self.gpu_animation_system {
            features.animation.prepare_frame(
                &mut self.renderer,
                &mut self.frame_graph,
                token,
                &mut self.world,
                cpu,
            )?;
        }
        let scene_size = if self.panel_rt_size.width > 0 && self.panel_rt_size.height > 0 {
            self.panel_rt_size
        } else {
            self.renderer.swapchain_extent()
        };
        features.lights.prepare_frame(
            &mut self.renderer,
            &mut self.frame_graph,
            token,
            &self.point_lights_buffer,
            uniforms,
            scene_size,
        )?;
        features.particles.prepare_frame(
            &mut self.renderer,
            &mut self.frame_graph,
            token,
            &mut self.world,
            &mut self.particle_system,
            dt,
            self.frame_count as u32,
            uniforms,
        )?;
        #[cfg(feature = "editor")]
        let selected = self
            .editor
            .editor_ui
            .selected_entity
            .map(|entity| self.collect_selected_instance_indices(entity))
            .unwrap_or_default();
        #[cfg(not(feature = "editor"))]
        let selected = Vec::new();
        let Some(features) = &mut self.scene_features else {
            return Ok(());
        };
        let skinning = features.animation.skinning_accesses().to_vec();
        for name in [
            &self.frame_graph_bindings.passes.geometry,
            &self.frame_graph_bindings.passes.depth_prepass,
            &self.frame_graph_bindings.passes.picking,
            &self.frame_graph_bindings.passes.shadow,
            &self.frame_graph_bindings.passes.outline,
            &self.frame_graph_bindings.passes.stencil_indicator,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(pass) = self.frame_graph.pass_id(name) {
                let mut accesses = skinning.clone();
                if self.frame_graph_bindings.passes.geometry.as_deref() == Some(name.as_str()) {
                    accesses.extend(features.lights.graphics_accesses()?);
                }
                self.frame_graph
                    .set_pass_commands(pass, Vec::new(), accesses)?;
            }
        }
        if let Some(pass) = self.frame_graph.pass_id("particles") {
            self.frame_graph.set_pass_commands(
                pass,
                Vec::new(),
                features.particles.graphics_accesses()?,
            )?;
        }
        let mut ordinary = Vec::new();
        let mut billboards = Vec::new();
        for draw in draws.iter() {
            let target = if draw.is_billboard {
                &mut billboards
            } else {
                &mut ordinary
            };
            target.extend(draw.base_object_slot()..draw.base_object_slot() + draw.instance_count());
        }
        features.graphics.prepare_frame(
            &mut self.frame_graph,
            super::scene_features::GraphicsFrame {
                ids: &self.pass_ids,
                bindings: &self.frame_graph_bindings,
                uniforms,
                lights: &features.lights,
                particles: &features.particles,
                selected,
                ordinary,
                billboards,
                frame_slot: token.slot(),
                scene_size,
            },
        )?;
        features
            .animation
            .retire_unused_imports(&mut self.frame_graph)?;
        Ok(())
    }
}
