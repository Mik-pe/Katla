//! Application builder for configuring and constructing the engine.
//!
//! # Handle-Based Asset Workflow
//!
//! Katla uses opaque handles to reference GPU resources. Handles are cheap to copy
//! and store, but the underlying GPU resources are owned by the renderer. The typical
//! workflow is:
//!
//! 1. **Load resources** after `build()` and `init()` via [`Application`] methods:
//!    - [`Application::load_texture(path)`] → `AppResult<TextureHandle>` — image files (PNG, JPEG)
//!    - [`Application::load_mesh(path)`] → `AppResult<MeshHandle>` — GLTF/GLB files
//!    - [`Application::load_animation(path, name)`] → `AppResult<AnimationClip>` — GLTF animations
//!
//! 2. **Spawn entities** using handles:
//!    - [`Spawner::spawn_primitive`] on `World` for simple mesh+material entities
//!    - [`Application::spawn_gltf_model`] for full GLTF import with textures and animation
//!
//! 3. **Track resources** for cleanup:
//!    - [`GpuResourceTracker`] automatically handles reference-counted cleanup
//!    - Entity destruction releases tracked GPU resources when ref counts reach zero
//!
//! Handles are valid for the lifetime of the renderer. Destroying a handle explicitly
//! via `renderer.destroy_mesh(handle)` is safe but usually unnecessary — the tracker
//! handles it automatically.

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Instant;

use katla_ecs::{System, SystemExecutionOrder, TypedSystem, World};
use katla_ui::{FontId, ForkAwesome};
use winit::dpi::LogicalSize;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::Window;

#[cfg(test)]
use katla_gfx::GpuRenderer;

use crate::{FrameGraph, Renderer};

use super::camera::Camera;
use super::frame_graph_config::{
    ApplicationFrameGraph, FrameGraphFactory, FrameGraphRuntime, KatlaEditorFrameGraphPreset,
};

use crate::util::{GLTFModel, GltfCache};
use crate::{
    application::{Application, ApplicationInfo},
    error::AppResult,
    input::InputMapper,
    preferences::Preferences,
    resources::ResourceManager,
    util::Timer,
};

/// Hook types stored on Application.
pub(crate) type InitHook = Box<dyn FnOnce(&mut Application)>;
pub(crate) type UpdateHook = Box<dyn FnMut(&mut World, f32)>;
pub(crate) type ShutdownHook = Box<dyn FnOnce(&mut Application)>;

