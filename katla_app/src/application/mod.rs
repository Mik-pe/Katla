//! Application module - main application lifecycle and event handling.
//!
//! This module contains the main [`Application`] struct and its implementation
//! of [`ApplicationHandler`] for winit event handling. The heavy lifting is
//! delegated to submodules:
//!
//! - [`builder`] - Application builder pattern
//! - [`renderer`] - Render graph setup and frame rendering
//! - [`editor`] - UI rendering and entity management for the editor
//! - [`events`] - Input routing helpers
//! - [`frame_loop`] - Per-frame orchestration, cleanup, background loading
//! - [`init`] - Shader/pipeline setup and resource initialization
//! - [`picking`] - GPU picking readback
//! - [`gizmo`] - Gizmo interaction methods (hit-test, drag, hover)

pub mod builder;
pub(crate) mod camera;
pub(crate) mod clipboard;
#[cfg(feature = "editor")]
pub mod editor;
#[cfg(feature = "editor")]
mod editor_methods;
mod events;
pub mod frame_graph_config;
mod frame_loop;
mod game_state;
#[cfg(feature = "editor")]
mod gizmo;
mod headless;
mod init;
#[cfg(not(feature = "editor"))]
mod no_editor_methods;
mod particle_drive;
mod picking;
mod renderer;
mod resource_loading;
pub(crate) mod spawning;
pub(crate) mod ui_test;

#[cfg(feature = "editor")]
use std::collections::HashMap;
#[cfg(feature = "editor")]
use std::path::PathBuf;
use std::time::Instant;

use log::{debug, info};
use winit::keyboard::ModifiersState;

use crate::{FrameGraph, Renderer};
pub use builder::*;
use katla_ecs::World;
use katla_math::Vec2;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use self::camera::Camera;
use crate::util::GltfCache;
use crate::{
    geometry_cache::GeometryCache,
    input::{Action, InputBinding, InputMapper, KeyCombo, MouseCombo},
    preferences::Preferences,
    resources::ResourceManager,
    util::Timer,
};
#[cfg(feature = "editor")]
use crate::{gui_state::GuiState, util::BackgroundLoader};

pub struct ApplicationInfo {
    name: String,
    validation_mode: katla_gfx::ValidationMode,
    max_frames: Option<usize>, // Some(n) = exit after n frames, None = run indefinitely
    check_black_frames: bool,
    scene_path: Option<String>, // Override scene to load on startup
    dump_layout_path: Option<DumpLayoutTarget>,
    screenshot_path: Option<String>, // Headless screenshot output path
    headless: bool,                  // Running without a window
    pub(crate) ui_test_path: Option<String>, // UI test mode: output directory for screenshots
}

/// Where to write the layout dump.
#[derive(Clone)]
pub(crate) enum DumpLayoutTarget {
    Stdout,
    File(String),
}

