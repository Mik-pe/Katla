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
pub mod renderer;

use std::{cell::RefCell, collections::HashMap, ffi::CString, path::PathBuf, rc::Rc, time::Instant};

use log::{debug, info, warn};
use winit::keyboard::ModifiersState;

pub use builder::*;
use katla_ecs::{input::Action, EntityId, World};
use katla_math::{Transform, Vec2, Vec3};
use katla_vulkan::{material::MaterialPipeline, MaterialRegistry, SkeletonBuffer, VulkanRenderer};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    animation::{AnimationManager, Skeleton},
    components::{DirectionalLight, DrawableComponent, PointLight, TransformComponent},
    entities::{Camera, Model},
    gui_state::GuiState,
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    preferences::Preferences,
    rendering::{
        create_checkerboard_material, create_checkerboard_texture, MaterialManager, MeshBuilder,
    },
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
    pub(crate) window: Option<Window>,
    pub(crate) renderer: Option<VulkanRenderer>,
    pub(crate) camera: Rc<RefCell<Camera>>,
    pub(crate) gltf_cache: FileCache<GLTFModel>,
    pub(crate) material_manager: MaterialManager,
    pub(crate) stage_upload: bool,
    pub(crate) timer: Timer,
    pub(crate) info: ApplicationInfo,
    pub(crate) world: World,
    pub(crate) input_mapper: InputMapper,
    pub(crate) current_modifiers: ModifiersState,
    pub(crate) frame_count: usize,
    pub(crate) resources: ResourceManager,
    /// Skeleton buffers for animated meshes, indexed by entity ID
    pub(crate) skeleton_buffers: HashMap<EntityId, Rc<RefCell<SkeletonBuffer>>>,
    /// Immediate mode UI context
    pub(crate) ui_context: katla_ui::UiContext,
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
    /// Gizmo rendering resources (mesh and material handles)
    pub(crate) gizmo_resources: Option<renderer::GizmoResources>,
    /// Grid pipeline for runtime toggle
    pub(crate) grid_pipeline: Option<Rc<RefCell<MaterialPipeline>>>,
    /// Background asset loader thread
    pub(crate) background_loader: BackgroundLoader,
    /// Next texture ID for thumbnails (custom IDs start at 100)
    pub(crate) next_thumbnail_texture_id: u64,
    /// Mapping of thumbnail paths to their uploaded texture IDs
    pub(crate) thumbnail_texture_ids: HashMap<PathBuf, katla_ui::TextureId>,
    /// Pending model spawns (path, position) waiting for background load
    pub(crate) pending_model_spawns: Vec<(PathBuf, katla_math::Vec3)>,
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(&self.info.name)
                        .with_resizable(true)
                        .with_min_inner_size(LogicalSize {
                            width: 800.0,
                            height: 600.0,
                        })
                        .with_maximized(true),
                )
                .unwrap();

            // Enable IME for text input (required for receiving text input events)
            window.set_ime_allowed(true);

            // Get initial DPI scale factor
            self.scale_factor = window.scale_factor() as f32;

            let engine_name = CString::new("Katla Engine").unwrap();
            let mut renderer = VulkanRenderer::init(
                &event_loop,
                &window,
                self.info.validation_layer_enabled,
                CString::new(self.info.name.as_str()).unwrap(),
                engine_name,
            );

            renderer
                .init_storage_standard()
                .expect("Failed to initialize storage uniform system");

            let loaded_count = renderer
                .material_registry
                .borrow_mut()
                .load_directory_storage(&self.resources.materials, renderer.context.clone())
                .expect("Failed to load materials directory");
            info!(
                "Loaded {} material templates from {}",
                loaded_count,
                self.resources.materials.display()
            );

            renderer
                .material_registry
                .borrow_mut()
                .enable_hot_reload(&self.resources.root, 100)
                .expect("Failed to enable hot reload");
            info!("Hot reload enabled for materials and shaders");

            // Load Fox model with skeletal animation
            let fox_path = self.resources.model_path("Fox.glb");
            let fox_transform = Transform::new_from_position(Vec3::new(0.0, 5.0, 0.0))
                .with_scale(Vec3::new(0.05, 0.05, 0.05));
            let context = renderer.context.clone();
            let fox_model = self.gltf_cache.read(fox_path);

            // Get material registry reference before mutable borrow of renderer
            // This is safe: we only read the RefCell's address, not its contents
            let material_registry: *const std::cell::RefCell<MaterialRegistry> =
                &renderer.material_registry;

            let fox = Model::from_gltf(
                &mut self.world,
                fox_model.clone(),
                context,
                Some(&mut renderer),
                fox_transform,
                // SAFETY: We're passing a valid reference to the material_registry.
                // The renderer's mutable borrow is only used for registration,
                // not for accessing material_registry during material creation.
                unsafe { &*material_registry },
            );

            info!(
                "Fox model entity: {:?} loaded, setting up animation...",
                fox.entity
            );

            AnimationManager::setup_animated_model(
                &mut self.world,
                fox.entity,
                &fox_model,
                Some("Run"),
            );

            debug!("Fox animation setup complete for entity {:?}", fox.entity);

            // Setup GPU skeleton buffer
            if let Some(skeleton) = self.world.get_component::<Skeleton>(fox.entity) {
                let joint_count = skeleton.joint_transforms.len();
                debug!("Fox has {} joints, creating skeleton buffer", joint_count);

                let skeleton_buffer = Rc::new(RefCell::new(SkeletonBuffer::new(
                    renderer.context.clone(),
                    joint_count,
                )));

                if let Some(drawable) = self.world.get_component::<DrawableComponent>(fox.entity) {
                    if let Some(material_handle) = drawable.material_handle {
                        if let Some(skeleton_layout) = renderer
                            .asset_registry
                            .get_skeleton_set_layout(material_handle)
                        {
                            if let Some(skeleton_handle) =
                                renderer.register_skeleton(skeleton_buffer.clone(), skeleton_layout)
                            {
                                debug!("Registered skeleton with handle {:?}", skeleton_handle);

                                if let Some(drawable) = self
                                    .world
                                    .get_component_mut::<DrawableComponent>(fox.entity)
                                {
                                    drawable.skeleton_handle = Some(skeleton_handle);
                                }

                                self.skeleton_buffers.insert(fox.entity, skeleton_buffer);
                            } else {
                                warn!("Failed to register skeleton with renderer");
                            }
                        } else {
                            warn!("Material does not have skeleton_set_layout");
                        }
                    }
                }
            } else {
                warn!("Fox entity has no Skeleton component");
            }

            // Create scene meshes
            let _cube = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(15.0, 5.0, -15.0))
                .color([1.0, 0.3, 0.3])
                .with_shared_material("Checkerboard")
                .build(&mut self.world, &mut renderer);

            let _sphere = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(30.0, 5.0, 0.0))
                .color([0.3, 1.0, 0.3])
                .with_shared_material("Checkerboard")
                .sphere()
                .build(&mut self.world, &mut renderer);

            let _cylinder = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(-30.0, 5.0, 0.0))
                .color([0.3, 0.3, 1.0])
                .with_shared_material("Checkerboard")
                .cylinder()
                .build(&mut self.world, &mut renderer);

            let _plane = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, -5.0, 0.0))
                .color([0.8, 0.8, 0.8])
                .with_shared_material("Checkerboard")
                .plane()
                .size(Vec3::new(10.0, 10.0, 1.0))
                .build(&mut self.world, &mut renderer);

            let _torus = MeshBuilder::new(renderer.context.clone())
                .position(Vec3::new(0.0, 15.0, 0.0))
                .color([1.0, 0.8, 0.3])
                .with_shared_material("Checkerboard")
                .torus()
                .build(&mut self.world, &mut renderer);

            // Load Avocado model for PBR testing
            // Avocado.glb is a small model (~0.1 units), scale up 20x for visibility
            let avocado_path = self.resources.model_path("Avocado.glb");
            let avocado_transform = Transform::new_from_position(Vec3::new(-15.0, 5.0, 15.0))
                .with_scale(Vec3::new(20.0, 20.0, 20.0));
            let avocado_model = self.gltf_cache.read(avocado_path);
            let _avocado = Model::from_gltf(
                &mut self.world,
                avocado_model.clone(),
                renderer.context.clone(),
                Some(&mut renderer),
                avocado_transform,
                // SAFETY: Same as above - registry is only used for reading templates
                unsafe { &*material_registry },
            );
            if let Some(name_comp) = self.world.get_component_mut::<NameComponent>(_avocado.entity) {
                name_comp.name = "Avocado (PBR Test)".to_string();
            }
            info!(
                "Avocado model loaded for PBR testing (entity {})",
                _avocado.entity.id()
            );

            // Load DamagedHelmet for PBR testing - classic PBR showcase model
            // Position it next to the avocado for comparison
            // The unified importer will automatically detect full PBR textures
            let helmet_path = self.resources.model_path("DamagedHelmet.glb");
            let helmet_transform = Transform::new_from_position(Vec3::new(-10.0, 5.0, 15.0))
                .with_scale(Vec3::new(3.0, 3.0, 3.0));
            let helmet_model = self.gltf_cache.read(helmet_path);
            let _helmet = Model::from_gltf(
                &mut self.world,
                helmet_model.clone(),
                renderer.context.clone(),
                Some(&mut renderer),
                helmet_transform,
                // SAFETY: Same as above - registry is only used for reading templates
                unsafe { &*material_registry },
            );
            if let Some(name_comp) = self.world.get_component_mut::<NameComponent>(_helmet.entity) {
                name_comp.name = "DamagedHelmet (Full PBR)".to_string();
            }
            info!(
                "DamagedHelmet model loaded with full PBR textures (entity {})",
                _helmet.entity.id()
            );

            // Setup parent-child relationships
            use crate::components::{Children, NameComponent, Parent};

            if let Some(torus_name) = self.world.get_component_mut::<NameComponent>(_torus) {
                torus_name.name = "Torus (Fox child)".to_string();
            }
            self.world.add_component(_torus, Parent::new(fox.entity));

            let existing_children = self
                .world
                .get_component::<Children>(fox.entity)
                .map(|c| c.children.clone())
                .unwrap_or_default();
            let mut children = existing_children;
            children.push(_torus);
            self.world
                .add_component(fox.entity, Children::new(children));

            // Add lighting
            let _sun = self.world.spawn((
                DirectionalLight::new(Vec3::new(-0.3, -1.0, -0.2), [1.0, 0.95, 0.8], 1.0),
                NameComponent::new("Sun Light"),
            ));

            let _red_light = self.world.spawn((
                TransformComponent {
                    transform: Transform::new_from_position(Vec3::new(10.0, 10.0, 10.0)),
                },
                PointLight::new([1.0, 0.3, 0.3], 5.0, 20.0),
                NameComponent::new("Red Point Light"),
            ));

            let _blue_light = self.world.spawn((
                TransformComponent {
                    transform: Transform::new_from_position(Vec3::new(-10.0, 8.0, 10.0)),
                },
                PointLight::new([0.3, 0.5, 1.0], 4.0, 25.0),
                NameComponent::new("Blue Point Light"),
            ));

            self.world.add_component(_blue_light, Parent::new(_cube));
            let existing_children = self
                .world
                .get_component::<Children>(_cube)
                .map(|c| c.children.clone())
                .unwrap_or_default();
            let mut cube_children = existing_children;
            cube_children.push(_blue_light);
            self.world
                .add_component(_cube, Children::new(cube_children));

            self.world
                .insert_resource(crate::components::AmbientLight::gray(0.15));

            // Create particle emitter
            let _particle_emitter = crate::entities::create_particle_emitter(
                &mut self.world,
                renderer.context.clone(),
                Vec3::new(0.0, 10.0, 0.0),
                100.0,
            );
            debug!("Created particle emitter entity");

            self.window = Some(window);

            // Setup checkerboard material
            let checkerboard_texture = create_checkerboard_texture(renderer.context.clone());
            if self
                .material_manager
                .register_from_template(
                    "Checkerboard",
                    &renderer.material_registry.borrow(),
                    Some(Rc::new(checkerboard_texture)),
                    None,
                )
                .is_some()
            {
                debug!("Registered checkerboard material from template");
            } else {
                warn!("Checkerboard template not found, using fallback");
                let checkerboard = create_checkerboard_material(renderer.context.clone());
                self.material_manager
                    .register_material("checkerboard", checkerboard);
            }

            self.material_manager.set_context(renderer.context.clone());

            self.renderer = Some(renderer);

            // Setup render graph
            renderer::setup_render_graph(self);
        }
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
                self.ui_context.input.set_mouse_button(btn, pressed);
            }
        }

        if let Some(_renderer) = &mut self.renderer {
            match event {
                WindowEvent::Resized(logical_size) => {
                    let new_width = logical_size.width;
                    let new_height = logical_size.height as f32;

                    if new_width > 0 && new_height > 0.0 {
                        if let Some(ref mut renderer) = self.renderer {
                            info!(
                                "=== Window resized to {}x{}, recreating swapchain ===",
                                new_width, new_height as u32
                            );
                            // Wait for GPU to finish before destroying old resources
                            renderer.wait_for_device();
                            renderer.recreate_swapchain();
                            let _ =
                                renderer.init_viewport_target(new_width as u32, new_height as u32);
                            let _ =
                                renderer.init_output_target(new_width as u32, new_height as u32);
                            renderer.setup_render_graph();

                            if let Some(viewport_extent) = renderer.viewport_extent() {
                                let aspect =
                                    viewport_extent.width as f32 / viewport_extent.height as f32;
                                self.camera
                                    .borrow_mut()
                                    .aspect_ratio_changed(&mut self.world, aspect);
                            }
                            info!("=== Resize complete ===");
                        }
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
                                ElementState::Released => {
                                    self.ui_context.input.add_key_release(key)
                                }
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

                        if event.state == ElementState::Pressed {
                            // Only process game-specific keys when viewport is focused
                            if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                                match keycode {
                                    KeyCode::Escape => event_loop.exit(),
                                    KeyCode::KeyT => self.stage_upload = true,
                                    _ => {}
                                }
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
                    debug!("RedrawRequested received");
                    self.timer.add_timestamp();
                    let dt = self.timer.get_delta() as f32;

                    // Update world (runs animation systems)
                    debug!("Updating world...");
                    self.world.update(dt);
                    debug!("World updated");

                    // Upload skeleton transforms to GPU buffers
                    debug!("Uploading skeleton transforms...");
                    renderer::upload_skeleton_transforms(self);
                    debug!("Skeleton transforms uploaded");

                    // Poll background loader for completed asset loads
                    self.poll_background_loader();

                    // Render using render graph
                    debug!("Rendering frame...");
                    renderer::render_frame(self);
                    debug!("Frame rendered");

                    // Render debug UI overlay
                    debug!("Rendering UI...");
                    editor::render_debug_ui(self, dt);
                    debug!("UI rendered");

                    // Handle max_frames limit
                    if let Some(max) = self.info.max_frames {
                        self.frame_count += 1;
                        if self.frame_count >= max {
                            info!("Rendered {} frames, exiting", self.frame_count);
                            event_loop.exit();
                        }
                    }

                    // Stage upload test (KeyT)
                    if self.stage_upload {
                        let start = Instant::now();
                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");

                        let _sphere = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(0.0, 5.0, 0.0))
                            .color([0.8, 0.2, 0.2])
                            .with_shared_material("Checkerboard")
                            .sphere()
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _cube = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(20.0, 5.0, 0.0))
                            .color([0.2, 0.8, 0.2])
                            .with_shared_material("Checkerboard")
                            .build(&mut self.world, renderer);

                        let renderer = self.renderer.as_mut().expect("Renderer not initialized");
                        let _plane = MeshBuilder::new(renderer.context.clone())
                            .position(Vec3::new(0.0, -5.0, 0.0))
                            .color([0.5, 0.5, 0.5])
                            .with_shared_material("Checkerboard")
                            .plane()
                            .size(Vec3::new(100.0, 100.0, 1.0))
                            .build(&mut self.world, renderer);

                        let millisecs = start.elapsed().as_micros() as f64 / 1000.0;
                        debug!("Mesh creation took {millisecs} ms");
                        self.stage_upload = false;
                    }

                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
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

        if let Some(ref mut renderer) = self.renderer {
            renderer.wait_for_device();
        }

        self.material_manager.destroy();

        if let Some(mut renderer) = self.renderer.take() {
            renderer.destroy();
        }
    }
}

impl Application {
    pub fn init(&mut self) {
        // Logger is now initialized in main() before building the application
    }

    /// Poll the background loader and process completed loads.
    fn poll_background_loader(&mut self) {
        use crate::ui::ThumbnailState;
        use crate::util::LoadResult;
        use katla_ui::TextureId;

        let results = self.background_loader.poll();

        for result in results {
            match result {
                LoadResult::ImageThumbnailLoaded { path, width, height, pixels, .. } => {
                    debug!("Thumbnail loaded: {:?} ({}x{})", path, width, height);

                    // Create texture ID for this thumbnail
                    let texture_id = TextureId::custom(self.next_thumbnail_texture_id);
                    self.next_thumbnail_texture_id += 1;

                    // Register with Vulkan renderer
                    if let Some(ref mut renderer) = self.renderer {
                        if let Err(e) = renderer.register_ui_thumbnail(texture_id.0, width, height, &pixels) {
                            warn!("Failed to register thumbnail texture: {:?}", e);
                            continue;
                        }
                    }

                    // Update the thumbnail cache entry
                    if let Some(entry) = self.background_loader.get_thumbnail_mut(&path) {
                        entry.uploaded = true;
                    }

                    // Store texture ID for this path (persists across directory navigations)
                    self.thumbnail_texture_ids.insert(path.clone(), texture_id);

                    // Update asset browser entries with this thumbnail
                    for asset in self.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Loaded { texture_id };
                            debug!("Updated thumbnail state for {:?}", path);
                            break;
                        }
                    }
                }
                LoadResult::ImageLoaded { path, width, height, pixels: _, .. } => {
                    debug!("Full image loaded: {:?} ({}x{})", path, width, height);
                    // Future: Handle full image loads (e.g., for textures, skyboxes)
                }
                LoadResult::ModelLoaded { path, vertices, indices, .. } => {
                    debug!("Model loaded: {:?} ({} vertices, {} indices)", path, vertices.len(), indices.len());
                    // Future: Handle model loads (e.g., for drag-drop spawning)
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
