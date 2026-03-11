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

struct ApplicationInfo {
    name: String,
    validation_layer_enabled: bool,
    max_frames: Option<usize>, // Some(n) = exit after n frames, None = run indefinitely
}

/// Main application struct containing all engine state.
pub struct Application {
    pub(crate) window: Window,
    pub(crate) renderer: VulkanRenderer,
    /// Frame graph for rendering (built once at startup)
    pub(crate) frame_graph: katla_gfx::FrameGraph,
    pub(crate) camera: Rc<RefCell<Camera>>,
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
    /// HDR texture bindless index for tonemapping
    pub(crate) hdr_texture_index: Option<u32>,
    /// Flag to prevent double cleanup
    cleaned_up: bool,
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

                    // Update LDR bindless index for UI viewport rendering
                    for (name, slot) in recreated_textures {
                        if name == "ldr_color" {
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

                // Poll background loader for completed asset loads
                self.poll_background_loader();

                // Generate UI draw list BEFORE frame graph execution
                debug!("Generating UI draw list...");
                let ui_draw_list = editor::generate_ui_draw_list(self, dt);
                debug!("UI draw list generated");

                // Render frame to GPU (includes UI if present)
                debug!("Rendering frame...");
                self.render_frame(ui_draw_list);
                debug!("Frame rendered");

                // Process editor actions after UI rendering
                editor::process_editor_actions(self);

                // Handle max_frames limit
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

        self.renderer.wait_for_device();

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

        // Initialize transient textures and register HDR texture with bindless system
        self.frame_graph
            .initialize_transient_textures(&self.renderer)
            .expect("Failed to initialize transient textures");

        // Register HDR texture with bindless system for tonemapping
        let hdr_texture_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "hdr_color")
            .expect("Failed to register HDR texture with bindless system");

        // Set HDR texture index on tonemap pass
        self.frame_graph
            .set_tonemap_texture_index("tonemap", hdr_texture_index)
            .expect("Failed to set tonemap texture index");

        self.hdr_texture_index = Some(hdr_texture_index);
        info!(
            "HDR texture registered with bindless system at index {}",
            hdr_texture_index
        );

        // Register LDR (tonemapped) texture with bindless system for viewport rendering
        let ldr_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "ldr_color")
            .expect("Failed to register LDR texture with bindless system");

        // Store the LDR bindless index for UI viewport rendering
        // The UI will use this index to sample from the transient texture directly
        self.editor_ui
            .set_viewport_bindless_index(ldr_bindless_index);
        info!(
            "LDR (tonemapped) texture registered with bindless system at index {}",
            ldr_bindless_index
        );

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

        let entity_id = self.world.spawn((
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
        ));

        entity_id
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

        // Set texture indices on material
        self.renderer
            .set_material_texture_indices(material_handle, texture_indices);

        // 6. Spawn entity
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

    /// Upload textures from a GLTF model and return bindless texture indices.
    ///
    /// Returns [albedo, normal, metallic_roughness, ao] indices.
    fn upload_gltf_textures(&mut self, model: &crate::util::GLTFModel) -> [u32; 4] {
        let default_index = 0u32; // Default white texture
        let mut albedo_index = default_index;
        let mut normal_index = default_index;
        let mut mr_index = default_index;
        let mut ao_index = default_index;

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
        }

        [albedo_index, normal_index, mr_index, ao_index]
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

        info!(
            "Default scene setup complete - {} entities spawned",
            self.world.entity_ids().count()
        );
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
