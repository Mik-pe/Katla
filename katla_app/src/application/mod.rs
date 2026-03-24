//! Application module - main application lifecycle and event handling.
//!
//! This module contains the main [`Application`] struct and its implementation
//! of [`ApplicationHandler`] for winit event handling. The heavy lifting is
//! delegated to submodules:
//!
//! - [`builder`] - Application builder pattern
//! - [`renderer`] - Render graph setup and frame rendering
//! - [`editor`] - UI rendering and entity management for the editor

pub mod builder;
mod camera;
pub mod editor;
mod renderer;
mod spawning;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Instant};

use log::{debug, info, warn};
use winit::keyboard::ModifiersState;

pub use builder::*;
use katla_ecs::{World, input::Action};
use katla_gfx::renderer::VulkanRenderer;
use katla_math::Vec2;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use self::camera::Camera;
use crate::{
    gui_state::GuiState,
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    preferences::Preferences,
    resources::ResourceManager,
    util::{BackgroundLoader, FileCache, GLTFModel, Timer},
};

pub struct ApplicationInfo {
    name: String,
    validation_mode: katla_gfx::ValidationMode,
    max_frames: Option<usize>, // Some(n) = exit after n frames, None = run indefinitely
    check_black_frames: bool,  // Check center pixel of swapchain for black frames
}

/// Main application struct containing all engine state.
pub struct Application {
    pub(crate) window: Window,
    pub(crate) renderer: VulkanRenderer,
    /// Frame graph for rendering (built once at startup)
    pub(crate) frame_graph: katla_gfx::FrameGraph,
    pub(crate) camera: Rc<RefCell<Camera>>,
    #[allow(clippy::type_complexity)]
    pub(crate) gltf_cache: FileCache<GLTFModel, Box<dyn Fn(&PathBuf) -> GLTFModel>>,
    pub(crate) timer: Timer,
    pub(crate) info: ApplicationInfo,
    pub(crate) world: World,
    pub(crate) input_mapper: InputMapper,
    pub(crate) current_modifiers: ModifiersState,
    pub(crate) frame_count: usize,
    pub(crate) resources: ResourceManager,
    /// Immediate mode UI context
    pub(crate) ui_context: katla_ui::UiContext,
    /// UI renderer for converting UI draw lists to GPU format
    pub(crate) ui_renderer: crate::ui::UIRenderer,
    /// Debug overlay UI (simplified stats)
    pub(crate) debug_overlay: crate::ui::DebugOverlay,
    /// Game engine editor UI
    pub(crate) editor_ui: crate::ui::EditorUI,
    /// Use editor UI mode (vs simple debug overlay)
    pub(crate) use_editor_ui: bool,
    /// User preferences (theme, settings)
    pub(crate) preferences: Preferences,
    /// GUI layout state (panel sizes, positions)
    pub(crate) gui_state: GuiState,
    /// DPI scale factor (physical pixels per logical pixel)
    pub(crate) scale_factor: f32,
    /// Background asset loader thread
    pub(crate) background_loader: BackgroundLoader,
    /// Mapping of thumbnail paths to their uploaded texture handles
    pub(crate) thumbnail_texture_handles: HashMap<PathBuf, katla_gfx::TextureHandle>,
    /// Application start time for double-click timestamp calculation
    pub(crate) start_time: Instant,
    /// Default PBR material handle for geometry rendering
    pub(crate) default_material_handle: katla_gfx::MaterialHandle,
    /// Flag to prevent double cleanup
    cleaned_up: bool,
    /// Particle system for managing particle emitters via ECS
    pub(crate) particle_system: crate::systems::ParticleSystem,
    /// Flag to trigger particle debug readback at frame 10
    pub(crate) particle_readback_pending: bool,
    /// Flag to ensure particle debug readback only happens once
    pub(crate) particle_readback_done: bool,
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Enable IME for text input (required for receiving text input events)
        self.window.set_ime_allowed(true);