/// Main application struct containing all engine state.
/// Editor-only state grouped behind a single cfg gate.
#[cfg(feature = "editor")]
pub(crate) struct EditorState {
    /// UI renderer for converting UI draw lists to GPU format
    pub(crate) ui_renderer: crate::ui::UIRenderer,
    /// Game engine editor UI
    pub(crate) editor_ui: crate::ui::EditorUI,
    /// GUI layout state (panel sizes, positions)
    pub(crate) gui_state: GuiState,
    /// Background asset loader thread
    pub(crate) background_loader: BackgroundLoader,
    /// Mapping of thumbnail paths to their uploaded texture handles
    pub(crate) thumbnail_texture_handles: HashMap<PathBuf, katla_gfx::TextureHandle>,
    /// Mapping of texture file paths to their GPU handles (for hot reload)
    pub(crate) texture_paths: HashMap<PathBuf, katla_gfx::TextureHandle>,
    /// Reusable buffer for collecting (instance_index, EntityId) pairs during
    /// collect_draws_with_context. Cleared and refilled each frame to avoid
    /// per-frame Vec allocation.
    pub(crate) draw_entity_map_entries: Vec<(u32, katla_ecs::EntityId)>,
    /// Maps instance_index -> EntityId for resolving GPU picking results.
    /// Populated each frame during collect_draws_with_context.
    pub(crate) entity_instance_map: std::collections::HashMap<u32, katla_ecs::EntityId>,
    /// Reverse map: EntityId -> Vec<instance_index> for outline selection.
    /// Populated each frame alongside entity_instance_map.
    pub(crate) entity_to_instance_indices: std::collections::HashMap<katla_ecs::EntityId, Vec<u32>>,
    /// Pending picking operation: (frame_number, mouse_x_physical, mouse_y_physical).
    /// Set on left-click in viewport, processed after the next render.
    pub(crate) pending_pick: Option<(usize, f32, f32)>,
    /// Bindless texture index for the stencil indicator R8 texture.
    /// Passed to the tonemap shader each frame via emission_idx field.
    pub(crate) stencil_indicator_bindless_index: Option<u32>,
    /// Gizmo state (mode, drag, hover).
    pub(crate) gizmo_state: crate::gizmo::GizmoState,
    /// Gizmo GPU resources (meshes, material).
    pub(crate) gizmo_resources: crate::gizmo::GizmoResources,
    /// Physics debug GPU resources (wireframe meshes, material).
    pub(crate) physics_debug_resources: crate::rendering::physics_debug::PhysicsDebugResources,
    /// Billboard GPU resources (mesh, material, icon textures).
    pub(crate) billboard_resources: crate::billboard::BillboardResources,
    /// Previous frame's mouse screen position (for gizmo rotation drag delta).
    pub(crate) prev_mouse_screen: Option<(f32, f32)>,
    /// Component registry for AI agent scene tools.
    pub(crate) component_registry: katla_ecs::scene_tool::ComponentRegistry,
    /// Agent harness for AI co-creator execution.
    pub(crate) _agent_harness: katla_ecs::agent::AgentHarness,
    /// LLM configuration (API key, model, endpoint).
    pub(crate) llm_config: katla_agent::LlmConfig,
    /// Async bridge for background LLM calls.
    pub(crate) async_bridge: Option<katla_agent::AsyncBridge>,
    /// Co-creator agent: single source of truth for conversation state and LLM interaction.
    pub(crate) co_creator_agent: katla_agent::CoCreatorAgent,
    /// MCP server bridge for external AI tool integration.
    #[cfg(feature = "mcp")]
    pub(crate) mcp_state: crate::application::editor::mcp::McpState,
    pub(crate) undo_stack: Vec<katla_ecs::scene_tool::UndoGroup>,
    pub(crate) redo_stack: Vec<katla_ecs::scene_tool::UndoGroup>,
    pub(crate) agent_undo_stack: Vec<katla_ecs::scene_tool::UndoGroup>,
    pub(crate) agent_redo_stack: Vec<katla_ecs::scene_tool::UndoGroup>,
    /// Whether an inspector slider was being dragged last frame.
    pub(crate) inspector_slider_was_active: bool,
    /// Pre-drag snapshot of ECS values for undo.
    pub(crate) inspector_drag_snapshot: Option<editor::InspectorDragSnapshot>,
    /// Maps entity ID to GPU handles for cleanup when entity is destroyed via undo/redo.
    pub(crate) entity_gpu_handles: HashMap<katla_ecs::EntityId, editor::GpuCleanupData>,
    /// Currently playing audio preview voice handle in asset browser.
    pub(crate) preview_voice: Option<katla_audio::VoiceHandle>,
}