/// Default font sizes for UI text (in pixels)
const DEFAULT_UI_FONT_SIZES: &[f32] = &[14.0, 16.0];

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_mode: katla_gfx::ValidationMode,
    max_frames: Option<usize>,
    check_black_frames: bool,
    world: World,
    scene_path: Option<String>,
    scene_components: crate::scene::SceneComponentRegistry,
    dump_layout_path: Option<super::DumpLayoutTarget>,
    dump_render_graph: Option<super::DumpLayoutTarget>,
    headless: bool,
    screenshot_path: Option<String>,
    ui_test_path: Option<String>,
    interaction_test_path: Option<String>,
    on_init: Option<InitHook>,
    on_update: Option<UpdateHook>,
    on_shutdown: Option<ShutdownHook>,
    frame_graph_factory: Option<FrameGraphFactory>,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install game component codecs before startup scene loading.
    pub fn with_scene_components(
        mut self,
        components: crate::scene::SceneComponentRegistry,
    ) -> Self {
        self.scene_components = components;
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn validation_layer(mut self, on: bool) -> Self {
        self.validation_mode = if on {
            katla_gfx::ValidationMode::Enabled
        } else {
            katla_gfx::ValidationMode::Disabled
        };
        self
    }

    pub fn gpu_assisted_validation(mut self, on: bool) -> Self {
        if on {
            self.validation_mode = katla_gfx::ValidationMode::GpuAssisted;
        }
        self
    }

    pub fn single_frame(mut self, on: bool) -> Self {
        // When single_frame is enabled, render some frames for better validation
        self.max_frames = if on { Some(25) } else { None };
        self
    }

    pub fn max_frames(mut self, count: usize) -> Self {
        self.max_frames = Some(count);
        self
    }

    pub fn check_black_frames(mut self, enabled: bool) -> Self {
        self.check_black_frames = enabled;
        self
    }

    /// Set the scene file to load on startup (relative path or absolute).
    ///
    /// If not set, the default scene (`assets/scenes/default.katla`) is loaded.
    pub fn with_scene_path(mut self, path: impl Into<String>) -> Self {
        self.scene_path = Some(path.into());
        self
    }

    /// Replace Katla's editor graph preset with an application-owned graph.
    ///
    /// The factory runs exactly once after the renderer and resource paths are
    /// available. Returning an error aborts construction; Katla never silently
    /// falls back to its default preset. `ApplicationFrameGraph::new` selects a
    /// graph-only runtime with no required pass names or hidden scene submissions.
    pub fn with_frame_graph(
        mut self,
        factory: impl FnOnce(&mut Renderer, &ResourceManager) -> AppResult<ApplicationFrameGraph>
        + 'static,
    ) -> Self {
        self.frame_graph_factory = Some(Box::new(factory));
        self
    }

    /// Dump the UI layout tree to stdout after the first frame, then exit.
    pub fn dump_layout_to_stdout(mut self) -> Self {
        self.dump_layout_path = Some(super::DumpLayoutTarget::Stdout);
        self
    }

    /// Dump the UI layout tree to a file after the first frame, then exit.
    pub fn dump_layout_to_file(mut self, path: impl Into<String>) -> Self {
        self.dump_layout_path = Some(super::DumpLayoutTarget::File(path.into()));
        self
    }

    /// Dump the compiled render-graph diagnostics to stdout after the first frame, then exit.
    pub fn dump_render_graph_to_stdout(mut self) -> Self {
        self.dump_render_graph = Some(super::DumpLayoutTarget::Stdout);
        self
    }

    /// Dump the compiled render-graph diagnostics to a file after the first frame, then exit.
    pub fn dump_render_graph_to_file(mut self, path: impl Into<String>) -> Self {
        self.dump_render_graph = Some(super::DumpLayoutTarget::File(path.into()));
        self
    }

    /// Enable headless mode — no window, offscreen rendering, screenshot output.
    pub fn headless(mut self, enabled: bool) -> Self {
        self.headless = enabled;
        self
    }

    /// Set the screenshot output path (for headless mode).
    pub fn screenshot_path(mut self, path: impl Into<String>) -> Self {
        self.screenshot_path = Some(path.into());
        self
    }

    /// Enable UI test mode: capture multiple screenshots at different UI states.
    /// Implies `--headless` and `--single-frame`. The directory will be created if it doesn't exist.
    pub fn ui_test_path(mut self, dir: impl Into<String>) -> Self {
        self.ui_test_path = Some(dir.into());
        self
    }

    /// Enable interaction test mode: drive synthetic mouse input (UI clicks,
    /// wheel scrolling, viewport picking) headless and capture screenshots.
    /// Implies `--headless` and `--single-frame`. The directory will be created if it doesn't exist.
    pub fn interaction_test_path(mut self, dir: impl Into<String>) -> Self {
        self.interaction_test_path = Some(dir.into());
        self
    }

    #[cfg(feature = "editor")]
    fn load_editor_state_static(
        preferences: &Preferences,
    ) -> (crate::ui::ColorScheme, crate::gui_state::GuiState) {
        let theme = crate::ui::ColorScheme::by_name(&preferences.theme).unwrap_or_default();
        let gui_state = crate::gui_state::GuiState::load();
        log::info!(
            "Loaded GUI state: left_panel={}, right_panel={}, asset_browser_height={}",
            gui_state.left_panel_width,
            gui_state.right_panel_width,
            gui_state.asset_browser_height
        );
        (theme, gui_state)
    }

    /// Add a system whose access is derived from its typed parameters.
    pub fn with_typed_system<S: TypedSystem>(
        mut self,
        system: S,
        order: SystemExecutionOrder,
    ) -> Self {
        self.world.register_typed_system(system, order);
        self
    }

    /// Add a system that accesses the full world on the calling thread.
    pub fn with_exclusive_system(
        mut self,
        system: Box<dyn System>,
        order: SystemExecutionOrder,
    ) -> Self {
        self.world.register_exclusive_system(system, order);
        self
    }

    /// Register a hook that runs once after `build()` returns, before the event loop starts.
    ///
    /// Use this to spawn initial entities or configure application state that requires
    /// a fully initialized renderer.
    pub fn on_init(mut self, f: impl FnOnce(&mut Application) + 'static) -> Self {
        self.on_init = Some(Box::new(f));
        self
    }

    /// Register a hook that runs each frame between `world.update(dt)` and rendering.
    ///
    /// Receives a mutable reference to the World and the delta time in seconds.
    /// Use this for per-frame game logic that needs to run after ECS systems but
    /// before rendering (e.g., custom physics, AI, procedural generation).
    pub fn on_update(mut self, f: impl FnMut(&mut World, f32) + 'static) -> Self {
        self.on_update = Some(Box::new(f));
        self
    }

    /// Register a hook that runs once during `cleanup_on_exit()`.
    ///
    /// Use this for game-side cleanup (e.g., saving state, releasing external resources).
    pub fn on_shutdown(mut self, f: impl FnOnce(&mut Application) + 'static) -> Self {
        self.on_shutdown = Some(Box::new(f));
        self
    }

    fn build_event_loop() -> AppResult<EventLoop<()>> {
        let event_loop =
            EventLoop::new().map_err(|e| crate::error::AppError::RendererInitFailed {
                reason: e.to_string(),
            })?;
        event_loop.set_control_flow(ControlFlow::Poll);
        Ok(event_loop)
    }

    /// Initialize the renderer using the default backend for the current platform.
    ///
    /// macOS uses Metal, all other platforms use Vulkan.
    fn init_renderer(
        event_loop: &EventLoop<()>,
        window: &Window,
        info: &ApplicationInfo,
        _resources: &ResourceManager,
    ) -> AppResult<Renderer> {
        let engine_name =
            CString::new("Katla Engine").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;
        let app_name =
            CString::new(info.name.as_str()).map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;

        let renderer = {
            #[cfg(target_os = "macos")]
            {
                Renderer::new_metal(
                    event_loop,
                    window,
                    info.validation_mode,
                    app_name,
                    engine_name,
                )
                .map_err(|e| crate::error::AppError::Graphics { source: e })?
            }
            #[cfg(not(target_os = "macos"))]
            {
                Renderer::new_vulkan(
                    event_loop,
                    window,
                    katla_gfx::Size2D::new(window.inner_size().width, window.inner_size().height),
                    info.validation_mode,
                    app_name,
                    engine_name,
                )
                .map_err(|e| crate::error::AppError::Graphics { source: e })?
            }
        };

        Ok(renderer)
    }

    fn build_selected_frame_graph(
        factory: Option<FrameGraphFactory>,
        renderer: &mut Renderer,
        resources: &ResourceManager,
    ) -> AppResult<ApplicationFrameGraph> {
        match factory {
            Some(factory) => factory(renderer, resources),
            None => KatlaEditorFrameGraphPreset::build(renderer, resources),
        }
    }

    fn prepare_frame_graph(
        configured: ApplicationFrameGraph,
    ) -> AppResult<(
        FrameGraph,
        super::PassIds,
        super::frame_graph_config::FrameGraphBindings,
        FrameGraphRuntime,
    )> {
        let (frame_graph, bindings, runtime) = configured.into_parts();
        bindings.validate_resources(&frame_graph)?;
        let pass_ids = super::PassIds::resolve(&frame_graph, &bindings.passes)?;

        Ok((frame_graph, pass_ids, bindings, runtime))
    }

    fn initialize_frame_graph(
        frame_graph: &mut FrameGraph,
        renderer: &mut Renderer,
        bindings: &super::frame_graph_config::FrameGraphBindings,
        runtime: FrameGraphRuntime,
    ) -> AppResult<()> {
        frame_graph
            .initialize_transient_textures(renderer)
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
        frame_graph
            .initialize_transient_buffers(renderer)
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        for name in [
            bindings.resources.hdr_color.as_deref(),
            bindings.resources.viewport.as_deref(),
            bindings.resources.tonemap_output.as_deref(),
            bindings.resources.stencil_indicator.as_deref(),
            (runtime.uses_katla_scene() && frame_graph.resource_id("scene_depth").is_some())
                .then_some("scene_depth"),
        ]
        .into_iter()
        .flatten()
        {
            frame_graph
                .register_transient_texture_bindless(renderer, name)
                .map_err(|source| crate::AppError::Graphics {
                    source: source.into(),
                })?;
        }

        Ok(())
    }

    #[cfg(feature = "editor")]
    fn create_asset_watcher() -> Option<crate::util::AssetWatcher> {
        use std::path::PathBuf;

        let resources_dir = PathBuf::from("resources");
        let dirs = vec![resources_dir];

        match crate::util::AssetWatcher::new(&dirs) {
            Ok(watcher) => Some(watcher),
            Err(e) => {
                log::warn!("Failed to create asset watcher: {e}");
                None
            }
        }
    }

    /// Build a headless application for offscreen rendering.
    ///
    /// Returns an `Application` configured for headless rendering (no window),
    /// ready to run N frames and save a screenshot PNG.
    pub fn build_headless(
        mut self,
        max_frames: usize,
        screenshot_path: String,
    ) -> AppResult<Application> {
        // Install logger. With the editor, the console logger wraps env_logger
        // and owns the global logger slot; plain env_logger otherwise.
        #[cfg(feature = "editor")]
        let log_buffer = Self::install_console_logger();
        #[cfg(not(feature = "editor"))]
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init()
            .ok();

        let preferences = Preferences::load();
        #[cfg(feature = "editor")]
        let (theme, gui_state) = Self::load_editor_state_static(&preferences);
        let frame_graph_factory = self.frame_graph_factory.take();

        let info = ApplicationInfo {
            name: self.app_name.clone(),
            validation_mode: self.validation_mode,
            max_frames: Some(max_frames),
            check_black_frames: false,
            scene_path: self.scene_path.clone(),
            dump_layout_path: self.dump_layout_path.clone(),
            dump_render_graph: self.dump_render_graph.clone(),
            screenshot_path: Some(screenshot_path),
            headless: true,
            ui_test_path: self.ui_test_path.clone(),
            interaction_test_path: self.interaction_test_path.clone(),
        };

        let mut world = self.world;
        let camera = Camera::new(&mut world);
        let resources = ResourceManager::discover()?;

        let mut ui_context = katla_ui::UiContext::new();
        let scale_factor = crate::application::headless::HEADLESS_SCALE_FACTOR;

        // Create headless Metal renderer
        let engine_name =
            CString::new("Katla Engine").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;
        let app_name =
            CString::new("Katla Headless").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;

        #[cfg(target_os = "macos")]
        let mut renderer = Renderer::new_metal_headless(
            crate::application::headless::HEADLESS_WIDTH,
            crate::application::headless::HEADLESS_HEIGHT,
            self.validation_mode,
            app_name,
            engine_name,
        )
        .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        #[cfg(not(target_os = "macos"))]
        let mut renderer = Renderer::new_vulkan_headless(
            crate::application::headless::HEADLESS_WIDTH,
            crate::application::headless::HEADLESS_HEIGHT,
            self.validation_mode,
            app_name,
            engine_name,
        )
        .map_err(|source| crate::error::AppError::Graphics { source })?;

        let configured_frame_graph =
            Self::build_selected_frame_graph(frame_graph_factory, &mut renderer, &resources)?;
        let (mut frame_graph, mut pass_ids, frame_graph_bindings, frame_graph_runtime) =
            Self::prepare_frame_graph(configured_frame_graph)?;
        frame_graph.set_execution_trace(info.dump_render_graph.is_some());
        let mut scene_features = frame_graph_runtime
            .uses_katla_scene()
            .then(|| {
                super::scene_features::SceneFeatures::new(
                    &mut renderer,
                    &resources,
                    &frame_graph_bindings,
                )
            })
            .transpose()?;
        if let Some(features) = &mut scene_features {
            features.animation.install_graph(&mut frame_graph)?;
            features.lights.install_graph(&mut frame_graph)?;
            features.particles.install_graph(&mut frame_graph)?;
            pass_ids.refresh(&frame_graph, &frame_graph_bindings.passes)?;
            features
                .animation
                .warm_pipeline(&mut renderer, &frame_graph)?;
            frame_graph.initialize_compute_pipelines(&mut renderer)?;
        }
        Self::initialize_frame_graph(
            &mut frame_graph,
            &mut renderer,
            &frame_graph_bindings,
            frame_graph_runtime,
        )?;
        let mut editor_features = super::features::EditorFeatures::default();
        if frame_graph_runtime.uses_katla_scene() {
            let font_path = resources.font_path("roboto-regular.ttf");
            if font_path.exists()
                && let Ok(font_bytes) = std::fs::read(&font_path)
            {
                let font_id = ui_context.fonts_mut().add_font(&font_bytes).ok();
                if let Some(font_id) = font_id {
                    for &size in DEFAULT_UI_FONT_SIZES {
                        ui_context
                            .fonts_mut()
                            .precache_ascii(font_id, size, scale_factor);
                    }
                    ui_context.set_font(font_id);
                }
            }
            let icon_font_path = resources.font_path("forkawesome-webfont.ttf");
            if icon_font_path.exists()
                && let Ok(font_bytes) = std::fs::read(&icon_font_path)
                && ui_context
                    .fonts_mut()
                    .add_font_with_id(&font_bytes, katla_ui::FontId::ICON)
                    .is_ok()
            {
                for &size in DEFAULT_UI_FONT_SIZES {
                    ui_context.fonts_mut().precache_icons(
                        katla_ui::FontId::ICON,
                        size,
                        scale_factor,
                        katla_ui::ForkAwesome::common_icons(),
                    );
                }
            }

            editor_features.upload_font_atlas(&mut renderer, &mut ui_context)?;
        }

        #[cfg(feature = "editor")]
        let mut ui_renderer = crate::ui::UIRenderer::new();
        #[cfg(feature = "editor")]
        if let Some(slot) = editor_features.font_atlas_slot() {
            ui_renderer.set_font_atlas_bindless_slot(slot);
        }

        // Insert required ECS resources
        world.insert_resource(crate::input::InputState::new());
        world.insert_resource(katla_script::ScriptsActive(false));
        world.insert_resource(katla_script::PendingAudioCommands::default());
        world.insert_resource(katla_script::PendingRaycastCommands::default());
        world.insert_resource(katla_script::PendingRaycastResults::default());
        world.insert_resource(katla_script::PendingPhysicsEvents::default());
        world.insert_resource(katla_script::ScriptInspectorData::default());
        world.insert_resource(katla_script::PopulateScriptInspector(false));
        world.insert_resource(katla_script::PendingScriptVarEdits::default());
        world.insert_resource(katla_physics::PhysicsWorld::new());
        world.insert_resource(katla_physics::PhysicsActive(false));
        world.insert_resource(crate::geometry_cache::GeometryCache::default());
        world.insert_resource(crate::resources::AmbientLight::default());

        let gltf_loader: crate::util::GltfLoaderFn = Box::new(|path: &PathBuf| {
            crate::util::GLTFModel::new(path).map_err(|e| {
                log::error!("Failed to load GLTF model from {:?}: {e}", path);
                e
            })
        });

        let app = Application {
            window: None,
            renderer,
            frame_graph,
            pass_ids,
            frame_graph_bindings,
            frame_graph_runtime,
            editor_features,
            scene_features,
            camera,
            gltf_cache: GltfCache::new(gltf_loader),
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
            frame_readback: None,
            last_draw_call_count: 0,
            resources,
            ui_context,
            #[cfg(feature = "editor")]
            editor: { super::EditorState::new(ui_renderer, theme, &preferences, gui_state) },
            preferences,
            scale_factor: crate::application::headless::HEADLESS_SCALE_FACTOR,
            start_time: Instant::now(),
            default_material_handle: katla_gfx::MaterialHandle::NONE,
            cleaned_up: false,
            quit_requested: false,
            particle_system: crate::systems::ParticleSystem::new(),
            gpu_animation_system: None,
            audio_system: None,
            minimized: false,
            needs_swapchain_recreate: false,
            panel_rt_size: katla_gfx::Size2D::new(0, 0),
            gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker::new(
                katla_gfx::MaterialHandle::NONE,
            ),
            geometry_cache: crate::geometry_cache::GeometryCache::default(),
            mesh_assets: crate::mesh_asset::MeshAssetCache::default(),
            point_lights_buffer: Vec::new(),
            on_init: self.on_init,
            on_update: self.on_update,
            on_shutdown: self.on_shutdown,
            #[cfg(feature = "editor")]
            play_mode: super::game_state::PlayMode::Editing,
            #[cfg(feature = "editor")]
            scene_snapshot: None,
            scene_components: self.scene_components,
            scene_document: crate::scene::document::SceneDocument::default(),
            #[cfg(feature = "editor")]
            asset_watcher: None,
            layout_dumped: false,
            render_graph_dumped: false,
        };

        #[cfg(feature = "editor")]
        let mut app = app;
        #[cfg(feature = "editor")]
        app.editor.editor_ui.set_log_buffer(log_buffer);

        Ok(app)
    }

    /// Install the console logger (wrapping env_logger so stderr output is
    /// preserved) and return the shared buffer backing the editor console panel.
    #[cfg(feature = "editor")]
    fn install_console_logger() -> std::sync::Arc<std::sync::Mutex<crate::ui::console::LogBuffer>> {
        use crate::ui::console::ConsoleLoggerHandle;
        let secondary =
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .build();
        let level = secondary.filter();
        let console_handle = ConsoleLoggerHandle::init(level, Box::new(secondary));
        let buffer = console_handle.buffer();
        log::set_boxed_logger(console_handle.into_logger()).ok();
        log::set_max_level(level);
        buffer
    }

    pub fn build(mut self) -> AppResult<(Application, EventLoop<()>)> {
        let event_loop = Self::build_event_loop()?;

        // Install console logger early so all subsequent log messages are captured.
        // Wraps env_logger as secondary so stderr output is preserved.
        #[cfg(feature = "editor")]
        let log_buffer = Self::install_console_logger();
        #[cfg(not(feature = "editor"))]
        let _ = (); // no console logger without editor

        // Load user preferences and editor state before moving fields
        let preferences = Preferences::load();
        #[cfg(feature = "editor")]
        let (theme, gui_state) = Self::load_editor_state_static(&preferences);

        let frame_graph_factory = self.frame_graph_factory.take();

        let info = ApplicationInfo {
            name: self.app_name,
            validation_mode: self.validation_mode,
            max_frames: self.max_frames,
            check_black_frames: self.check_black_frames,
            scene_path: self.scene_path,
            dump_layout_path: self.dump_layout_path,
            dump_render_graph: self.dump_render_graph,
            screenshot_path: None,
            headless: false,
            ui_test_path: None,
            interaction_test_path: None,
        };

        let mut world = self.world;
        let camera = Camera::new(&mut world);

        let resources = ResourceManager::discover()?;

        log::info!(
            "Loaded preferences: theme={}, show_grid={}, show_stats={}, font_scale={}",
            preferences.theme,
            preferences.show_grid,
            preferences.show_stats,
            preferences.font_scale
        );

        // Create UI context and load default font
        let mut ui_context = katla_ui::UiContext::new();

        // Set up OS clipboard for copy/cut/paste
        match crate::application::clipboard::OsClipboard::new() {
            Ok(cb) => ui_context.set_clipboard_provider(Box::new(cb)),
            Err(e) => log::warn!("Failed to initialize clipboard: {}", e),
        }

        let gltf_loader: crate::util::GltfLoaderFn = Box::new(|path: &PathBuf| {
            GLTFModel::new(path).map_err(|e| {
                log::error!("Failed to load GLTF model from {:?}: {e}", path);
                e
            })
        });

        #[allow(deprecated)]
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(&info.name)
                    .with_resizable(true)
                    .with_maximized(true)
                    .with_min_inner_size(LogicalSize {
                        width: 800.0,
                        height: 600.0,
                    }),
            )
            .map_err(|e| crate::error::AppError::RendererInitFailed {
                reason: e.to_string(),
            })?;

        let mut renderer = Self::init_renderer(&event_loop, &window, &info, &resources)?;

        let configured_frame_graph =
            Self::build_selected_frame_graph(frame_graph_factory, &mut renderer, &resources)?;
        let (mut frame_graph, mut pass_ids, frame_graph_bindings, frame_graph_runtime) =
            Self::prepare_frame_graph(configured_frame_graph)?;
        frame_graph.set_execution_trace(info.dump_render_graph.is_some());
        let mut scene_features = frame_graph_runtime
            .uses_katla_scene()
            .then(|| {
                super::scene_features::SceneFeatures::new(
                    &mut renderer,
                    &resources,
                    &frame_graph_bindings,
                )
            })
            .transpose()?;
        if let Some(features) = &mut scene_features {
            features.animation.install_graph(&mut frame_graph)?;
            features.lights.install_graph(&mut frame_graph)?;
            features.particles.install_graph(&mut frame_graph)?;
            pass_ids.refresh(&frame_graph, &frame_graph_bindings.passes)?;
            features
                .animation
                .warm_pipeline(&mut renderer, &frame_graph)?;
            frame_graph.initialize_compute_pipelines(&mut renderer)?;
        }
        Self::initialize_frame_graph(
            &mut frame_graph,
            &mut renderer,
            &frame_graph_bindings,
            frame_graph_runtime,
        )?;
        let mut editor_features = super::features::EditorFeatures::default();
        if frame_graph_runtime.uses_katla_scene() {
            // Load default font for text rendering
            let font_path = resources.font_path("roboto-regular.ttf");
            if font_path.exists() {
                match std::fs::read(&font_path) {
                    Ok(font_bytes) => {
                        let font_result = ui_context.fonts_mut().add_font(&font_bytes);
                        match font_result {
                            Ok(font_id) => {
                                // Precache common ASCII characters at typical UI sizes
                                // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                                // actual DPI scale on first use if different
                                for &size in DEFAULT_UI_FONT_SIZES {
                                    ui_context.fonts_mut().precache_ascii(font_id, size, 1.0);
                                }
                                ui_context.set_font(font_id);
                                log::info!("Loaded default font from {}", font_path.display());
                            }
                            Err(e) => {
                                log::warn!("Failed to parse font: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to read font file {}: {}", font_path.display(), e);
                    }
                }
            } else {
                log::warn!("Font file not found: {}", font_path.display());
            }

            // Load icon font (ForkAwesome)
            let icon_font_path = resources.font_path("forkawesome-webfont.ttf");
            if icon_font_path.exists() {
                match std::fs::read(&icon_font_path) {
                    Ok(font_bytes) => {
                        let icon_font_result = ui_context
                            .fonts_mut()
                            .add_font_with_id(&font_bytes, FontId::ICON);
                        match icon_font_result {
                            Ok(()) => {
                                // Precache common icons at typical UI sizes
                                // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                                // actual DPI scale on first use if different
                                for &size in DEFAULT_UI_FONT_SIZES {
                                    ui_context.fonts_mut().precache_icons(
                                        FontId::ICON,
                                        size,
                                        1.0,
                                        ForkAwesome::common_icons(),
                                    );
                                }
                                log::info!("Loaded icon font from {}", icon_font_path.display());
                            }
                            Err(e) => {
                                log::warn!("Failed to parse icon font: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to read icon font file {}: {}",
                            icon_font_path.display(),
                            e
                        );
                    }
                }
            } else {
                log::warn!("Icon font file not found: {}", icon_font_path.display());
            }

            editor_features.upload_font_atlas(&mut renderer, &mut ui_context)?;
        }

        #[cfg(feature = "editor")]
        let mut ui_renderer = crate::ui::UIRenderer::new();
        #[cfg(feature = "editor")]
        if let Some(slot) = editor_features.font_atlas_slot() {
            ui_renderer.set_font_atlas_bindless_slot(slot);
        }

        world.insert_resource(crate::input::InputState::new());
        world.insert_resource(katla_script::ScriptsActive(false));
        world.insert_resource(katla_script::PendingAudioCommands::default());
        world.insert_resource(katla_script::PendingRaycastCommands::default());
        world.insert_resource(katla_script::PendingRaycastResults::default());
        world.insert_resource(katla_script::PendingPhysicsEvents::default());
        world.insert_resource(katla_script::ScriptInspectorData::default());
        world.insert_resource(katla_script::PopulateScriptInspector(false));
        world.insert_resource(katla_script::PendingScriptVarEdits::default());
        world.insert_resource(katla_physics::PhysicsWorld::new());
        world.insert_resource(katla_physics::PhysicsActive(false));
        world.insert_resource(crate::geometry_cache::GeometryCache::default());

        let app = Application {
            window: Some(window),
            renderer,
            frame_graph,
            pass_ids,
            frame_graph_bindings,
            frame_graph_runtime,
            editor_features,
            scene_features,
            camera,
            gltf_cache: GltfCache::new(gltf_loader),
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
            frame_readback: None,
            last_draw_call_count: 0,
            resources,
            ui_context,
            #[cfg(feature = "editor")]
            editor: {
                let mut state =
                    super::EditorState::new(ui_renderer, theme, &preferences, gui_state);
                state.editor_ui.set_log_buffer(log_buffer);
                state
            },
            preferences,
            scale_factor: 1.0, // Will be updated when window is created
            start_time: Instant::now(),
            default_material_handle: katla_gfx::MaterialHandle::NONE,
            cleaned_up: false,
            quit_requested: false,
            particle_system: crate::systems::ParticleSystem::new(),
            gpu_animation_system: None,
            audio_system: None,
            minimized: false,
            needs_swapchain_recreate: false,
            panel_rt_size: katla_gfx::Size2D::new(0, 0),
            gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker::new(
                katla_gfx::MaterialHandle::NONE,
            ),
            geometry_cache: crate::geometry_cache::GeometryCache::default(),
            mesh_assets: crate::mesh_asset::MeshAssetCache::default(),
            point_lights_buffer: Vec::new(),
            on_init: self.on_init,
            on_update: self.on_update,
            on_shutdown: self.on_shutdown,
            #[cfg(feature = "editor")]
            play_mode: super::game_state::PlayMode::Editing,
            #[cfg(feature = "editor")]
            scene_snapshot: None,
            scene_components: self.scene_components,
            scene_document: crate::scene::document::SceneDocument::default(),
            #[cfg(feature = "editor")]
            asset_watcher: Self::create_asset_watcher(),
            layout_dumped: false,
            render_graph_dumped: false,
        };

        Ok((app, event_loop))
    }

    /// Build, initialize, and run the application in one call.
    ///
    /// Equivalent to `build()`, `init()`, `on_init` callback, and `event_loop.run_app()`.
    /// Returns on error during build; panics if the event loop itself fails.
    pub fn run(self) -> AppResult<()> {
        let (mut application, event_loop) = self.build()?;
        application.init()?;

        // Run the on_init hook after initialization, before the event loop
        if let Some(hook) = application.on_init.take() {
            hook(&mut application);
        }

        event_loop
            .run_app(&mut application)
            .map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use katla_ecs::World;

    #[test]
    #[ignore = "requires a native headless GPU"]
    fn test_headless_graph_only_keeps_scene_and_editor_features_uninitialized() {
        use katla_gfx::render_graph::{FrameGraphBuilder, PassType, SimplePass};
        use katla_gfx::{AttachmentOps, ClearValue, MaterialHandle};

        let mut app = ApplicationBuilder::new()
            .with_frame_graph(|renderer, _resources| {
                let builder = FrameGraphBuilder::new().add_pass(
                    SimplePass::new("application_clear", PassType::Graphics)
                        .write("backbuffer")
                        .attachment("backbuffer", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
                );
                let graph = match renderer {
                    Renderer::Vulkan(_) => {
                        FrameGraph::from_vulkan(builder.build::<katla_gfx::VulkanRenderer>()?)
                    }
                    #[cfg(target_os = "macos")]
                    Renderer::Metal(_) => {
                        FrameGraph::from_metal(builder.build::<katla_gfx::MetalRenderer>()?)
                    }
                };
                Ok(ApplicationFrameGraph::new(graph))
            })
            .build_headless(1, String::new())
            .expect("Build graph-only application");
        assert_eq!(app.frame_graph_runtime, FrameGraphRuntime::GraphOnly);
        assert!(app.scene_features.is_none());
        assert!(!app.editor_features.has_font_atlas());
        assert!(app.ui_context.fonts().get_font(FontId::DEFAULT).is_none());
        assert!(app.ui_context.fonts().get_font(FontId::ICON).is_none());
        assert_eq!(app.default_material_handle, MaterialHandle::NONE);

        app.init().expect("Initialize graph-only application");
        #[cfg(target_os = "macos")]
        {
            let drawable = app.renderer.create_offscreen_texture(32, 32);
            app.renderer.set_headless_drawable(drawable);
        }
        app.render_editor_frame(1.0 / 60.0);
        assert!(app.renderer.capture_submission_snapshot().is_some());
        assert!(app.scene_features.is_none());
        assert!(app.gpu_animation_system.is_none());
        assert!(!app.editor_features.has_font_atlas());
        assert!(app.ui_context.fonts().get_font(FontId::DEFAULT).is_none());
        assert!(app.ui_context.fonts().get_font(FontId::ICON).is_none());
        assert_eq!(app.default_material_handle, MaterialHandle::NONE);
    }

    #[cfg(feature = "editor")]
    #[test]
    #[ignore = "requires a native headless GPU"]
    fn test_scene_material_occlusion_is_independent_of_font_atlas() {
        let mut app = ApplicationBuilder::new()
            .build_headless(1, String::new())
            .expect("Build scene application");
        let textures = app
            .scene_features
            .as_ref()
            .expect("Scene features")
            .material_textures();
        let white = app.renderer.default_texture();
        let white_slot = app.renderer.get_bindless_slot(white).expect("White slot");
        let atlas_slot = app.editor_features.font_atlas_slot().expect("Font slot");
        assert_eq!(textures.albedo, white);
        assert_eq!(textures.occlusion, white);
        assert_eq!(
            app.renderer.get_bindless_slot(textures.occlusion),
            Some(white_slot)
        );
        assert_ne!(white_slot, atlas_slot);
        assert_ne!(textures.normal, white);
        assert_ne!(textures.metallic_roughness, white);
        app.cleanup_on_exit();
    }

    #[test]
    fn test_builder_on_init_stores_hook() {
        let called = Rc::new(RefCell::new(false));
        let called_clone = called.clone();

        let builder = ApplicationBuilder::new().on_init(move |_app: &mut Application| {
            *called_clone.borrow_mut() = true;
        });

        assert!(builder.on_init.is_some(), "on_init hook should be stored");
        drop(called);
    }

    #[test]
    fn test_builder_on_update_stores_hook() {
        let builder = ApplicationBuilder::new().on_update(|_world: &mut World, _dt: f32| {
            // no-op test hook
        });

        assert!(
            builder.on_update.is_some(),
            "on_update hook should be stored"
        );
    }

    #[test]
    fn test_builder_on_shutdown_stores_hook() {
        let builder = ApplicationBuilder::new().on_shutdown(|_app: &mut Application| {
            // no-op test hook
        });

        assert!(
            builder.on_shutdown.is_some(),
            "on_shutdown hook should be stored"
        );
    }

    #[test]
    fn test_builder_on_init_hook_can_access_world() {
        let entity_count = Rc::new(RefCell::new(0usize));
        let entity_count_clone = entity_count.clone();

        let builder = ApplicationBuilder::new().on_init(move |app: &mut Application| {
            // Verify the hook has access to the world
            *entity_count_clone.borrow_mut() = app.world.entity_count();
        });

        assert!(builder.on_init.is_some());

        // Verify the hook closure captures correctly (not yet called)
        assert_eq!(
            *entity_count.borrow(),
            0,
            "Hook should not have been called yet"
        );
        drop(entity_count);
    }

    #[test]
    fn test_builder_on_update_hook_receives_dt() {
        let received_dts = Rc::new(RefCell::new(Vec::<f32>::new()));
        let received_dts_clone = received_dts.clone();

        let mut builder =
            ApplicationBuilder::new().on_update(move |_world: &mut World, dt: f32| {
                received_dts_clone.borrow_mut().push(dt);
            });

        assert!(builder.on_update.is_some());

        // Simulate calling the hook multiple times
        if let Some(ref mut hook) = builder.on_update {
            let mut world = World::new();
            hook(&mut world, 0.016);
            hook(&mut world, 0.033);
            hook(&mut world, 0.050);
        }

        let dts = received_dts.borrow();
        assert_eq!(dts.len(), 3);
        assert!((dts[0] - 0.016).abs() < f32::EPSILON);
        assert!((dts[1] - 0.033).abs() < f32::EPSILON);
        assert!((dts[2] - 0.050).abs() < f32::EPSILON);
    }

    #[test]
    fn test_builder_hooks_chain_with_other_methods() {
        let builder = ApplicationBuilder::new()
            .with_name("test-app")
            .single_frame(true)
            .on_init(|_app| {})
            .on_update(|_world, _dt| {})
            .on_shutdown(|_app| {});

        assert!(builder.on_init.is_some());
        assert!(builder.on_update.is_some());
        assert!(builder.on_shutdown.is_some());
    }

    #[test]
    fn test_builder_default_has_no_hooks() {
        let builder = ApplicationBuilder::default();
        assert!(builder.on_init.is_none());
        assert!(builder.on_update.is_none());
        assert!(builder.on_shutdown.is_none());
    }

    #[test]
    fn test_on_update_hook_can_mutate_world() {
        use katla_ecs::World;

        let mut builder = ApplicationBuilder::new().on_update(|world: &mut World, _dt: f32| {
            world.insert_resource(42i32);
        });

        assert!(builder.on_update.is_some());

        if let Some(ref mut hook) = builder.on_update {
            let mut world = World::new();
            hook(&mut world, 0.016);
            let value = world.get_resource::<i32>();
            assert!(value.is_some());
            assert_eq!(*value.unwrap(), 42);
        }
    }

    #[test]
    fn default_builder_selects_the_explicit_editor_preset() {
        let builder = ApplicationBuilder::new();
        assert!(builder.frame_graph_factory.is_none());
    }

    #[test]
    fn custom_frame_graph_factory_can_only_be_taken_once() {
        let mut builder = ApplicationBuilder::new().with_frame_graph(|renderer, _resources| {
            Ok(ApplicationFrameGraph::new(
                super::super::frame_graph_config::empty_frame_graph(renderer),
            ))
        });

        assert!(builder.frame_graph_factory.take().is_some());
        assert!(builder.frame_graph_factory.take().is_none());
    }
}
