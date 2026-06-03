//! Headless rendering constants and screenshot helpers.
//!
//! The headless frame loop is now driven by `Application::run_headless()`
//! which reuses the exact same Application code (editor UI, scene, frame loop)
//! as windowed mode, with only the window/drawable swapped for an offscreen texture.

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
#[cfg(target_os = "macos")]
use katla_gfx::MetalTextureRetained;
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

        let mut ui_test = self
            .info
            .ui_test_path
            .as_ref()
            .map(|dir| crate::application::ui_test::UiTestRunner::new(dir.clone()));

        info!(
            "Running {} headless frames at {}x{}",
            max_frames, HEADLESS_WIDTH, HEADLESS_HEIGHT
        );

        // Run the on_init hook
        if let Some(hook) = self.on_init.take() {
            hook(self);
        }

        // Keep a reference to the last rendered offscreen texture for readback.
        // The renderer takes ownership of the drawable texture during render_frame
        // (via .take()), so we must clone it beforehand.
        #[cfg(target_os = "macos")]
        let mut last_offscreen: Option<MetalTextureRetained> = None;

        for frame in 0..max_frames {
            #[cfg(target_os = "macos")]
            {
                last_offscreen = self.run_one_headless_frame();
            }

            // UI test: check for screenshot and inject state changes
            #[cfg(all(target_os = "macos", feature = "editor"))]
            if let Some(ref mut runner) = ui_test {
                if let Some(screenshot_dest) = runner.on_frame(
                    frame,
                    &mut self.editor.editor_ui.selected_entity,
                    &self.world,
                ) {
                    self.save_headless_screenshot(&screenshot_dest, last_offscreen.clone())?;
                }
            }

            self.frame_count += 1;

            #[cfg(all(target_os = "macos", not(feature = "editor")))]
            let _ = &mut ui_test;
        }

        // Wait for GPU to finish the last frame
        if let Err(e) = self.renderer.wait_for_frame() {
            log::error!("Failed to wait for frame: {}", e);
        }

        // Save screenshot from the last frame's offscreen texture (standard mode only)
        #[cfg(target_os = "macos")]
        if ui_test.is_none() {
            self.save_headless_screenshot(&screenshot_path, last_offscreen)?;
        }

        // Layout dump (if both --headless and --dump-layout are set)
        self.dump_layout_if_needed();

        // Cleanup
        self.cleanup_on_exit();

        if let Some(ref runner) = ui_test {
            info!(
                "UI test complete: {} screenshots saved to {}",
                runner.screenshots_taken(),
                self.info.ui_test_path.as_deref().unwrap_or("?")
            );
        } else {
            info!("Headless render complete");
        }

        Ok(())
    }

    /// Render one headless frame. Returns the offscreen texture that was rendered to.
    #[cfg(target_os = "macos")]
    fn run_one_headless_frame(&mut self) -> Option<MetalTextureRetained> {
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

        // Create a fresh offscreen texture and set as drawable for this frame.
        // Clone it before passing to the renderer — the renderer takes ownership
        // via .take() during render_frame, but the Shared-storage texture persists
        // on the GPU and the clone remains valid for readback.
        let offscreen = self
            .renderer
            .create_offscreen_texture(HEADLESS_WIDTH, HEADLESS_HEIGHT);
        let offscreen_clone = offscreen.clone();
        self.renderer.set_headless_drawable(offscreen);

        // Render editor frame (same as windowed — includes UI generation)
        self.render_editor_frame(dt);

        Some(offscreen_clone)
    }

    #[cfg(target_os = "macos")]
    fn save_headless_screenshot(
        &self,
        path: &str,
        texture: Option<MetalTextureRetained>,
    ) -> AppResult<()> {
        let Some(texture) = texture else {
            log::error!("No offscreen texture available for screenshot");
            return Err(crate::error::AppError::Other {
                message: "No offscreen texture available".to_string(),
            });
        };

        let bgra_data =
            crate::Renderer::readback_bgra_texture(&texture, HEADLESS_WIDTH, HEADLESS_HEIGHT);

        // Convert BGRA to RGBA for PNG
        let rgba_data: Vec<u8> = bgra_data
            .chunks_exact(4)
            .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], 255])
            .collect();

        // Encode PNG
        let mut png_data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_data, HEADLESS_WIDTH, HEADLESS_HEIGHT);
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
            HEADLESS_WIDTH,
            HEADLESS_HEIGHT,
            png_data.len()
        );

        Ok(())
    }
}
