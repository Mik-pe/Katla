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
pub(crate) mod camera;
pub mod editor;
mod renderer;
mod spawning;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Instant};

use log::{debug, error, info, warn};
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
    util::{BackgroundLoader, GltfCache, Timer},
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
    pub(crate) gltf_cache: GltfCache,
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
    /// Game engine editor UI
    pub(crate) editor_ui: crate::ui::EditorUI,
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
    /// Whether the application should exit (set by editor actions, checked in window_event)
    pub(crate) quit_requested: bool,
    /// Flag to prevent double cleanup
    cleaned_up: bool,
    /// Particle system for managing particle emitters via ECS
    pub(crate) particle_system: crate::systems::ParticleSystem,
    /// GPU animation system for pose evaluation (ECS queries only, GPU resources on renderer)
    pub(crate) gpu_animation_system:
        Option<crate::systems::gpu_animation_system::GpuAnimationSystem>,
    /// Flag to trigger particle debug readback at frame 10
    #[cfg(debug_assertions)]
    pub(crate) particle_readback_pending: bool,
    /// Flag to ensure particle debug readback only happens once
    #[cfg(debug_assertions)]
    pub(crate) particle_readback_done: bool,
    /// Maps instance_index -> EntityId for resolving GPU picking results.
    /// Populated each frame during collect_draws_with_context.
    pub(crate) entity_instance_map: std::collections::HashMap<u32, katla_ecs::EntityId>,
    /// Reverse map: EntityId -> Vec<instance_index> for outline selection.
    /// Populated each frame alongside entity_instance_map.
    entity_to_instance_indices: std::collections::HashMap<katla_ecs::EntityId, Vec<u32>>,
    /// Pending picking operation: (frame_number, mouse_x_physical, mouse_y_physical).
    /// Set on left-click in viewport, processed after the next render.
    pub(crate) pending_pick: Option<(usize, f32, f32)>,
    /// Bindless texture index for the stencil indicator R8 texture.
    /// Passed to the tonemap shader each frame via emission_idx field.
    pub(crate) stencil_indicator_bindless_index: Option<u32>,
    /// Whether the window is currently minimized (zero extent).
    pub(crate) minimized: bool,
    /// Tracks GPU resource reference counts for automatic cleanup on entity/component destruction.
    pub(crate) gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker,
    /// Gizmo state (mode, drag, hover).
    pub(crate) gizmo_state: crate::gizmo::GizmoState,
    /// Gizmo GPU resources (meshes, material).
    pub(crate) gizmo_resources: crate::gizmo::GizmoResources,
    /// Previous frame's mouse screen position (for gizmo rotation drag delta).
    pub(crate) prev_mouse_screen: Option<(f32, f32)>,
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
            // Don't track mouse motion for orbit camera when gizmo is active
            if self.gizmo_state.is_dragging() {
                return;
            }
            let input = self.world.get_input();
            let should_track = input.is_action_pressed(Action::LookEnable)
                || input.is_action_pressed(Action::PanEnable);
            if should_track {
                let current_delta = input.mouse_delta;
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
        if self.quit_requested {
            event_loop.exit();
            return;
        }

        if let WindowEvent::MouseInput { state, button, .. } = &event {
            let mouse_combo = MouseCombo::with_modifiers(*button, self.current_modifiers);
            let binding = InputBinding::Mouse(mouse_combo);

            if let ElementState::Pressed = state {
                let mouse_pos = self.ui_context.input.mouse_pos;
                self.editor_ui.update_focused_panel_from_click(mouse_pos);

                // Trigger GPU picking on left-click in viewport
                if *button == winit::event::MouseButton::Left
                    && self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                    && self.editor_ui.last_viewport_bounds.contains(mouse_pos)
                {
                    // Check if the click hits a gizmo axis first
                    self.gizmo_state.consumed_click = false;

                    if let Some(axis) = self.hit_test_gizmo(mouse_pos) {
                        self.begin_gizmo_drag(axis, mouse_pos);
                    } else {
                        // Store viewport-relative logical coordinates for the picking readback.
                        let vp = self.editor_ui.last_viewport_bounds;
                        let rel_x = mouse_pos.x() - vp.min.x();
                        let rel_y = mouse_pos.y() - vp.min.y();
                        self.pending_pick = Some((self.frame_count, rel_x, rel_y));
                    }
                }
            }

            if let Some(action) = self.input_mapper.get_action(&binding) {
                // Only send mouse input to game when viewport is focused and gizmo is not active
                if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                    && !self.gizmo_state.is_dragging()
                    && !self.gizmo_state.consumed_click
                {
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

            // End gizmo drag on mouse release
            if matches!(state, ElementState::Released)
                && *button == winit::event::MouseButton::Left
                && self.gizmo_state.is_dragging()
            {
                self.gizmo_state.end_drag();
            }
        }

        match event {
            WindowEvent::Resized(logical_size) => {
                let new_width = logical_size.width;
                let new_height = logical_size.height as f32;

                if new_width > 0 && new_height > 0.0 {
                    if self.minimized {
                        self.minimized = false;
                        info!("Window restored from minimized");
                    }

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
                } else if !self.minimized {
                    self.minimized = true;
                    info!("Window minimized (zero extent), skipping rendering");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Convert physical pixels to logical pixels for UI
                let logical_x = position.x as f32 / self.scale_factor;
                let logical_y = position.y as f32 / self.scale_factor;
                let mouse_pos = Vec2::new(logical_x, logical_y);
                self.ui_context.input.set_mouse_pos(mouse_pos);

                // Gizmo hover and drag updates
                self.update_gizmo_interaction(mouse_pos);
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

                // Forward scroll to ECS input state for orbit camera zoom,
                // but only when the mouse is hovering over the viewport.
                let mouse_pos = self.ui_context.input.mouse_pos;
                if self.editor_ui.last_viewport_bounds.contains(mouse_pos) {
                    let wheel_y = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    self.world.get_input_mut().mouse_wheel_delta += wheel_y;
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Occluded(occluded) => {
                if occluded && !self.minimized {
                    self.minimized = true;
                    debug!("Window occluded, skipping rendering");
                } else if !occluded && self.minimized {
                    self.minimized = false;
                    info!("Window unoccluded, resuming rendering");
                    let _ = self.renderer.recreate_swapchain(&mut self.frame_graph);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    let key_combo = KeyCombo::with_modifiers(keycode, self.current_modifiers);
                    let binding = InputBinding::Keyboard(key_combo);

                    // Focus camera on selected entity with 'F'
                    if event.state == ElementState::Pressed
                        && keycode == KeyCode::KeyF
                        && self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                        && !self.current_modifiers.control_key()
                        && !self.current_modifiers.shift_key()
                        && !self.current_modifiers.alt_key()
                        && let Some(entity_id) = self.editor_ui.selected_entity
                    {
                        self.focus_camera_on_entity(entity_id);
                    }

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

                    // Save scene with Ctrl+S (suppressed when TextInput or modal is focused)
                    if event.state == ElementState::Pressed
                        && keycode == KeyCode::KeyS
                        && self.current_modifiers.control_key()
                        && !self.current_modifiers.shift_key()
                        && !self.current_modifiers.alt_key()
                        && !self.editor_ui.prev_want_capture_keyboard
                    {
                        self.editor_ui
                            .pending_actions
                            .push(crate::ui::EditorAction::SaveScene);
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

                    if event.state == ElementState::Pressed
                        && self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                        && !self.ui_context.input.want_capture_keyboard
                    {
                        // Gizmo mode shortcuts
                        if keycode == KeyCode::KeyW {
                            self.gizmo_state
                                .set_mode(crate::gizmo::GizmoMode::Translate);
                        } else if keycode == KeyCode::KeyE {
                            self.gizmo_state.set_mode(crate::gizmo::GizmoMode::Rotate);
                        } else if keycode == KeyCode::KeyR {
                            self.gizmo_state.set_mode(crate::gizmo::GizmoMode::Scale);
                        }

                        if keycode == KeyCode::Escape {
                            event_loop.exit()
                        }
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
                if self.minimized {
                    self.window.request_redraw();
                    return;
                }

                debug!("RedrawRequested (frame {})", self.frame_count);
                self.timer.add_timestamp();
                let dt = self.timer.get_delta() as f32;

                // Update world (runs animation systems)
                debug!("Updating world...");
                self.world.update(dt);
                debug!("World updated");

                // Process ECS events to clean up GPU resources for destroyed entities
                crate::gpu_cleanup::process_gpu_cleanup_events(
                    &self.world,
                    &mut self.gpu_resource_tracker,
                    &mut self.renderer,
                );

                // Update particle emitters from ECS components
                self.particle_system.update(
                    &mut self.world,
                    &mut self.renderer.particle_system,
                    dt,
                );

                // Update GPU animation: prepare data and upload per-frame params
                if let (Some(gpu_anim), Some(pipeline), Some(buffers)) = (
                    &mut self.gpu_animation_system,
                    &mut self.renderer.animation_pipeline,
                    &mut self.renderer.animation_buffers,
                ) {
                    gpu_anim.prepare(&mut self.world, pipeline, buffers);
                    gpu_anim.update_params(&mut self.world, buffers);
                    self.frame_graph
                        .set_animation_skeleton_count(gpu_anim.skeleton_count() as u32);

                    // Build per-entity skeleton copy commands:
                    // (skeleton_handle_index, joint_offset, joint_count)
                    use crate::components::DrawableComponent;
                    let mut copy_cmds = Vec::new();
                    for entity in gpu_anim.entities() {
                        if let Some(drawable) =
                            self.world.get_component::<DrawableComponent>(entity)
                            && let Some(info) = gpu_anim.entity_info(entity)
                        {
                            copy_cmds.push((
                                drawable.skeleton_handle.index(),
                                info.joint_offset,
                                info.joint_count,
                            ));
                        }
                    }
                    self.frame_graph.set_skeleton_copy_commands(copy_cmds);
                }

                // Poll background loader for completed asset loads
                self.poll_background_loader();

                // DEBUG: Test particle readback at frame 10
                #[cfg(debug_assertions)]
                {
                    if self.frame_count == 10
                        && let Some(ref particle_system) = self.renderer.particle_system
                    {
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
                                for (i, idx) in debug_data.alive_list.iter().take(10).enumerate() {
                                    log::info!("Alive[{}] = {}", i, idx);
                                }
                            }
                            Err(e) => {
                                log::warn!("Failed to read particle debug data: {}", e);
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

                // Save keyboard capture state for next frame's Ctrl+S suppression.
                // Must happen after generate_ui_draw_list (which sets the flag) and
                // before process_editor_actions (which calls clear_frame_state).
                self.editor_ui.prev_want_capture_keyboard =
                    self.ui_context.input.want_capture_keyboard;

                // Upload font atlas AFTER draw list generation (which rasterizes new glyphs)
                // and BEFORE render_frame (which samples from the GPU atlas).
                // Doing it after render_frame would cause a one-frame lag where text
                // samples from stale GPU data.
                editor::upload_font_atlas(self);

                // Render frame to GPU (includes UI if present)
                debug!("Rendering frame...");
                self.render_frame(ui_draw_list, dt, self.frame_count);
                debug!("Frame rendered");

                // GPU picking: queue readback if a pick was triggered this frame,
                // or check the result from a previous frame's readback.
                self.process_picking();

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
        info!("Application::init() called");

        // Register scene resources
        self.world
            .insert_resource(crate::resources::AmbientLight::default());

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

        // Set the protected material in the GPU resource tracker so it's never destroyed
        self.gpu_resource_tracker
            .set_protected_material(self.default_material_handle);

        // Initialize gizmo GPU resources
        self.init_gizmo_resources();

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
            use katla_gfx::render_graph::RenderGraphError;

            // Insert particle compute passes at the beginning of the frame graph.
            // Vulkan requires compute dispatches to run outside render passes, and the
            // particle render pass (inline after geometry) reads their output, so these
            // must execute before any graphics passes.
            self.frame_graph.insert_pass(
                0,
                PassDesc::new("particle_simulate", PassType::Compute, vec![], vec![])
                    .with_pipeline(simulate_pipeline)
                    .with_compute_fn(|frame, cmd, _pipeline_handle| {
                        let workgroup_count = frame.particle_simulate_workgroup_count();
                        let emit_ran = frame.particle_emit_ran;
                        let debug_readback = frame.particle_debug_readback;

                        {
                            let renderer = frame.renderer_mut();
                            let current_frame = renderer.current_frame();
                            let particle_system = match renderer.particle_system.as_mut() {
                                Some(ps) => ps,
                                None => return Ok(()),
                            };

                            if workgroup_count == 0 {
                                log::debug!("Skipping particle simulate - workgroup_count is 0");
                                return Ok(());
                            }

                            particle_system
                                .update_compute_descriptor_binding(current_frame)
                                .map_err(|e| {
                                    RenderGraphError::VulkanError(format!(
                                        "Failed to update particle compute descriptor binding: {}",
                                        e
                                    ))
                                })?;

                            particle_system.reset_simulate_counters(
                                cmd.vk_command_buffer(),
                                emit_ran,
                                current_frame,
                            );

                            particle_system
                                .record_simulate_dispatch(
                                    cmd.vk_command_buffer(),
                                    &renderer.asset_registry,
                                    workgroup_count,
                                    current_frame,
                                )
                                .map_err(|e| {
                                    RenderGraphError::VulkanError(format!(
                                        "Particle simulate dispatch failed: {}",
                                        e
                                    ))
                                })?;

                            if let Err(e) = particle_system.record_draw_command_dispatch(
                                cmd.vk_command_buffer(),
                                &renderer.asset_registry,
                                current_frame,
                            ) {
                                log::warn!("Failed to record draw command dispatch: {}", e);
                            }

                            if debug_readback {
                                log::info!("Recording particle debug readback after simulate pass");
                                particle_system
                                    .record_debug_readback(cmd.vk_command_buffer(), current_frame)
                                    .map_err(|e| {
                                        RenderGraphError::VulkanError(format!(
                                            "Particle debug readback failed: {}",
                                            e
                                        ))
                                    })?;
                            }
                        }

                        if debug_readback {
                            frame.particle_debug_readback = false;
                        }

                        Ok(())
                    }),
            );

            self.frame_graph.insert_pass(
                0,
                PassDesc::new("particle_emit", PassType::Compute, vec![], vec![])
                    .with_pipeline(emit_pipeline)
                    .with_compute_fn(|frame, cmd, _pipeline_handle| {
                        let workgroup_count = frame.particle_emit_workgroup_count();

                        {
                            let renderer = frame.renderer_mut();
                            let current_frame = renderer.current_frame();
                            let particle_system = match renderer.particle_system.as_mut() {
                                Some(ps) => ps,
                                None => return Ok(()),
                            };

                            if workgroup_count == 0 {
                                log::debug!("Skipping particle emit - workgroup_count is 0");
                                return Ok(());
                            }

                            particle_system
                                .update_compute_descriptor_binding(current_frame)
                                .map_err(|e| {
                                    RenderGraphError::VulkanError(format!(
                                        "Failed to update particle compute descriptor binding: {}",
                                        e
                                    ))
                                })?;

                            particle_system
                                .record_emit_dispatch(
                                    cmd.vk_command_buffer(),
                                    &renderer.asset_registry,
                                    workgroup_count,
                                    current_frame,
                                )
                                .map_err(|e| {
                                    RenderGraphError::VulkanError(format!(
                                        "Particle emit dispatch failed: {}",
                                        e
                                    ))
                                })?;
                        }

                        frame.particle_emit_ran = true;
                        Ok(())
                    }),
            );

            info!("Added particle compute passes to frame graph");
        }

        // Initialize animation pose evaluation pipeline
        let anim_shader_path = self
            .resources
            .shader_path("compute/animation/pose_eval.wgsl");
        if let Err(e) = self.renderer.init_animation_pipeline(&anim_shader_path) {
            warn!("Failed to initialize animation pipeline: {}", e);
        } else {
            info!("Animation pose evaluation pipeline initialized");
        }

        // Create GPU animation system (ECS queries only, GPU resources on renderer)
        self.gpu_animation_system =
            Some(crate::systems::gpu_animation_system::GpuAnimationSystem::new());

        // Add animation compute pass to frame graph.
        // Inserted at position 0 so it runs before other compute passes (light culling,
        // particle emit/simulate) and all graphics passes. This ensures skeleton matrices
        // are ready for the subsequent copy commands and vertex shader skinning.
        if let Some(pipeline_handle) = self.renderer.animation_pipeline_handle() {
            use katla_gfx::render_graph::{PassDesc, PassType, RenderGraphError};
            self.frame_graph.insert_pass(
                0,
                PassDesc::new("animation_pose_eval", PassType::Compute, vec![], vec![])
                    .with_pipeline(pipeline_handle)
                    .with_compute_fn(|frame, cmd, _pipeline_handle| {
                        let skeleton_count = frame.animation_skeleton_count();
                        if skeleton_count == 0 {
                            return Ok(());
                        }

                        let copy_cmds = frame.skeleton_copy_commands().to_vec();
                        let renderer = frame.renderer_mut();

                        let pipeline = match renderer.animation_pipeline.as_ref() {
                            Some(p) => p,
                            None => return Ok(()),
                        };
                        let buffers = match renderer.animation_buffers.as_ref() {
                            Some(b) => b,
                            None => return Ok(()),
                        };

                        pipeline.record_dispatch(
                            cmd.vk_command_buffer(),
                            &renderer.asset_registry,
                            skeleton_count,
                        );

                        // Barrier: compute write → copy read
                        pipeline.add_output_barrier(
                            cmd.vk_command_buffer(),
                            buffers,
                            ash::vk::PipelineStageFlags2::COPY,
                            ash::vk::AccessFlags2::TRANSFER_READ,
                        );

                        // Copy per-entity joint matrices from output buffer to SkeletonBuffers
                        let output_buf = buffers.output_buffer();
                        for &(handle_idx, joint_offset, joint_count) in &copy_cmds {
                            let handle = katla_gfx::SkeletonHandle::new(handle_idx);
                            renderer.copy_skeleton_from_compute_output(
                                cmd.vk_command_buffer(),
                                handle,
                                output_buf,
                                joint_offset,
                                joint_count,
                            );
                        }

                        // Global barrier: transfer write → vertex shader read
                        // Covers all skeleton buffers written by the copies above.
                        if !copy_cmds.is_empty() {
                            let vk_cmd = cmd.vk_command_buffer();
                            let barrier = ash::vk::MemoryBarrier2::default()
                                .src_stage_mask(ash::vk::PipelineStageFlags2::COPY)
                                .dst_stage_mask(ash::vk::PipelineStageFlags2::VERTEX_SHADER)
                                .src_access_mask(ash::vk::AccessFlags2::TRANSFER_WRITE)
                                .dst_access_mask(ash::vk::AccessFlags2::SHADER_READ);
                            let barriers = [barrier];
                            let dep_info =
                                ash::vk::DependencyInfo::default().memory_barriers(&barriers);
                            unsafe {
                                renderer
                                    .context()
                                    .device
                                    .cmd_pipeline_barrier2(vk_cmd, &dep_info);
                            }
                        }

                        Ok::<(), RenderGraphError>(())
                    }),
            );
            info!("Added animation pose evaluation compute pass to frame graph");
        }

        // Add light culling compute pass to frame graph.
        // This must run before geometry passes so culling results are available for PBR shading.
        // Inserted at position 0 — after animation_pose_eval, before particle passes.
        if self.renderer.has_light_culling() {
            use katla_gfx::render_graph::{PassDesc, PassType, RenderGraphError};
            self.frame_graph.insert_pass(
                0,
                PassDesc::new("light_culling", PassType::Compute, vec![], vec![]).with_compute_fn(
                    |frame, cmd, _pipeline_handle| {
                        let renderer = frame.renderer_mut();
                        let view = renderer.frame_uniforms().view_matrix;
                        let proj = renderer.frame_uniforms().proj_matrix;
                        renderer.dispatch_light_culling(cmd.vk_command_buffer(), &view, &proj);
                        Ok::<(), RenderGraphError>(())
                    },
                ),
            );
            info!("Added light culling compute pass to frame graph");
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

        // Register stencil indicator texture with bindless for tonemap shader
        let stencil_indicator_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "stencil_indicator")
            .expect("Failed to register stencil indicator texture with bindless system");

        info!(
            "Stencil indicator texture registered with bindless at index {}",
            stencil_indicator_index
        );

        // Store stencil indicator bindless index for passing to overlay each frame
        self.stencil_indicator_bindless_index = Some(stencil_indicator_index);

        // Set overlay texture indices so the wallhack overlay shader can read LDR + indicator
        self.frame_graph
            .set_overlay_texture_indices(
                "wallhack_overlay",
                viewport_bindless_index,
                stencil_indicator_index,
            )
            .expect("Failed to set wallhack overlay texture indices");

        // Load default scene from disk
        let scene_path = std::path::Path::new(crate::scene::DEFAULT_SCENE_PATH);
        match crate::scene::SceneManager::load_from_file(self, scene_path) {
            Ok(()) => info!(
                "Loaded default scene from {}",
                crate::scene::DEFAULT_SCENE_PATH
            ),
            Err(e) => error!(
                "Failed to load default scene from {}: {}",
                crate::scene::DEFAULT_SCENE_PATH,
                e
            ),
        }

        info!("Application::init() completed");
    }

    fn focus_camera_on_entity(&mut self, entity_id: katla_ecs::EntityId) {
        use crate::components::{Children, OrbitCameraControllerComponent, WorldTransform};

        // Collect world positions of the entity and all its children
        let mut positions = Vec::new();
        let mut queue = vec![entity_id];
        let mut visited = std::collections::HashSet::new();
        visited.insert(entity_id);

        while let Some(eid) = queue.pop() {
            if let Some(wt) = self.world.get_component::<WorldTransform>(eid) {
                positions.push(wt.transform.position);
            }
            if let Some(children) = self.world.get_component::<Children>(eid) {
                for &child in &children.children {
                    if visited.insert(child) {
                        queue.push(child);
                    }
                }
            }
        }

        if positions.is_empty() {
            return;
        }

        // Compute bounding sphere center
        let center = positions
            .iter()
            .fold(katla_math::Vec3::new(0.0, 0.0, 0.0), |acc, p| acc + *p)
            / positions.len() as f32;

        // Compute radius as the max distance from center
        let radius = positions
            .iter()
            .map(|p| (*p - center).length())
            .fold(0.0_f32, f32::max)
            .max(0.5);

        // Distance to fit the object so it covers ~50% of the smaller viewport dimension.
        let camera_entity = self.camera.borrow().entity;
        let fov_rad = self
            .world
            .get_component::<crate::components::PerspectiveComponent>(camera_entity)
            .map(|p| p.fov.to_radians())
            .unwrap_or_else(|| 60.0_f32.to_radians());
        let aspect = self
            .world
            .get_component::<crate::components::PerspectiveComponent>(camera_entity)
            .map(|p| p.aspect_ratio)
            .unwrap_or(16.0 / 9.0);
        let target_fraction = 0.5;
        // Vertical FOV covers half_height = distance * tan(fov/2)
        // Horizontal FOV covers half_width = half_height * aspect
        // We want the object to fill target_fraction of the smaller visible extent
        let half_height = 1.0 / fov_rad.tan(); // at distance=1
        let half_width = half_height * aspect;
        let smaller_half = half_height.min(half_width);
        let distance = radius / (target_fraction * smaller_half);

        if let Some(orbit) = self
            .world
            .get_component_mut::<OrbitCameraControllerComponent>(camera_entity)
        {
            orbit.focus = Some(crate::components::camera::orbit_camera::FocusTarget {
                target: center,
                distance: distance.clamp(orbit.min_distance, orbit.max_distance),
                duration: 0.35,
                elapsed: 0.0,
                start_target: orbit.target,
                start_distance: orbit.distance,
                start_yaw: orbit.yaw,
                start_pitch: orbit.pitch,
                target_yaw: orbit.yaw,
                target_pitch: orbit.pitch,
            });
        }
    }

    /// Process GPU picking: queue a readback for a pending pick, or resolve a completed readback.
    ///
    /// Flow:
    /// 1. On left-click in viewport: `pending_pick` is set with viewport-relative logical coords
    /// 2. After render_frame: If `pending_pick` is set for this frame, queue the GPU readback
    ///    converting viewport-relative logical coords to full-render-target physical pixel coords
    /// 3. On subsequent frames: Check if the readback completed, resolve instance_index -> EntityId
    fn process_picking(&mut self) {
        // Check for completed readback from a previous frame
        if let Ok(Some((_frame, instance_index))) = self.renderer.check_picking_readback() {
            if instance_index == 0 {
                // Background/empty space was clicked — clear selection
                if self.editor_ui.selected_entity.is_some() {
                    info!("Clicked empty space, clearing selection");
                    self.editor_ui.selected_entity = None;
                }
            } else {
                // The shader encodes instance_index + 1, so subtract 1 to get the storage buffer index
                let storage_index = instance_index - 1;

                if let Some(&entity_id) = self.entity_instance_map.get(&storage_index) {
                    info!(
                        "Picked entity {:?} (instance_index={}, storage_index={})",
                        entity_id, instance_index, storage_index
                    );
                    self.editor_ui.selected_entity = Some(entity_id);
                } else {
                    log::debug!(
                        "Picked instance_index={} but no entity mapping found (storage_index={})",
                        instance_index,
                        storage_index
                    );
                    self.editor_ui.selected_entity = None;
                }
            }
        }

        // Queue a new readback if a pick was triggered this frame
        if let Some((pick_frame, rel_x, rel_y)) = self.pending_pick.take() {
            if pick_frame != self.frame_count {
                // Stale pick from a previous frame — discard
                log::debug!("Discarding stale pending pick from frame {}", pick_frame);
                return;
            }

            // Convert viewport-panel-relative logical coordinates to physical pixel coordinates
            // in the full render target (swapchain resolution).
            //
            // The object_id texture covers the full swapchain, but the UI maps it into the
            // viewport panel (a sub-region of the window). So we need to map panel-local
            // coords to full-texture coords:
            //   physical_x = (rel_x / panel_logical_width) * swapchain_physical_width
            let vp = &self.editor_ui.last_viewport_bounds;
            let panel_width = vp.width().max(1.0);
            let panel_height = vp.height().max(1.0);
            let extent = self.renderer.swapchain_extent();
            let physical_x = ((rel_x / panel_width) * extent.width as f32) as u32;
            let physical_y = ((rel_y / panel_height) * extent.height as f32) as u32;

            if physical_x >= extent.width || physical_y >= extent.height {
                log::debug!(
                    "Picking coords ({}, {}) out of render target bounds ({}x{}), skipping",
                    physical_x,
                    physical_y,
                    extent.width,
                    extent.height
                );
                return;
            }

            // Get the object-ID texture image for the current frame
            let frame_idx = self.renderer.current_frame();
            if let Some(transient) = self.frame_graph.transient_texture("object_id", frame_idx) {
                let image = transient.image;
                match self.renderer.queue_picking_readback(
                    self.frame_count,
                    image,
                    physical_x,
                    physical_y,
                ) {
                    Ok(()) => {
                        log::debug!(
                            "Queued picking readback at physical ({}, {}) for frame {}",
                            physical_x,
                            physical_y,
                            self.frame_count
                        );
                    }
                    Err(e) => {
                        log::warn!("Failed to queue picking readback: {}", e);
                    }
                }
            } else {
                log::warn!("Object-ID transient texture not found for picking readback");
            }
        }
    }

    /// Get the default PBR material handle.
    pub fn default_material(&self) -> katla_gfx::MaterialHandle {
        self.default_material_handle
    }

    /// Initialize GPU resources for the 3D gizmo (meshes + material).
    fn init_gizmo_resources(&mut self) {
        use crate::gizmo::GizmoResources;

        let shaft_mesh = self.renderer.create_cylinder_mesh(1.0, 0.05, 16);
        let cone_mesh = self.renderer.create_cone_mesh(1.0, 0.5, 16);
        let cube_mesh = self.renderer.create_cube_mesh([1.0, 1.0, 1.0]);
        let ring_mesh = self.renderer.create_torus_mesh(0.5, 0.02, 48, 24);

        let unlit_shader_path = self.resources.shader_path("unlit.wgsl");
        let material = self
            .renderer
            .compile_material(
                &unlit_shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    depth_test: false,
                    ..Default::default()
                },
            )
            .expect("Failed to create gizmo unlit material");

        self.gpu_resource_tracker.set_protected_material(material);

        self.gizmo_resources = GizmoResources {
            shaft_mesh,
            cone_mesh,
            cube_mesh,
            ring_mesh,
            material,
            initialized: true,
        };

        info!("Gizmo GPU resources initialized");
    }

    /// Hit-test gizmo axes at the given screen position.
    ///
    /// Returns the hit axis, or None if no axis is close enough to the mouse.
    fn hit_test_gizmo(&self, mouse_pos: katla_math::Vec2) -> Option<crate::gizmo::GizmoAxis> {
        use crate::components::{PerspectiveComponent, TransformComponent};
        use crate::gizmo::*;

        if self.gizmo_state.entity.is_none() || !self.gizmo_resources.initialized {
            return None;
        }

        let vp = &self.editor_ui.last_viewport_bounds;
        let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

        if !vp.contains(mouse_pos) {
            return None;
        }

        let camera = self.camera.borrow();
        let view_mat = camera.get_view_mat(&self.world);
        let proj_mat = camera.get_proj_mat(&self.world);
        drop(camera);

        let fov = self
            .world
            .get_component::<PerspectiveComponent>(self.camera.borrow().entity)
            .map(|p| p.fov)
            .unwrap_or(60.0);

        let viewport_height = self.editor_ui.viewport_size().1 as f32;
        let cam_pos = self
            .world
            .get_component::<TransformComponent>(self.camera.borrow().entity)
            .map(|t| t.transform.position)
            .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));

        let gizmo_scale = compute_gizmo_scale(
            cam_pos,
            self.gizmo_state.origin,
            fov.to_radians(),
            viewport_height,
            120.0,
        );

        hit_test_axes(
            (mouse_pos.x(), mouse_pos.y()),
            self.gizmo_state.origin,
            gizmo_scale,
            &view_mat,
            &proj_mat,
            viewport,
            self.gizmo_state.mode,
            12.0, // pixel threshold
        )
    }

    /// Begin dragging a gizmo axis.
    fn begin_gizmo_drag(&mut self, axis: crate::gizmo::GizmoAxis, mouse_pos: katla_math::Vec2) {
        use crate::components::TransformComponent;

        if let Some(entity_id) = self.gizmo_state.entity {
            let entity_pos = self
                .world
                .get_component::<TransformComponent>(entity_id)
                .map(|t| t.transform.position)
                .unwrap_or(self.gizmo_state.origin);

            // Compute a world-space reference point on the drag plane
            let vp = &self.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());
            let camera = self.camera.borrow();
            let view_mat = camera.get_view_mat(&self.world);
            let proj_mat = camera.get_proj_mat(&self.world);
            drop(camera);

            let (ray_origin, ray_dir) = crate::gizmo::screen_to_ray(
                (mouse_pos.x(), mouse_pos.y()),
                viewport,
                &view_mat,
                &proj_mat,
            );
            {
                // Compute camera forward for the drag plane
                let _cam_pos = self
                    .world
                    .get_component::<TransformComponent>(self.camera.borrow().entity)
                    .map(|t| t.transform.position)
                    .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));
                let cam_rot = self.camera.borrow().get_view_rotation(&self.world);
                let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

                if let Some(delta) = crate::gizmo::compute_translate_delta(
                    axis,
                    ray_origin,
                    ray_dir,
                    entity_pos,
                    camera_forward,
                ) {
                    // Store the initial world position on the plane (not the entity position)
                    let world_pos = entity_pos + delta;
                    self.gizmo_state.begin_drag(axis, world_pos, entity_pos);
                } else {
                    self.gizmo_state.begin_drag(axis, entity_pos, entity_pos);
                }

                // Store initial rotation/scale for rotate/scale modes
                if let Some(transform) = self.world.get_component::<TransformComponent>(entity_id) {
                    let euler = transform.transform.rotation.to_euler();
                    self.gizmo_state.drag_start_rotation = Some(euler);
                    self.gizmo_state.drag_start_scale = Some(transform.transform.scale);
                    self.gizmo_state.drag_rotation_accum = katla_math::Vec3::new(0.0, 0.0, 0.0);
                }
            }
        }
    }

    /// Update gizmo interaction on mouse move: hover highlight and drag application.
    fn update_gizmo_interaction(&mut self, mouse_pos: katla_math::Vec2) {
        use crate::components::TransformComponent;

        // Store previous screen position for rotation delta
        let prev_screen = self.prev_mouse_screen;
        let current_screen = (mouse_pos.x(), mouse_pos.y());
        self.prev_mouse_screen = Some(current_screen);

        if self.gizmo_state.is_dragging() {
            // Apply the drag based on the current mode
            let Some(entity_id) = self.gizmo_state.entity else {
                return;
            };

            let Some(axis) = self.gizmo_state.active_axis else {
                return;
            };

            let vp = &self.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

            if !vp.contains(mouse_pos) {
                return;
            }

            let camera = self.camera.borrow();
            let view_mat = camera.get_view_mat(&self.world);
            let proj_mat = camera.get_proj_mat(&self.world);
            drop(camera);

            let cam_rot = self.camera.borrow().get_view_rotation(&self.world);
            let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

            let (ray_origin, ray_dir) =
                crate::gizmo::screen_to_ray(current_screen, viewport, &view_mat, &proj_mat);
            {
                if let Some(transform) = self
                    .world
                    .get_component_mut::<TransformComponent>(entity_id)
                {
                    match self.gizmo_state.mode {
                        crate::gizmo::GizmoMode::Translate => {
                            if let Some(start_origin) = self.gizmo_state.drag_start_origin
                                && let Some(delta) = crate::gizmo::compute_translate_delta(
                                    axis,
                                    ray_origin,
                                    ray_dir,
                                    start_origin,
                                    camera_forward,
                                )
                            {
                                transform.transform.position = start_origin + delta;
                                self.gizmo_state.origin = transform.transform.position;
                            }
                        }
                        crate::gizmo::GizmoMode::Rotate => {
                            if let Some(prev) = prev_screen {
                                // Project gizmo origin to screen space for rotation center
                                let origin_screen = crate::gizmo::world_to_screen(
                                    self.gizmo_state.origin,
                                    &view_mat,
                                    &proj_mat,
                                    viewport,
                                );

                                if let Some(center) = origin_screen {
                                    let delta = crate::gizmo::compute_rotate_delta(
                                        axis,
                                        center,
                                        current_screen,
                                        prev,
                                    );
                                    self.gizmo_state.drag_rotation_accum = katla_math::Vec3::new(
                                        self.gizmo_state.drag_rotation_accum.x()
                                            + if axis == crate::gizmo::GizmoAxis::X {
                                                delta
                                            } else {
                                                0.0
                                            },
                                        self.gizmo_state.drag_rotation_accum.y()
                                            + if axis == crate::gizmo::GizmoAxis::Y {
                                                delta
                                            } else {
                                                0.0
                                            },
                                        self.gizmo_state.drag_rotation_accum.z()
                                            + if axis == crate::gizmo::GizmoAxis::Z {
                                                delta
                                            } else {
                                                0.0
                                            },
                                    );

                                    if let Some((start_pitch, start_yaw, start_roll)) =
                                        self.gizmo_state.drag_start_rotation
                                    {
                                        let new_pitch =
                                            start_pitch + self.gizmo_state.drag_rotation_accum.x();
                                        let new_yaw =
                                            start_yaw + self.gizmo_state.drag_rotation_accum.y();
                                        let new_roll =
                                            start_roll + self.gizmo_state.drag_rotation_accum.z();
                                        transform.transform.rotation = katla_math::Quat::from_euler(
                                            new_pitch, new_yaw, new_roll,
                                        );
                                    }
                                }
                            }
                        }
                        crate::gizmo::GizmoMode::Scale => {
                            if let Some(start_origin) = self.gizmo_state.drag_start_origin
                                && let Some(axis_dist) = crate::gizmo::compute_scale_delta(
                                    axis,
                                    ray_origin,
                                    ray_dir,
                                    start_origin,
                                    camera_forward,
                                )
                                && let Some(start_scale) = self.gizmo_state.drag_start_scale
                            {
                                let axis_idx = match axis {
                                    crate::gizmo::GizmoAxis::X => 0,
                                    crate::gizmo::GizmoAxis::Y => 1,
                                    crate::gizmo::GizmoAxis::Z => 2,
                                };
                                // Store the initial axis distance on the first drag frame
                                // to compute relative scale from the drag start
                                if self.gizmo_state.drag_start_world.is_none() {
                                    self.gizmo_state.drag_start_world =
                                        Some(katla_math::Vec3::new(axis_dist, 0.0, 0.0));
                                }
                                let initial_dist = self.gizmo_state.drag_start_world.unwrap().x();
                                // Scale relative to drag start: ratio of current distance to initial distance
                                let scale_factor = if initial_dist.abs() > 1e-6 {
                                    axis_dist / initial_dist
                                } else {
                                    1.0 + axis_dist * 0.01
                                };
                                let mut scale = [start_scale.x(), start_scale.y(), start_scale.z()];
                                scale[axis_idx] = (scale[axis_idx] * scale_factor).max(0.01);
                                transform.transform.scale =
                                    katla_math::Vec3::new(scale[0], scale[1], scale[2]);
                            }
                        }
                    }
                }
            }
        } else if self.gizmo_state.entity.is_some() {
            // Update hover highlight
            self.gizmo_state.hovered_axis = self.hit_test_gizmo(mouse_pos);
        }
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