#[cfg(feature = "editor")]
impl EditorState {
    /// Construct EditorState from its component parts.
    pub(crate) fn new(
        ui_renderer: crate::ui::UIRenderer,
        theme: crate::ui::ColorScheme,
        preferences: &Preferences,
        gui_state: crate::gui_state::GuiState,
    ) -> Self {
        use crate::util::BackgroundLoader;
        let component_registry =
            crate::application::editor::component_registry::build_editor_component_registry();
        let available = component_registry.type_names();
        let llm_config = katla_agent::LlmConfig::load();
        let async_bridge = katla_agent::AsyncBridge::with_rate_limits(
            std::time::Duration::from_millis(llm_config.rate_limit_min_interval_ms),
            llm_config.rate_limit_max_calls_per_minute,
        )
        .ok();
        let mut state = Self {
            ui_renderer,
            editor_ui: {
                let mut editor = crate::ui::EditorUI::with_theme(theme);
                editor.show_grid = preferences.show_grid;
                editor.show_stats = preferences.show_stats;
                editor.show_physics_debug = preferences.show_physics_debug;
                editor.show_reverb_debug = preferences.show_reverb_debug;
                editor.set_font_scale(preferences.font_scale);
                editor.left_panel_width = gui_state.left_panel_width;
                editor.right_panel_width = gui_state.right_panel_width;
                editor.asset_browser.panel_height = gui_state.asset_browser_height;
                editor
            },
            gui_state,
            background_loader: BackgroundLoader::new(),
            thumbnail_texture_handles: HashMap::new(),
            texture_paths: HashMap::new(),
            draw_entity_map_entries: Vec::new(),
            entity_instance_map: std::collections::HashMap::new(),
            entity_to_instance_indices: std::collections::HashMap::new(),
            pending_pick: None,
            stencil_indicator_bindless_index: None,
            gizmo_state: crate::gizmo::GizmoState::default(),
            gizmo_resources: crate::gizmo::GizmoResources::default(),
            physics_debug_resources:
                crate::rendering::physics_debug::PhysicsDebugResources::default(),
            billboard_resources: crate::billboard::BillboardResources::default(),
            prev_mouse_screen: None,
            component_registry,
            _agent_harness: katla_ecs::agent::AgentHarness::new(),
            llm_config,
            async_bridge,
            co_creator_agent: katla_agent::CoCreatorAgent::new(),
            #[cfg(feature = "mcp")]
            mcp_state: crate::application::editor::mcp::McpState::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            agent_undo_stack: Vec::new(),
            agent_redo_stack: Vec::new(),
            inspector_slider_was_active: false,
            inspector_drag_snapshot: None,
            entity_gpu_handles: HashMap::new(),
            preview_voice: None,
        };
        state.editor_ui.set_available_components(available);
        state
    }

    /// Clear all editor state that holds EntityId references.
    /// Must be called after any operation that invalidates entity IDs
    /// (e.g. scene restore after play mode, new scene).
    pub(crate) fn clear_entity_references(&mut self) {
        self.editor_ui.clear_entity_references();
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.agent_undo_stack.clear();
        self.agent_redo_stack.clear();
        self.entity_gpu_handles.clear();
        self.inspector_drag_snapshot = None;
    }

    pub(crate) fn push_undo(&mut self, group: katla_ecs::scene_tool::UndoGroup) {
        self.redo_stack.clear();
        self.undo_stack.push(group);
    }

    pub(crate) fn perform_undo(&mut self, world: &mut katla_ecs::World) -> bool {
        if let Some(mut group) = self.undo_stack.pop() {
            if group.undo_all(world).is_ok() {
                self.redo_stack.push(group);
                return true;
            }
            self.undo_stack.push(group);
        }
        false
    }

    pub(crate) fn perform_redo(&mut self, world: &mut katla_ecs::World) -> bool {
        if let Some(mut group) = self.redo_stack.pop() {
            if group.redo_all(world).is_ok() {
                self.undo_stack.push(group);
                return true;
            }
            self.redo_stack.push(group);
        }
        false
    }

    pub(crate) fn perform_agent_undo(&mut self, world: &mut katla_ecs::World) -> bool {
        if let Some(mut group) = self.agent_undo_stack.pop() {
            if group.undo_all(world).is_ok() {
                self.agent_redo_stack.push(group);
                return true;
            }
            self.agent_undo_stack.push(group);
        }
        false
    }
}

/// Cached optional pass IDs resolved from the application-selected bindings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PassIds {
    pub(crate) depth_prepass: Option<katla_gfx::render_graph::PassId>,
    pub(crate) picking: Option<katla_gfx::render_graph::PassId>,
    pub(crate) geometry: Option<katla_gfx::render_graph::PassId>,
    pub(crate) shadow: Option<katla_gfx::render_graph::PassId>,
    pub(crate) outline: Option<katla_gfx::render_graph::PassId>,
    pub(crate) stencil_indicator: Option<katla_gfx::render_graph::PassId>,
    pub(crate) ui: Option<katla_gfx::render_graph::PassId>,
    pub(crate) tonemap: Option<katla_gfx::render_graph::PassId>,
    pub(crate) wallhack_overlay: Option<katla_gfx::render_graph::PassId>,
}

