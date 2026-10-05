//! Headless scene/editor rendering and PNG capture on Vulkan and Metal.

/// Headless offscreen texture dimensions (physical pixels).
///
/// Uses 2x resolution (2560x1440) with scale_factor=2 to match Retina rendering,
/// ensuring text and UI elements render at full quality. The UI layout operates in
/// logical coordinates (1280x720).
pub const HEADLESS_WIDTH: u32 = 2560;
pub const HEADLESS_HEIGHT: u32 = 1440;

/// DPI scale factor for headless rendering. Matches Retina (2x) so font
/// rasterization and UI sizing are identical to windowed mode.
pub const HEADLESS_SCALE_FACTOR: f32 = 2.0;

use crate::application::Application;
use crate::error::AppResult;
use katla_gfx::GpuRenderer;
use log::info;

impl Application {
    /// Run the headless frame loop: render N frames and save a screenshot.
    ///
    /// Uses the same frame logic as windowed mode (same scene, same editor UI,
    /// same render graph) but with an offscreen texture instead of a window drawable.
    pub fn run_headless(&mut self) -> AppResult<()> {
        let max_frames = self.info.max_frames.unwrap_or(10);
        let screenshot_path = self
            .info
            .screenshot_path
            .clone()
            .unwrap_or_else(|| "/tmp/katla_screenshot.png".to_string());

        let ui_test = self
            .info
            .ui_test_path
            .as_ref()
            .map(|dir| crate::application::ui_test::UiTestRunner::new(dir.clone()));
        #[cfg(feature = "editor")]
        let mut ui_test = ui_test;

        let interaction_test = self.info.interaction_test_path.as_ref().map(|dir| {
            crate::application::interaction_test::InteractionTestRunner::new(dir.clone())
        });
        #[cfg(feature = "editor")]
        let mut interaction_test = interaction_test;

        info!(
            "Running {} headless frames at {}x{}",
            max_frames, HEADLESS_WIDTH, HEADLESS_HEIGHT
        );

        // Run the on_init hook
        if let Some(hook) = self.on_init.take() {
            hook(self);
        }

        #[cfg(target_os = "macos")]
        let mut frame_metrics = Vec::with_capacity(max_frames);

        for _frame in 0..max_frames {
            #[cfg(target_os = "macos")]
            let frame_started = std::time::Instant::now();
            // Interaction test: inject synthetic input before this frame renders.
            #[cfg(feature = "editor")]
            if let Some(ref mut runner) = interaction_test {
                runner.begin_frame(self, _frame);
            }

            #[cfg(target_os = "macos")]
            {
                self.run_one_headless_frame();
                if let Some(renderer) = self.renderer.as_metal() {
                    let metrics = renderer.frame_metrics();
                    frame_metrics.push([
                        frame_started.elapsed().as_secs_f64() * 1_000_000.0,
                        metrics.cpu_submit.as_secs_f64() * 1_000_000.0,
                        metrics.slot_wait.as_secs_f64() * 1_000_000.0,
                        metrics.gpu_frame_time.as_secs_f64() * 1_000_000.0,
                        metrics.cpu_lead as f64,
                        metrics.in_flight as f64,
                    ]);
                }
            }
            #[cfg(not(target_os = "macos"))]
            self.run_one_headless_frame();

            // Interaction test: run checks and capture screenshots.
            #[cfg(feature = "editor")]
            if let Some(ref mut runner) = interaction_test
                && let Some(screenshot_dest) = runner.end_frame(self, _frame)
            {
                self.save_headless_screenshot(&screenshot_dest)?;
            }

            // UI test: check for screenshot and inject state changes
            #[cfg(feature = "editor")]
            if let Some(ref mut runner) = ui_test
                && let Some(screenshot_dest) =
                    runner.on_frame(_frame, &mut self.editor.editor_ui, &self.world)
            {
                self.save_headless_screenshot(&screenshot_dest)?;
            }

            self.frame_count += 1;
        }

        #[cfg(target_os = "macos")]
        info!(
            "Metal headless frame samples (frame/submit/slot-wait/completed-GPU microseconds, CPU-lead, in-flight): {}",
            serde_json::json!(frame_metrics)
        );

        // Save screenshot from the last frame's offscreen texture (standard mode only)
        if ui_test.is_none() && interaction_test.is_none() {
            self.save_headless_screenshot(&screenshot_path)?;
        }

        // Layout dump (if both --headless and --dump-layout are set)
        self.dump_layout_if_needed();

        // Render-graph dump (if --dump-render-graph[-file] is set)
        self.dump_render_graph_if_needed();

        // Cleanup
        self.cleanup_on_exit();

        if let Some(ref runner) = ui_test {
            info!(
                "UI test complete: {} screenshots saved to {}",
                runner.screenshots_taken(),
                self.info.ui_test_path.as_deref().unwrap_or("?")
            );
        } else if let Some(ref runner) = interaction_test {
            runner.log_summary();
            #[cfg(feature = "editor")]
            runner.validate()?;
            info!(
                "Interaction test screenshots saved to {}",
                self.info.interaction_test_path.as_deref().unwrap_or("?")
            );
        } else {
            info!("Headless render complete");
        }

        Ok(())
    }

