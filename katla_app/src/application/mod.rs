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
pub mod editor;
mod renderer;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Instant};

use log::{debug, info, warn};
use winit::keyboard::ModifiersState;

pub use builder::*;
use katla_ecs::{input::Action, World};
use katla_gfx::{renderer::VulkanRenderer, TextureHandle};
use katla_math::Vec2;

use crate::components::TransformComponent;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    entities::Camera,
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
    /// Pending readback data from previous frame (for async black frame checking)
    pending_readback: Option<(usize, Vec<u8>)>,
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
                self.world.get_input_mut().mouse_delta = Vec2::new(
                    current_delta.x() + delta.0 as f32,
                    current_delta.y() + delta.1 as f32,
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
                        self.editor_ui.show_particle_inspector =
                            !self.editor_ui.show_particle_inspector;
                        info!(
                            "Particle inspector: {}",
                            if self.editor_ui.show_particle_inspector {
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
                debug!("RedrawRequested received");
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
                                            p.position[0], p.position[1], p.position[2],
                                            p.velocity[0], p.velocity[1], p.velocity[2],
                                            p.lifetime,
                                            p.scale,
                                            p.color[0], p.color[1], p.color[2], p.color[3]
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
                                            p.position[0], p.position[1], p.position[2],
                                            p.velocity[0], p.velocity[1], p.velocity[2],
                                            p.lifetime,
                                            p.scale,
                                            p.color[0], p.color[1], p.color[2], p.color[3]
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

                // Note: Transient textures are now single-buffered
                // Bindless indices are set once during initialization

                // Generate UI draw list BEFORE frame graph execution
                debug!("Generating UI draw list...");
                let ui_draw_list = editor::generate_ui_draw_list(self, dt);
                debug!("UI draw list generated");

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

    /// Spawn a test cube entity with the default material.
    ///
    /// Creates a cube mesh and spawns an entity with DrawableComponent and TransformComponent.
    /// Returns the entity ID of the spawned cube.
    pub fn spawn_test_cube(&mut self, position: [f32; 3], size: [f32; 3]) -> katla_ecs::EntityId {
        self.spawn_test_cube_with_color(position, size, katla_math::Color::WHITE)
    }

    /// Spawn a test cube entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_test_cube_with_color(
        &mut self,
        position: [f32; 3],
        size: [f32; 3],
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_cube_mesh(size);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        let entity_id = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
        ));

        info!("Spawned test cube at {:?} with size {:?}", position, size);
        entity_id
    }

    /// Spawn a sphere entity with the default material.
    pub fn spawn_sphere(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_sphere_with_color(position, radius, segments, rings, katla_math::Color::WHITE)
    }

    /// Spawn a sphere entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_sphere_with_color(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_sphere_mesh(radius, segments, rings);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        let entity_id = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
        ));

        info!("Spawned sphere at {:?} with radius {}", position, radius);
        entity_id
    }

    /// Spawn a sphere entity with PBR material properties.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_sphere_with_material(
        &mut self,
        position: [f32; 3],
        radius: f32,
        segments: u32,
        rings: u32,
        color: katla_math::Color,
        metallic: f32,
        roughness: f32,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_sphere_mesh(radius, segments, rings);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_material(
                mesh_handle,
                material_handle,
                Some(linear_color),
                metallic,
                roughness,
                1.0, // ao
            ),
        ))
    }

    /// Spawn a grid of spheres showcasing PBR material properties.
    ///
    /// Creates a grid where:
    /// - X-axis: Roughness (0.0 → 1.0)
    /// - Y-axis: Metallic (0.0 → 1.0)
    ///
    /// This is the standard PBR material showcase pattern used by most engines.
    pub fn spawn_pbr_material_grid(
        &mut self,
        center: [f32; 3],
        grid_size: usize,
        sphere_radius: f32,
        spacing: f32,
    ) {
        use katla_math::Color;

        let half_grid = (grid_size - 1) as f32 / 2.0;

        for y in 0..grid_size {
            for x in 0..grid_size {
                let metallic = y as f32 / (grid_size - 1).max(1) as f32;
                let roughness = x as f32 / (grid_size - 1).max(1) as f32;

                let pos_x = center[0] + (x as f32 - half_grid) * spacing;
                let pos_y = center[1] + (y as f32 - half_grid) * spacing;
                let pos_z = center[2];

                // Cool blue color - shifts to cyan for metals
                // spawn_sphere_with_material expects sRGB and converts to linear internally
                let base_color = Color::rgb(0.4 + metallic * 0.2, 0.6 + metallic * 0.2, 1.0);

                self.spawn_sphere_with_material(
                    [pos_x, pos_y, pos_z],
                    sphere_radius,
                    32,
                    16,
                    base_color,
                    metallic,
                    roughness,
                );
            }
        }

        info!(
            "Spawned PBR material grid ({}x{}) at {:?}",
            grid_size, grid_size, center
        );
    }

    /// Spawn a cylinder entity with the default material.
    pub fn spawn_cylinder(
        &mut self,
        position: [f32; 3],
        height: f32,
        radius: f32,
        segments: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_cylinder_with_color(position, height, radius, segments, katla_math::Color::WHITE)
    }

    /// Spawn a cylinder entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_cylinder_with_color(
        &mut self,
        position: [f32; 3],
        height: f32,
        radius: f32,
        segments: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_cylinder_mesh(height, radius, segments);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        let entity_id = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
        ));

        info!("Spawned cylinder at {:?}", position);
        entity_id
    }

    /// Spawn a plane entity with the default material.
    pub fn spawn_plane(
        &mut self,
        position: [f32; 3],
        width: f32,
        height: f32,
    ) -> katla_ecs::EntityId {
        self.spawn_plane_with_color(position, width, height, katla_math::Color::WHITE)
    }

    /// Spawn a plane entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_plane_with_color(
        &mut self,
        position: [f32; 3],
        width: f32,
        height: f32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle = self.renderer.create_plane_mesh(width, height);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        let entity_id = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
        ));

        info!("Spawned plane at {:?}", position);
        entity_id
    }

    /// Spawn a torus entity with the default material.
    pub fn spawn_torus(
        &mut self,
        position: [f32; 3],
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
    ) -> katla_ecs::EntityId {
        self.spawn_torus_with_color(
            position,
            radius,
            tube_radius,
            segments,
            tube_segments,
            katla_math::Color::WHITE,
        )
    }

    /// Spawn a torus entity with a specific color.
    /// Color is expected in sRGB (perceptual) space and converted to linear for PBR.
    pub fn spawn_torus_with_color(
        &mut self,
        position: [f32; 3],
        radius: f32,
        tube_radius: f32,
        segments: u32,
        tube_segments: u32,
        color: katla_math::Color,
    ) -> katla_ecs::EntityId {
        use crate::components::{DrawableComponent, TransformComponent};

        use katla_math::Vec3;

        let mesh_handle =
            self.renderer
                .create_torus_mesh(radius, tube_radius, segments, tube_segments);
        let material_handle = self.default_material();

        // Convert sRGB to linear for correct PBR rendering
        let linear_color = color.to_linear();

        let entity_id = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, linear_color),
        ));

        info!("Spawned torus at {:?}", position);
        entity_id
    }

    /// Spawn a GLTF model from file. Handles both static and skinned meshes.
    ///
    /// # Arguments
    /// * `path` - Path to the GLTF file
    /// * `position` - World position to spawn at
    /// * `default_animation` - Optional animation name to play automatically
    ///
    /// # Returns
    /// The entity ID of the spawned model, or None if loading failed
    pub fn spawn_gltf_model(
        &mut self,
        path: impl AsRef<std::path::Path>,
        position: [f32; 3],
        default_animation: Option<&str>,
    ) -> Option<katla_ecs::EntityId> {
        use crate::components::{DrawableComponent, TransformComponent};
        use katla_math::Vec3;

        // 1. Load model from cache
        let path_buf = path.as_ref().to_path_buf();
        let model = self.gltf_cache.read(path_buf);

        // 2. Convert indices to u32 (generate sequential indices for non-indexed geometry)
        let vertex_count = if model.has_skinning {
            model.skinned_vertex_data.len()
        } else {
            model.vertex_data.len()
        };
        let indices = Self::convert_indices_to_u32_with_vertex_count(
            &model.index_data,
            model.index_stride,
            vertex_count,
        );

        debug!(
            "Model '{}' index conversion: {} bytes input (stride {}), {} indices output, {} vertices",
            path.as_ref().display(),
            model.index_data.len(),
            model.index_stride,
            indices.len(),
            vertex_count
        );

        // 3. Create mesh (skinned or regular)
        let mesh_handle = if model.has_skinning {
            self.renderer
                .create_mesh(&model.skinned_vertex_data, &indices)
        } else {
            self.renderer.create_mesh(&model.vertex_data, &indices)
        };

        // 4. Create material (skinned or regular)
        let shader_path = if model.has_skinning {
            self.resources.shader_path("model_pbr_skinned.wgsl")
        } else {
            self.resources.shader_path("model_pbr.wgsl")
        };

        let material_handle = if model.has_skinning {
            self.renderer
                .material_builder(&shader_path)
                .with_vertex_type(katla_gfx::VertexType::Skinned)
                .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat)
                .build()
                .ok()?
        } else {
            self.renderer
                .compile_material(
                    &shader_path,
                    katla_gfx::MaterialOptions {
                        vertex_type: katla_gfx::VertexType::Pbr,
                        color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                        ..Default::default()
                    },
                )
                .ok()?
        };

        // 5. Upload textures and set texture indices
        let texture_indices = self.upload_gltf_textures(&model);

        // Set texture indices on material (only first 4: albedo, normal, mr, ao)
        self.renderer.set_material_texture_indices(
            material_handle,
            [
                texture_indices[0],
                texture_indices[1],
                texture_indices[2],
                texture_indices[3],
            ],
        );

        // 6. Spawn entity with emission texture index
        let entity = self.world.spawn((
            TransformComponent {
                transform: katla_math::Transform::from_position(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )),
            },
            DrawableComponent::with_handles(mesh_handle, material_handle),
        ));

        // Set emission texture index on drawable component
        if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(entity) {
            drawable.emission = texture_indices[4] as f32;

            // Log if we have an emission texture
            if texture_indices[4] > 0 {
                info!(
                    "Model '{}' has emission texture at bindless index {}",
                    path.as_ref().display(),
                    texture_indices[4]
                );
            }
        }

        // 7. If skinned, set up animation
        if model.has_skinning {
            // Get joint count from skin
            let joint_count = model
                .document
                .skins()
                .next()
                .map(|s| s.joints().count())
                .unwrap_or(0);

            if joint_count > 0 {
                // Create GPU skeleton
                let skeleton_handle = self.renderer.create_skeleton(joint_count).ok()?;

                // Add skeleton handle to drawable
                if let Some(drawable) = self.world.get_component_mut::<DrawableComponent>(entity) {
                    drawable.skeleton_handle = skeleton_handle;
                }

                // Set up CPU animation components
                crate::animation::AnimationManager::setup_animated_model(
                    &mut self.world,
                    entity,
                    &model,
                    default_animation,
                );

                info!(
                    "Spawned animated model '{}' with {} joints",
                    path.as_ref().display(),
                    joint_count
                );
            }
        } else {
            info!("Spawned static model '{}'", path.as_ref().display());
        }

        Some(entity)
    }

    /// Spawn a simple particle emitter at the given position.
    ///
    /// Creates a particle emitter component with default fire-like settings.
    /// The emitter will be processed by the particle simulation system.
    ///
    /// Upload textures from a GLTF model and return bindless texture indices.
    ///
    /// Returns [albedo, normal, metallic_roughness, ao, emission] indices.
    fn upload_gltf_textures(&mut self, model: &crate::util::GLTFModel) -> [u32; 5] {
        let default_index = 0u32; // Default white texture
        let mut albedo_index = default_index;
        let mut normal_index = default_index;
        let mut mr_index = default_index;
        let mut ao_index = default_index;
        let mut emission_index = default_index;

        // Get first material if available
        let material_info = model.materials.first();

        if let Some(mat) = material_info {
            // Upload albedo texture
            if let Some(tex_idx) = mat.base_color_texture {
                if let Some(image) = model.images.get(tex_idx) {
                    let handle = self.upload_gltf_image(image, true);
                    albedo_index = self.get_bindless_index(handle);
                    debug!(
                        "Uploaded albedo texture {} -> bindless {}",
                        tex_idx, albedo_index
                    );
                }
            }

            // Upload normal texture
            if let Some(tex_idx) = mat.normal_texture {
                if let Some(image) = model.images.get(tex_idx) {
                    let handle = self.upload_gltf_image(image, false);
                    normal_index = self.get_bindless_index(handle);
                    debug!(
                        "Uploaded normal texture {} -> bindless {}",
                        tex_idx, normal_index
                    );
                }
            }

            // Upload metallic/roughness texture
            if let Some(tex_idx) = mat.metallic_roughness_texture {
                if let Some(image) = model.images.get(tex_idx) {
                    let handle = self.upload_gltf_image(image, false);
                    mr_index = self.get_bindless_index(handle);
                    debug!("Uploaded MR texture {} -> bindless {}", tex_idx, mr_index);
                }
            }

            // Upload AO texture
            if let Some(tex_idx) = mat.occlusion_texture {
                if let Some(image) = model.images.get(tex_idx) {
                    let handle = self.upload_gltf_image(image, false);
                    ao_index = self.get_bindless_index(handle);
                    debug!("Uploaded AO texture {} -> bindless {}", tex_idx, ao_index);
                }
            }

            // Upload emissive texture
            if let Some(tex_idx) = mat.emission_texture {
                if let Some(image) = model.images.get(tex_idx) {
                    let handle = self.upload_gltf_image(image, false);
                    emission_index = self.get_bindless_index(handle);
                    debug!(
                        "Uploaded emissive texture {} -> bindless {}",
                        tex_idx, emission_index
                    );
                }
            }
        }

        [
            albedo_index,
            normal_index,
            mr_index,
            ao_index,
            emission_index,
        ]
    }

    /// Upload a single GLTF image to the GPU.
    fn upload_gltf_image(&mut self, image: &gltf::image::Data, srgb: bool) -> TextureHandle {
        // Convert RGB to RGBA if needed (Vulkan requires 4-channel alignment)
        let pixels = if image.format == gltf::image::Format::R8G8B8 {
            let mut rgba = Vec::with_capacity(image.pixels.len() / 3 * 4);
            for chunk in image.pixels.chunks(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            rgba
        } else {
            image.pixels.clone()
        };

        if srgb {
            let desc = katla_gfx::TextureDescriptor::rgba8_srgb(image.width, image.height);
            self.renderer.create_texture(&desc, &pixels)
        } else {
            let desc = katla_gfx::TextureDescriptor::rgba8_unorm(image.width, image.height);
            self.renderer.create_texture(&desc, &pixels)
        }
    }

    /// Get the bindless texture index for a texture handle.
    fn get_bindless_index(&self, handle: katla_gfx::TextureHandle) -> u32 {
        // The texture manager assigns bindless indices during texture creation
        // We need to query the texture manager for the bindless slot
        self.renderer.get_texture_bindless_index(handle)
    }

    /// Convert index data from bytes to u32 based on stride.
    ///
    /// For non-indexed geometry (empty index_data), generates sequential indices
    /// [0, 1, 2, ... vertex_count-1] for the given vertex count.
    fn convert_indices_to_u32_with_vertex_count(
        index_data: &[u8],
        index_stride: u8,
        vertex_count: usize,
    ) -> Vec<u32> {
        if index_data.is_empty() || index_stride == 0 {
            // Generate sequential indices for non-indexed geometry
            return (0..vertex_count as u32).collect();
        }

        match index_stride {
            1 => index_data.iter().map(|&b| b as u32).collect(),
            2 => index_data
                .chunks(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]) as u32)
                .collect(),
            4 => index_data
                .chunks(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Set up a default test scene with various primitives.
    ///
    /// Creates a visually interesting scene with multiple objects for testing.
    pub fn setup_default_scene(&mut self) {
        use katla_math::Color;

        info!("Setting up default scene...");

        // Ground plane - nice dark gray
        self.spawn_plane_with_color([0.0, -1.0, 0.0], 20.0, 20.0, Color::from_u8(40, 44, 52));

        // PBR material grid - metallic (Y) x roughness (X)
        self.spawn_pbr_material_grid([0.0, 2.0, -6.0], 5, 0.4, 1.2);

        // Center cube - vibrant coral/orange
        self.spawn_test_cube_with_color(
            [-5.0, 0.0, -5.0],
            [1.0, 1.0, 1.0],
            Color::from_u8(255, 120, 80),
        );

        // Sphere to the left - bright cyan
        self.spawn_sphere_with_color([-7.0, 0.0, -5.0], 0.7, 32, 16, Color::from_u8(80, 220, 255));

        // Cylinder to the right - magenta/pink
        self.spawn_cylinder_with_color(
            [5.0, 0.0, -5.0],
            1.5,
            0.5,
            32,
            Color::from_u8(255, 80, 200),
        );

        // Torus in front - lime green
        self.spawn_torus_with_color(
            [7.0, 0.5, -3.0],
            0.8,
            0.2,
            32,
            16,
            Color::from_u8(150, 255, 100),
        );

        // Distant plane as backdrop - deep purple/blue
        self.spawn_plane_with_color([0.0, 2.0, -10.0], 15.0, 8.0, Color::from_u8(60, 40, 100));

        // Add animated Fox - scale down and position
        if let Some(fox) =
            self.spawn_gltf_model("resources/models/Fox.glb", [3.0, 0.0, 0.0], Some("Run"))
        {
            // Scale down the fox (it's huge by default)
            if let Some(transform) = self.world.get_component_mut::<TransformComponent>(fox) {
                transform.transform.scale = katla_math::Vec3::new(0.01, 0.01, 0.01);
            }
            info!("Spawned animated Fox with Run animation");
        }

        // Add DamagedHelmet - position for viewing
        if let Some(helmet) =
            self.spawn_gltf_model("resources/models/DamagedHelmet.glb", [0.0, 1.5, -5.0], None)
        {
            // Scale the helmet appropriately
            if let Some(transform) = self.world.get_component_mut::<TransformComponent>(helmet) {
                transform.transform.scale = katla_math::Vec3::new(1.0, 1.0, 1.0);
            }
            info!("Spawned DamagedHelmet model");
        }

        // Add particle emitters
        self.setup_particle_emitters();

        info!(
            "Default scene setup complete - {} entities spawned with particle effects",
            self.world.entity_ids().count()
        );
    }

    /// Set up particle emitters for the default scene.
    fn setup_particle_emitters(&mut self) {
        use crate::components::ParticleEmitterComponent;

        info!("Setting up particle emitters via ECS...");

        // Fire emitter near the center cube
        let fire_emitter = ParticleEmitterComponent::fire_effect([-3.0, 1.0, -3.0]);
        self.world.spawn((fire_emitter,));

        // Magic sparkles emitter
        let sparkle_emitter = ParticleEmitterComponent::sparkle_effect([0.0, 3.0, 0.0]);
        self.world.spawn((sparkle_emitter,));

        info!("Particle emitters setup complete - emitters will be initialized by ParticleSystem");
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
                LoadResult::ImageLoaded {
                    path,
                    width,
                    height,
                    pixels: _,
                    ..
                } => {
                    debug!("Full image loaded: {:?} ({}x{})", path, width, height);
                    // Future: Handle full image loads (e.g., for textures, skyboxes)
                }
                LoadResult::ModelLoaded {
                    path,
                    vertices: _,
                    indices: _,
                    ..
                } => {
                    debug!("Model loaded: {:?}", path);
                }
                LoadResult::ShaderSourceLoaded { path, source, .. } => {
                    debug!("Shader source loaded: {:?} ({} bytes)", path, source.len());
                    // Future: Handle shader loads (e.g., for hot reload)
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

        info!("Saved frame {} to {:?} ({}x{} pixels, converted from BGRA_sRGB to RGBA, alpha forced to 255)", 
              frame, filename, width, height);
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