impl PassIds {
    pub(crate) fn resolve(
        graph: &FrameGraph,
        bindings: &frame_graph_config::FrameGraphPassBindings,
    ) -> crate::AppResult<Self> {
        fn resolve_one(
            graph: &FrameGraph,
            role: &str,
            name: &Option<String>,
        ) -> crate::AppResult<Option<katla_gfx::render_graph::PassId>> {
            let Some(name) = name.as_deref() else {
                return Ok(None);
            };

            graph
                .pass_id(name)
                .map(Some)
                .ok_or_else(|| crate::AppError::Other {
                    message: format!(
                        "Frame-graph binding '{role}' references missing pass '{name}'"
                    ),
                })
        }

        Ok(Self {
            depth_prepass: resolve_one(graph, "depth_prepass", &bindings.depth_prepass)?,
            picking: resolve_one(graph, "picking", &bindings.picking)?,
            geometry: resolve_one(graph, "geometry", &bindings.geometry)?,
            shadow: resolve_one(graph, "shadow", &bindings.shadow)?,
            outline: resolve_one(graph, "outline", &bindings.outline)?,
            stencil_indicator: resolve_one(
                graph,
                "stencil_indicator",
                &bindings.stencil_indicator,
            )?,
            ui: resolve_one(graph, "ui", &bindings.ui)?,
            tonemap: resolve_one(graph, "tonemap", &bindings.tonemap)?,
            wallhack_overlay: resolve_one(graph, "wallhack_overlay", &bindings.wallhack_overlay)?,
        })
    }

    /// Re-resolve every capability after graph mutation. Missing bindings become
    /// `None`; stale pass IDs are never retained.
    pub(crate) fn refresh(
        &mut self,
        graph: &FrameGraph,
        bindings: &frame_graph_config::FrameGraphPassBindings,
    ) -> crate::AppResult<()> {
        *self = Self::resolve(graph, bindings)?;
        Ok(())
    }
}