        // Get initial DPI scale factor
        self.scale_factor = self.window.scale_factor() as f32;
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.world.get_input().is_action_pressed(Action::LookEnable) {
                let current_delta = self.world.get_input().mouse_delta;
                self.world.get_input_mut().mouse_delta = (
                    current_delta.0 + delta.0 as f32,
                    current_delta.1 + delta.1 as f32,
                );
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::MouseInput { state, button, .. } = &event {
            let mouse_combo = MouseCombo::with_modifiers(*button, self.current_modifiers);
            let binding = InputBinding::Mouse(mouse_combo);

            if let Some(action) = self.input_mapper.get_action(&binding) {
                // Only send mouse input to game when viewport is focused
                if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                    let pressed = matches!(state, ElementState::Pressed);
                    self.world.get_input_mut().set_action_state(action, pressed);
                }
            }

            let ui_button = match button {
                winit::event::MouseButton::Left => Some(katla_ui::input::mouse_button::LEFT),
                winit::event::MouseButton::Right => Some(katla_ui::input::mouse_button::RIGHT),
                winit::event::MouseButton::Middle => Some(katla_ui::input::mouse_button::MIDDLE),
                _ => None,
            };
            if let Some(btn) = ui_button {
                let pressed = matches!(state, ElementState::Pressed);
                let time = self.start_time.elapsed().as_secs_f64();
                self.ui_context
                    .input
                    .set_mouse_button_with_time(btn, pressed, time);
            }
        }