    /// Render one headless frame into the device's offscreen drawable.
    fn run_one_headless_frame(&mut self) {
        self.timer.add_timestamp();
        let dt = self.timer.get_delta() as f32;

        // Sync editor camera speed
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

        // ECS systems
        self.world.update_parallel(dt);

        // Clear per-frame input
        if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
            input.mouse_delta = (0.0, 0.0);
            input.mouse_wheel_delta = 0.0;
        }

        // Script audio commands (no-op without audio system in headless)
        {
            let _ = self
                .world
                .get_resource_mut::<katla_script::PendingAudioCommands>()
                .map(|r| std::mem::take(&mut r.0));
        }
        // Script raycast commands
        {
            let _ = self
                .world
                .get_resource_mut::<katla_script::PendingRaycastCommands>()
                .map(|r| std::mem::take(&mut r.0));
        }

        crate::particle_control::process_script_commands(&mut self.world);

        // Run per-frame update hook
        if let Some(ref mut hook) = self.on_update {
            hook(&mut self.world, dt);
        }

        // Process ECS events for GPU cleanup
        crate::gpu_cleanup::process_gpu_cleanup_events(
            &self.world,
            &mut self.gpu_resource_tracker,
            &mut self.renderer,
        );

        // Headless Metal needs an offscreen drawable for each submission.
        #[cfg(target_os = "macos")]
        let offscreen = {
            let extent = self.renderer.swapchain_extent();
            self.renderer
                .create_offscreen_texture(extent.width, extent.height)
        };
        #[cfg(target_os = "macos")]
        self.renderer.set_headless_drawable(offscreen);

        self.poll_background_loader();

        // Render editor frame (same as windowed — includes UI generation)
        self.render_editor_frame(dt);
    }

    fn save_headless_screenshot(&mut self, path: &str) -> AppResult<()> {
        let ticket = self.queue_frame_readback()?;
        self.renderer.wait_for_device();
        let data = self
            .renderer
            .poll_texture_readback(ticket)?
            .ok_or_else(|| crate::AppError::Other {
                message: "Frame capture did not complete after waiting for the device".into(),
            })?;
        let bgra_data = data.bytes;

        // Convert BGRA to RGBA for PNG
        let rgba_data: Vec<u8> = bgra_data
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], 255])
            .collect();

        // Encode PNG
        let mut png_data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_data, data.size.width, data.size.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| crate::error::AppError::Other {
                    message: format!("PNG encode error: {}", e),
                })?;
            writer
                .write_image_data(&rgba_data)
                .map_err(|e| crate::error::AppError::Other {
                    message: format!("PNG write error: {}", e),
                })?;
        }

        std::fs::write(path, &png_data).map_err(|e| crate::error::AppError::Other {
            message: format!("Failed to write screenshot: {}", e),
        })?;

        info!(
            "Saved screenshot to {} ({}x{}, {} bytes)",
            path,
            data.size.width,
            data.size.height,
            png_data.len()
        );

        Ok(())
    }
}