/// Main application struct containing all engine state.
pub struct Application {
    /// Window handle. `None` in headless mode.
    pub(crate) window: Option<Window>,
    pub(crate) renderer: Renderer,
    pub(crate) frame_graph: FrameGraph,
    pub(crate) pass_ids: PassIds,
    pub(crate) frame_graph_bindings: frame_graph_config::FrameGraphBindings,
    pub(crate) frame_graph_runtime: frame_graph_config::FrameGraphRuntime,
    pub(crate) camera: Camera,
    pub(crate) gltf_cache: GltfCache,
    pub(crate) timer: Timer,
    pub(crate) info: ApplicationInfo,
    pub(crate) world: World,
    pub(crate) input_mapper: InputMapper,
    pub(crate) current_modifiers: ModifiersState,
    pub(crate) frame_count: usize,
    pub(crate) last_draw_call_count: usize,
    pub(crate) resources: ResourceManager,
    /// Immediate mode UI context
    pub(crate) ui_context: katla_ui::UiContext,
    /// User preferences (theme, settings)
    pub(crate) preferences: Preferences,
    /// DPI scale factor (physical pixels per logical pixel)
    pub(crate) scale_factor: f32,
    /// Application start time for double-click timestamp calculation
    pub(crate) start_time: Instant,
    /// Default PBR material handle for geometry rendering
    pub(crate) default_material_handle: katla_gfx::MaterialHandle,
    /// Whether the application should exit (set by editor actions, checked in window_event)
    pub(crate) quit_requested: bool,
    /// Flag to prevent double cleanup
    cleaned_up: bool,
    /// Audio system for managing playback of sound effects
    pub(crate) audio_system: Option<crate::systems::AudioSystem>,
    /// Particle system for managing particle emitters via ECS
    pub(crate) particle_system: crate::systems::ParticleSystem,
    /// GPU animation system for pose evaluation (ECS queries only, GPU resources on renderer)
    pub(crate) gpu_animation_system:
        Option<crate::systems::gpu_animation_system::GpuAnimationSystem>,
    /// Whether the window is currently minimized (zero extent).
    pub(crate) minimized: bool,
    /// Whether the swapchain needs recreation (set when acquire/present returns out-of-date).
    pub(crate) needs_swapchain_recreate: bool,
    /// Current 3D-scene render target size in physical pixels. The HDR, depth,
    /// tonemap-output, and picking textures are sized to this (the viewport panel
    /// size under the editor, or the swapchain extent otherwise) so the scene —
    /// composed for the panel aspect ratio — fills the target exactly and the
    /// post-tonemap blit into the drawable is a clean 1:1 copy.
    pub(crate) panel_rt_size: katla_gfx::Size2D,
    /// Tracks GPU resource reference counts for automatic cleanup on entity/component destruction.
    pub(crate) gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker,
    /// CPU-side geometry data cache keyed by MeshHandle for collider generation etc.
    pub(crate) geometry_cache: GeometryCache,
    /// Reusable buffer for collecting point lights each frame. Cleared and refilled
    /// in collect_and_upload_lights to avoid per-frame Vec allocation.
    pub(crate) point_lights_buffer: Vec<katla_gfx::PointLightGPU>,
    /// Editor-only state (UI, picking, gizmos, billboards)
    #[cfg(feature = "editor")]
    pub(crate) editor: EditorState,
    /// Current play mode state (editing, playing, paused).
    #[cfg(feature = "editor")]
    pub(crate) play_mode: game_state::PlayMode,
    /// Snapshot taken before entering play mode, restored on stop.
    #[cfg(feature = "editor")]
    pub(crate) scene_snapshot: Option<game_state::SceneSnapshot>,
    /// Hook called once after build(), before the event loop.
    pub(crate) on_init: Option<builder::InitHook>,
    /// Hook called each frame between world.update(dt) and rendering.
    pub(crate) on_update: Option<builder::UpdateHook>,
    /// Hook called during cleanup_on_exit().
    pub(crate) on_shutdown: Option<builder::ShutdownHook>,
    /// File watcher for shader and texture hot reload (editor only).
    #[cfg(feature = "editor")]
    pub(crate) asset_watcher: Option<crate::util::AssetWatcher>,
    /// Whether the layout dump has already been performed.
    layout_dumped: bool,
}

impl Application {
    /// Returns true when running in headless mode (no window).
    #[expect(dead_code)]
    pub(crate) fn is_headless(&self) -> bool {
        self.info.headless
    }

    /// Helper to access the window when it exists.
    #[expect(dead_code)]
    pub(crate) fn window(&self) -> &Window {
        self.window
            .as_ref()
            .expect("window accessed in headless mode")
    }