        match event {
            WindowEvent::Resized(logical_size) => {
                let new_width = logical_size.width;
                let new_height = logical_size.height as f32;

                if new_width > 0 && new_height > 0.0 {
                    // Recreate swapchain and transient textures
                    let recreated_textures =
                        self.renderer.recreate_swapchain(&mut self.frame_graph);

                    let extent = self.renderer.swapchain_extent();

                    // Update bindless indices for recreated textures
                    for (name, slot) in recreated_textures {
                        if name == "hdr_color" {
                            self.frame_graph
                                .set_tonemap_texture_index("tonemap", slot)
                                .expect("Failed to update tonemap texture index");
                        } else if name == "viewport_0" {
                            self.editor_ui.set_viewport_bindless_index(slot);
                        }
                    }

                    // Update shadow atlas views for all frames if recreated
                    for frame_idx in 0..2 {
                        if let Some(view) = self
                            .frame_graph
                            .transient_texture_view_for_frame("shadow_atlas", frame_idx)
                        {
                            self.renderer.set_shadow_atlas_view(frame_idx, view);
                        }
                    }

                    let aspect = extent.width as f32 / extent.height as f32;
                    self.camera
                        .borrow_mut()
                        .aspect_ratio_changed(&mut self.world, aspect);
                    info!("=== Resize complete ===");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Convert physical pixels to logical pixels for UI
                let logical_x = position.x as f32 / self.scale_factor;
                let logical_y = position.y as f32 / self.scale_factor;
                self.ui_context
                    .input
                    .set_mouse_pos(Vec2::new(logical_x, logical_y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x * 20.0, y * 20.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Convert physical to logical pixels
                        Vec2::new(
                            pos.x as f32 / self.scale_factor,
                            pos.y as f32 / self.scale_factor,
                        )
                    }
                };
                self.ui_context.input.scroll_delta = scroll;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    let key_combo = KeyCombo::with_modifiers(keycode, self.current_modifiers);
                    let binding = InputBinding::Keyboard(key_combo);

                    // Toggle particle inspector with Ctrl+P
                    if event.state == ElementState::Pressed
                        && keycode == KeyCode::KeyP
                        && self.current_modifiers.control_key()
                    {
                        let state = &mut self.editor_ui.particle_inspector_state;
                        if state.panel.is_visible() {
                            state.panel.close();
                        } else {
                            state.panel.open();
                        }
                        info!(
                            "Particle inspector: {}",
                            if state.panel.is_visible() {
                                "visible"
                            } else {
                                "hidden"
                            }
                        );
                    }

                    if let Some(action) = self.input_mapper.get_action(&binding) {
                        // Only send keyboard input to game when viewport is focused
                        if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                            let pressed = matches!(event.state, ElementState::Pressed);
                            self.world.get_input_mut().set_action_state(action, pressed);
                        }
                    }

                    let ui_key = Self::winit_to_ui_key(keycode);
                    if let Some(key) = ui_key {
                        match event.state {
                            ElementState::Pressed => self.ui_context.input.add_key_press(key),
                            ElementState::Released => self.ui_context.input.add_key_release(key),
                        }
                    }

                    // Handle text input from key event (for UI text fields)
                    if event.state == ElementState::Pressed {
                        if let Some(text) = &event.text {
                            for c in text.chars() {
                                self.ui_context.input.add_char(c);
                            }
                        }
                    }

                    if event.state == ElementState::Pressed
                        && self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                        && keycode == KeyCode::Escape
                    {
                        event_loop.exit()
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.current_modifiers = modifiers.state();
            }
            WindowEvent::Ime(event) => {
                // Handle text input for UI widgets (text fields, search filters, etc.)
                if let winit::event::Ime::Preedit(_, _) | winit::event::Ime::Commit(_) = event {
                    // For Commit events, add each character to the UI input
                    if let winit::event::Ime::Commit(text) = event {
                        for c in text.chars() {
                            self.ui_context.input.add_char(c);
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor as f32;
                debug!("DPI scale factor changed to {}", self.scale_factor);
            }
            WindowEvent::RedrawRequested => {
                debug!("RedrawRequested (frame {})", self.frame_count);
                self.timer.add_timestamp();
                let dt = self.timer.get_delta() as f32;

                // Update world (runs animation systems)
                debug!("Updating world...");
                self.world.update(dt);
                debug!("World updated");

                // Update particle emitters from ECS components
                self.particle_system.update(
                    &mut self.world,
                    &mut self.renderer.particle_system,
                    dt,
                );

                // Poll background loader for completed asset loads
                self.poll_background_loader();

                // DEBUG: Test particle readback at frame 10
                #[cfg(debug_assertions)]
                {
                    if self.frame_count == 10 {
                        if let Some(ref particle_system) = self.renderer.particle_system {
                            log::info!(
                                "=== Attempting Particle Debug Readback at frame {} ===",
                                self.frame_count
                            );

                            // Read debug data from staging buffers
                            match particle_system.read_debug_data() {
                                Ok(debug_data) => {
                                    log::info!("Particle Summary: {}", debug_data.summary());

                                    // Print first 10 particles
                                    log::info!("=== First 10 Particles ===");
                                    for (i, p) in debug_data.particles.iter().take(10).enumerate() {
                                        log::info!(
                                            "Particle {}: pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) lifetime={:.2} scale={:.3} color=({:.2},{:.2},{:.2},{:.2})",
                                            i,
                                            p.position[0],
                                            p.position[1],
                                            p.position[2],
                                            p.velocity[0],
                                            p.velocity[1],
                                            p.velocity[2],
                                            p.lifetime,
                                            p.scale,
                                            p.color[0],
                                            p.color[1],
                                            p.color[2],
                                            p.color[3]
                                        );
                                    }

                                    // Print alive particle indices
                                    log::info!("=== First 10 Alive Particle Indices ===");
                                    for (i, idx) in
                                        debug_data.alive_list.iter().take(10).enumerate()
                                    {
                                        log::info!("Alive[{}] = {}", i, idx);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to read particle debug data: {}", e);
                                }
                            }
                        }
                    }
                }

                // Note: Transient textures are double-buffered (one per FRAMES_IN_FLIGHT).
                // The viewport bindless index must be updated BEFORE generating the UI
                // draw list so the UI samples from the correct per-frame texture.
                // Doing it after would cause an off-by-one mismatch: the UI would
                // sample from the previous frame's stale texture.
                {
                    let frame_idx = self.renderer.current_frame();
                    if let Some(base_ldr_index) = self.frame_graph.get_ldr_texture_base_index() {
                        let actual_ldr_index = base_ldr_index + frame_idx as u32;
                        self.editor_ui.set_viewport_bindless_index(actual_ldr_index);
                    }
                }

                // Generate UI draw list BEFORE frame graph execution
                debug!("Generating UI draw list...");
                let ui_draw_list = editor::generate_ui_draw_list(self, dt);
                debug!("UI draw list generated");

                // Upload font atlas AFTER draw list generation (which rasterizes new glyphs)
                // and BEFORE render_frame (which samples from the GPU atlas).
                // Doing it after render_frame would cause a one-frame lag where text
                // samples from stale GPU data.
                editor::upload_font_atlas(self);

                // Render frame to GPU (includes UI if present)
                debug!("Rendering frame...");
                self.render_frame(ui_draw_list, dt, self.frame_count);
                debug!("Frame rendered");

                // Process editor actions after UI rendering
                editor::process_editor_actions(self);

                // Asynchronous black frame checking:
                // - On frame N: Queue async readback (non-blocking)
                // - On frame N+1: Check if readback from frame N is complete and save to disk
                // This allows us to catch synchronization issues that synchronous readback would mask
                if self.info.check_black_frames && self.frame_count > 0 {
                    // Check if previous frame's async readback is complete
                    match self.renderer.check_pending_readback() {
                        Ok(Some((prev_frame, image_data))) => {
                            let extent = self.renderer.swapchain_extent();
                            let width = extent.width as usize;
                            let height = extent.height as usize;

                            // Save frame as PNG for visual inspection
                            if let Err(e) =
                                self.save_frame_as_png(prev_frame, &image_data, width, height)
                            {
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
                        }
                        Ok(None) => {}
                        Err(e) => {
                            log::error!("Failed to check pending readback: {}", e);
                        }
                    }

                    // Queue async readback for current frame (will be checked on next frame)
                    if let Err(e) = self.renderer.queue_async_readback(self.frame_count) {
                        log::error!(
                            "Frame {} - Failed to queue async readback: {}",
                            self.frame_count,
                            e
                        );
                    }
                }

                // Handle max_frames limit (after readback to ensure last frame's readback is queued)
                if let Some(max) = self.info.max_frames {
                    self.frame_count += 1;
                    if self.frame_count >= max {
                        info!("Rendered {} frames, exiting", self.frame_count);
                        // Call cleanup directly since exiting() may not be triggered
                        self.cleanup_on_exit();
                        event_loop.exit();
                    }
                }

                self.window.request_redraw();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        info!("Application exiting - cleaning up...");
        self.cleanup_on_exit();
    }
}

impl Application {
    /// Cleanup resources on exit.
    /// Called both from exiting() and directly before event_loop.exit() for max_frames mode.
    fn cleanup_on_exit(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        // Wait for any pending async readback to complete before destroying resources
        // This must happen BEFORE wait_for_device() to ensure readback finishes
        match self.renderer.wait_for_pending_readback() {
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

        // Save preferences before exit
        if let Err(e) = self.preferences.save() {
            warn!("Failed to save preferences: {}", e);
        } else {
            info!("Saved preferences to disk");
        }

        // Save GUI state before exit
        self.gui_state.left_panel_width = self.editor_ui.left_panel_width;
        self.gui_state.right_panel_width = self.editor_ui.right_panel_width;
        self.gui_state.asset_browser_height = self.editor_ui.asset_browser.panel_height;

        if let Err(e) = self.gui_state.save() {
            warn!("Failed to save GUI state: {}", e);
        } else {
            info!("Saved GUI state to disk");
        }

        // Wait for device to ensure all GPU operations are complete
        self.renderer.wait_for_device();

        // Cleanup frame graph transient textures BEFORE destroying renderer
        // This ensures proper cleanup order and avoids heap corruption during shutdown
        self.frame_graph.cleanup();

        // Destroy renderer (which owns the particle system)
        self.renderer.destroy();
    }
}

impl Application {
    pub fn init(&mut self) {
        // Logger is now initialized in main() before building the application
        println!("Application::init() called");

        // Initialize default PBR material
        let shader_path = self.resources.shader_path("model_pbr.wgsl");
        info!(
            "Loading default PBR material from: {}",
            shader_path.display()
        );

        // Create HDR PBR material for rendering to HDR intermediate
        self.default_material_handle = self
            .renderer
            .compile_material(
                &shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    ..Default::default()
                },
            )
            .expect("Failed to create default HDR PBR material");

        info!("Default HDR PBR material loaded successfully");

        // Initialize particle emit pipeline
        let particle_emit_shader_path = self.resources.shader_path("particles/particle_emit.wgsl");
        self.renderer
            .init_particle_emit_pipeline(&particle_emit_shader_path)
            .expect("Failed to initialize particle emit pipeline");

        // Initialize particle simulate pipeline
        let particle_simulate_shader_path = self
            .resources
            .shader_path("particles/particle_simulate.wgsl");
        self.renderer
            .init_particle_simulate_pipeline(&particle_simulate_shader_path)
            .expect("Failed to initialize particle simulate pipeline");

        // Initialize particle draw command pipeline (writes indirect draw buffer after simulate)
        let particle_draw_command_shader_path = self
            .resources
            .shader_path("particles/particle_draw_command.wgsl");
        self.renderer
            .init_particle_draw_command_pipeline(&particle_draw_command_shader_path)
            .expect("Failed to initialize particle draw command pipeline");

        // Add particle compute passes to frame graph
        // These must be added after particle pipelines are initialized
        if let Some(ref particle_system) = self.renderer.particle_system {
            let emit_pipeline = particle_system
                .emit_pipeline_handle()
                .expect("Particle emit pipeline not initialized");
            let simulate_pipeline = particle_system
                .simulate_pipeline_handle()
                .expect("Particle simulate pipeline not initialized");

            use katla_gfx::render_graph::PassDesc;
            use katla_gfx::render_graph::PassType;

            // Add particle emit pass
            self.frame_graph.add_pass(
                PassDesc::new("particle_emit", PassType::Compute, vec![], vec![])
                    .with_pipeline(emit_pipeline),
            );

            // Add particle simulate pass
            self.frame_graph.add_pass(
                PassDesc::new("particle_simulate", PassType::Compute, vec![], vec![])
                    .with_pipeline(simulate_pipeline),
            );

            info!("Added particle compute passes to frame graph");
        }

        // Initialize transient textures and register with bindless system
        self.frame_graph
            .initialize_transient_textures(&self.renderer)
            .expect("Failed to initialize transient textures");

        // Register HDR texture with bindless system for tonemapping
        let hdr_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "hdr_color")
            .expect("Failed to register HDR texture with bindless system");

        info!(
            "HDR texture registered with bindless system at index {}",
            hdr_bindless_index
        );

        // Set HDR texture index on tonemap pass
        self.frame_graph
            .set_tonemap_texture_index("tonemap", hdr_bindless_index)
            .expect("Failed to set tonemap texture index");

        // Register viewport texture with bindless system for viewport rendering
        let viewport_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "viewport_0")
            .expect("Failed to register viewport texture with bindless system");

        info!(
            "Viewport texture registered with bindless system at index {}",
            viewport_bindless_index
        );

        // Set LDR texture base index for compositing shader to use
        self.frame_graph
            .set_ldr_texture_base_index(viewport_bindless_index);

        // Set viewport bindless index in editor UI
        self.editor_ui
            .set_viewport_bindless_index(viewport_bindless_index);

        // Set up default test scene
        self.setup_default_scene();

        println!("Application::init() completed");
    }

    /// Get the default PBR material handle.
    pub fn default_material(&self) -> katla_gfx::MaterialHandle {
        self.default_material_handle
    }

    /// Poll the background loader and process completed loads.
    fn poll_background_loader(&mut self) {
        use crate::ui::ThumbnailState;
        use crate::util::LoadResult;

        let results = self.background_loader.poll();

        for result in results {
            match result {
                LoadResult::ImageThumbnailLoaded {
                    path,
                    width,
                    height,
                    pixels,
                    ..
                } => {
                    debug!("Thumbnail loaded: {:?} ({}x{})", path, width, height);

                    // Upload texture to renderer and get TextureHandle
                    // Use SRGB format for correct color rendering in UI
                    let desc = katla_gfx::TextureDescriptor::rgba8_srgb(width, height);
                    let texture_handle = self.renderer.create_texture(&desc, &pixels);

                    // Get the bindless slot for this texture
                    let bindless_slot = self
                        .renderer
                        .texture_manager
                        .get_bindless_slot(texture_handle)
                        .unwrap_or_else(|| {
                            log::warn!(
                                "Thumbnail texture {:?} (handle {}) has no bindless slot",
                                path,
                                texture_handle.index()
                            );
                            0 // Fallback to slot 0
                        });

                    // Register the bindless slot with the UI renderer
                    self.ui_renderer
                        .register_bindless_slot(texture_handle, bindless_slot);

                    // Update the thumbnail cache entry
                    if let Some(entry) = self.background_loader.get_thumbnail_mut(&path) {
                        entry.uploaded = true;
                    }

                    // Store texture handle for this path (persists across directory navigations)
                    self.thumbnail_texture_handles
                        .insert(path.clone(), texture_handle);

                    // Update asset browser entries with this thumbnail
                    for asset in self.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Loaded { texture_handle };
                            debug!(
                                "Updated thumbnail state for {:?} with handle {}, bindless slot {}",
                                path,
                                texture_handle.index(),
                                bindless_slot
                            );
                            break;
                        }
                    }
                }
                LoadResult::Failed { path, error, .. } => {
                    warn!("Failed to load {:?}: {}", path, error);

                    // Update asset browser entry to show failed state
                    for asset in self.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Failed;
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Save frame data as PNG file for visual inspection
    fn save_frame_as_png(
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

    /// Convert winit KeyCode to UI KeyCode.
    fn winit_to_ui_key(keycode: KeyCode) -> Option<katla_ui::input::KeyCode> {
        use katla_ui::input::KeyCode as UiKey;
        Some(match keycode {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => UiKey::Shift,
            KeyCode::ControlLeft | KeyCode::ControlRight => UiKey::Control,
            KeyCode::AltLeft | KeyCode::AltRight => UiKey::Alt,
            KeyCode::SuperLeft | KeyCode::SuperRight => UiKey::Super,
            KeyCode::Tab => UiKey::Tab,
            KeyCode::ArrowLeft => UiKey::ArrowLeft,
            KeyCode::ArrowRight => UiKey::ArrowRight,
            KeyCode::ArrowUp => UiKey::ArrowUp,
            KeyCode::ArrowDown => UiKey::ArrowDown,
            KeyCode::Home => UiKey::Home,
            KeyCode::End => UiKey::End,
            KeyCode::PageUp => UiKey::PageUp,
            KeyCode::PageDown => UiKey::PageDown,
            KeyCode::Enter | KeyCode::NumpadEnter => UiKey::Enter,
            KeyCode::Escape => UiKey::Escape,
            KeyCode::Backspace => UiKey::Backspace,
            KeyCode::Delete => UiKey::Delete,
            KeyCode::Insert => UiKey::Insert,
            KeyCode::Space => UiKey::Space,
            KeyCode::KeyA => UiKey::A,
            _ => return None,
        })
    }
}