    /// Helper to access the window mutably when it exists.
    #[expect(dead_code)]
    pub(crate) fn window_mut(&mut self) -> &mut Window {
        self.window
            .as_mut()
            .expect("window accessed in headless mode")
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref window) = self.window {
            // Enable IME for text input (required for receiving text input events)
            window.set_ime_allowed(true);

            // Get initial DPI scale factor
            self.scale_factor = window.scale_factor() as f32;

            // Kickstart the render loop on macOS. Without this, RedrawRequested
            // events are not delivered until the window is explicitly shown or
            // receives focus (e.g., desktop switch), leaving the window blank.
            window.request_redraw();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.should_track_mouse_motion()
        {
            let (dx, dy) = (delta.0 as f32, delta.1 as f32);
            self.forward_mouse_delta(dx, dy);
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

            self.on_mouse_input(state, button);

            if let Some(action) = self.input_mapper.get_action(&binding) {
                let send_input = self.should_send_game_input();

                if send_input {
                    let pressed = matches!(state, ElementState::Pressed);
                    if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
                        input.set_action_state(action, pressed);
                    }
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
                    .input_mut()
                    .set_mouse_button_with_time(btn, pressed, time);
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

                    // Defer swapchain recreation to the next RedrawRequested to
                    // avoid recreating mid-event-batch on macOS, which can cause
                    // descriptor set invalidation between record and submit.
                    self.needs_swapchain_recreate = true;
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
                self.ui_context.input_mut().set_mouse_pos(mouse_pos);
                self.on_cursor_moved(mouse_pos);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => Vec2::new(x, y),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Normalize trackpad pixel deltas to line-like units (~40px per line)
                        // so the scroll_area multiplier produces consistent speeds
                        let line_height = 40.0;
                        Vec2::new(
                            pos.x as f32 / self.scale_factor / line_height,
                            pos.y as f32 / self.scale_factor / line_height,
                        )
                    }
                };
                self.ui_context.input_mut().scroll_delta = scroll;
                self.forward_scroll_to_camera(delta);
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
                    self.needs_swapchain_recreate = true;
                    if let Some(ref window) = self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    let key_combo = KeyCombo::with_modifiers(keycode, self.current_modifiers);
                    let binding = InputBinding::Keyboard(key_combo);

                    self.on_keyboard_input(&event, keycode, event_loop);

                    // Try lookup with current modifiers first, then fall back to
                    // no-modifiers. This allows Shift+W to still trigger MoveForward
                    // while Shift alone triggers Sprint.
                    let action = self.input_mapper.get_action(&binding).or_else(|| {
                        let plain = InputBinding::Keyboard(KeyCombo::key(keycode));
                        self.input_mapper.get_action(&plain)
                    });

                    if let Some(action) = action {
                        let send_input = self.should_send_game_input();

                        if send_input {
                            let pressed = matches!(event.state, ElementState::Pressed);
                            if let Some(input) =
                                self.world.get_resource_mut::<crate::input::InputState>()
                            {
                                input.set_action_state(action, pressed);
                            }
                        }
                    }

                    let ui_key = Self::winit_to_ui_key(keycode);
                    if let Some(key) = ui_key {
                        match event.state {
                            ElementState::Pressed => self.ui_context.input_mut().add_key_press(key),
                            ElementState::Released => {
                                self.ui_context.input_mut().add_key_release(key)
                            }
                        }
                    }

                    // Forward printable characters from logical key to UI.
                    // IME::Commit only fires for composed text (CJK etc.),
                    // not for simple ASCII keypresses on Windows.
                    if event.state == ElementState::Pressed {
                        let ctrl_held = self
                            .current_modifiers
                            .contains(winit::keyboard::ModifiersState::CONTROL);

                        if !ctrl_held {
                            if let winit::keyboard::Key::Character(ch) = &event.logical_key {
                                for c in ch.chars() {
                                    self.ui_context.input_mut().add_char(c);
                                }
                            } else if matches!(
                                &event.logical_key,
                                winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space)
                            ) {
                                self.ui_context.input_mut().add_char(' ');
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
                            self.ui_context.input_mut().add_char(c);
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor as f32;
                debug!("DPI scale factor changed to {}", self.scale_factor);
            }
            WindowEvent::RedrawRequested => {
                self.handle_redraw_requested(event_loop);
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
    /// Get the default PBR material handle.
    pub fn default_material(&self) -> katla_gfx::MaterialHandle {
        self.default_material_handle
    }

    /// Look up cached CPU-side geometry for a mesh handle.
    pub fn mesh_geometry(
        &self,
        handle: katla_gfx::MeshHandle,
    ) -> Option<&std::sync::Arc<crate::geometry_cache::MeshGeometryData>> {
        self.geometry_cache.get(handle)
    }

    /// Forward mouse motion delta to the ECS input state for orbit/pan camera.
    fn forward_mouse_delta(&mut self, dx: f32, dy: f32) {
        let Some(input) = self.world.get_resource::<crate::input::InputState>() else {
            return;
        };
        let should_track = input.is_action_pressed(Action::LookEnable)
            || input.is_action_pressed(Action::PanEnable);
        if should_track {
            let current_delta = input.mouse_delta;
            if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
                input.mouse_delta = (current_delta.0 + dx, current_delta.1 + dy);
            }
        }
    }

    /// Forward scroll wheel delta to the ECS input state for orbit camera zoom.
    fn forward_scroll_to_camera(&mut self, delta: winit::event::MouseScrollDelta) {
        let line_height = 40.0;
        let wheel_y = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => y,
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                pos.y as f32 / self.scale_factor / line_height
            }
        };

        let wheel_y = self.filter_scroll_for_editor(wheel_y);

        if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
            input.mouse_wheel_delta += wheel_y;
        }
    }
}
