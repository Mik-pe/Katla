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
use std::time::Instant;

use katla_ecs::{System, SystemExecutionOrder, World};
use katla_ui::{FontId, ForkAwesome};
use winit::dpi::LogicalSize;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::Window;

use katla_gfx::GpuRenderer;

use crate::{FrameGraph, Renderer};

use super::camera::Camera;

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
    on_init: Option<InitHook>,
    on_update: Option<UpdateHook>,
    on_shutdown: Option<ShutdownHook>,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
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

    pub fn with_system(mut self, system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        self.world.register_system(system, order);
        self
    }

    /// Add multiple systems with their execution orders.
    ///
    /// # Arguments
    /// * `systems` - Vector of (system, order) tuples
    pub fn with_systems(mut self, systems: Vec<(Box<dyn System>, SystemExecutionOrder)>) -> Self {
        for (system, order) in systems {
            self.world.register_system(system, order);
        }
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

    fn build_event_loop() -> EventLoop<()> {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
    }

    /// Initialize the renderer using the default backend for the current platform.
    ///
    /// macOS uses Metal, all other platforms use Vulkan.
    fn init_renderer(
        event_loop: &EventLoop<()>,
        window: &Window,
        info: &ApplicationInfo,
        _resources: &ResourceManager,
    ) -> Renderer {
        let engine_name = CString::new("Katla Engine").unwrap();
        let app_name = CString::new(info.name.as_str()).unwrap();

        let mut renderer = {
            #[cfg(target_os = "macos")]
            {
                Renderer::new_metal(
                    event_loop,
                    window,
                    info.validation_mode,
                    app_name,
                    engine_name,
                )
                .expect("Failed to initialize Metal renderer")
            }
            #[cfg(not(target_os = "macos"))]
            {
                Renderer::new_vulkan(
                    event_loop,
                    window,
                    info.validation_mode,
                    app_name,
                    engine_name,
                )
                .expect("Failed to initialize Vulkan renderer")
            }
        };

        // Initialize particle system
        renderer
            .init_particle_system()
            .expect("Failed to initialize particle system");

        renderer
    }

    /// Build a minimal frame graph for the Metal backend.
    ///
    /// Creates transient resources (hdr_color, viewport_0) without passes.
    /// Metal uses hardcoded pass execution but benefits from frame graph
    /// transient texture management and bindless registration.

    fn build_metal_frame_graph(renderer: &mut katla_gfx::MetalRenderer) -> AppResult<FrameGraph> {
        use katla_gfx::render_graph::{
            FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassKind, PassType, SimplePass,
        };
        use katla_gfx::texture::ImageFormat;

        let extent = renderer.swapchain_extent();

        let graph = FrameGraphBuilder::new()
            .create_resource(GraphResourceDesc {
                name: "hdr_color".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.1, 0.1, 0.1, 1.0]),
                },
                format: ImageFormat::R16G16B16A16Sfloat,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            .create_resource(GraphResourceDesc {
                name: "viewport_0".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0, 0.0, 0.0, 1.0]),
                },
                format: ImageFormat::B8G8R8A8Srgb,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            .add_pass(SimplePass::new("geometry", PassType::Graphics).write("hdr_color"))
            .add_pass(SimplePass::new("shadow", PassType::Graphics))
            .add_pass(SimplePass::new("depth_prepass", PassType::Graphics))
            .add_pass(SimplePass::new("outline", PassType::Graphics))
            .add_pass(
                SimplePass::new("tonemap", PassType::Graphics)
                    .read("hdr_color")
                    .write("viewport_0"),
            )
            .add_pass(SimplePass::new("ui", PassType::Graphics))
            .build::<katla_gfx::MetalRenderer>()
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        Ok(FrameGraph::from_metal(graph))
    }

    /// Build the frame graph for the application.
    ///
    /// Uses HDR intermediate rendering with tonemapping and multi-viewport compositing:
    /// 1. Sky pass renders procedural sky to HDR texture
    /// 2. Geometry pass renders scene to HDR texture (R16G16B16A16Sfloat)
    /// 3. Tonemap pass samples HDR and outputs to viewport texture
    /// 4. Compositing pass composites viewport textures to backbuffer
    /// 5. UI pass samples from backbuffer (now gets composited result)
    fn build_frame_graph(
        renderer: &mut katla_gfx::VulkanRenderer,
        resources: &ResourceManager,
    ) -> AppResult<FrameGraph> {
        use katla_gfx::render_graph::UIPass;
        use katla_gfx::render_graph::{
            DepthPrepass, FullscreenPass, GeometryPass, GraphResourceDesc, GraphResourceType,
            OutlinePass, ShadowPass, StencilIndicatorPass,
        };
        use katla_gfx::render_pass::{ClearValue, LoadOp, StoreOp};
        use katla_gfx::texture::ImageFormat as TextureImageFormat;

        let extent = renderer.swapchain_extent();

        // Compile sky shader (procedural fullscreen sky) with HDR output format
        let sky_shader_path = resources.shader_path("sky.wgsl");
        let sky_pipeline = renderer
            .compile_fullscreen_shader_with_format(
                sky_shader_path,
                TextureImageFormat::R16G16B16A16Sfloat,
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Compile tonemap shader for post-processing
        let tonemap_shader_path = resources.shader_path("tonemapping.wgsl");
        let tonemap_pipeline = renderer
            .compile_fullscreen_shader(tonemap_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Compile wallhack overlay shader (reads LDR + stencil indicator, applies tint)
        let overlay_shader_path = resources.shader_path("wallhack_overlay.wgsl");
        let overlay_pipeline = renderer
            .compile_fullscreen_shader_with_format(
                overlay_shader_path,
                katla_gfx::ImageFormat::B8G8R8A8Srgb,
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // We'll get the HDR texture index after registering with bindless
        // For now, use None - it will be set during app init
        let tonemap_params = katla_gfx::TonemapParams {
            exposure: 0.4,
            gamma: 2.2,
            mode: katla_gfx::TonemapOperator::Aces,
            hdr_texture_index: None,
        };

        // Compile UI shader for editor UI rendering
        let ui_shader_path = resources.shader_path("ui/ui.wgsl");
        let ui_material = renderer
            .compile_material(
                ui_shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Ui,
                    alpha_blended: true,
                    double_sided: true,
                    ..Default::default()
                },
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize Forward+ light culling system BEFORE compiling PBR materials,
        // since PBR pipelines need Set 3 for light culling data.
        let light_cull_shader_path = resources.shader_path("lighting/light_cull.wgsl");
        renderer
            .init_light_culling(extent.width, extent.height, &light_cull_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize shadow resources BEFORE compiling PBR materials,
        // since PBR pipelines need Set 4 for shadow data.
        // Shadow atlas view will be set after frame graph creates the transient texture.
        use katla_gfx::CascadeParams;
        renderer
            .init_shadow_resources(None, CascadeParams::default())
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Register depth textures with bindless for screen-space effects (contact shadows, AO)
        let depth_texture_base = renderer
            .register_depth_textures_bindless()
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;
        log::info!(
            "Depth textures registered with bindless at base slot {}",
            depth_texture_base
        );

        // Initialize shadow depth pipeline (depth-only rendering from light's perspective)
        let shadow_shader_path = resources.shader_path("shadow/shadow_depth.wgsl");
        renderer
            .init_shadow_pipeline(&shadow_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let shadow_skinned_shader_path = resources.shader_path("shadow/shadow_depth_skinned.wgsl");
        renderer
            .init_shadow_pipeline_skinned(&shadow_skinned_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let depth_prepass_shader_path = resources.shader_path("depth_prepass.wgsl");
        renderer
            .init_depth_prepass_pipeline(&depth_prepass_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let depth_prepass_skinned_shader_path = resources.shader_path("depth_prepass_skinned.wgsl");
        renderer
            .init_depth_prepass_skinned_pipeline(&depth_prepass_skinned_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let billboard_depth_shader_path = resources.shader_path("billboard_depth.wgsl");
        renderer
            .init_depth_prepass_billboard_pipeline(&billboard_depth_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize outline pipelines for stencil-based selection highlight
        let stencil_mark_shader_path = resources.shader_path("outline/stencil_mark.wgsl");
        let stencil_mark_skinned_shader_path =
            resources.shader_path("outline/stencil_mark_skinned.wgsl");
        let outline_draw_shader_path = resources.shader_path("outline/outline_draw.wgsl");
        let outline_draw_skinned_shader_path =
            resources.shader_path("outline/outline_draw_skinned.wgsl");
        renderer
            .init_outline_pipelines(
                &stencil_mark_shader_path,
                &stencil_mark_skinned_shader_path,
                &outline_draw_shader_path,
                &outline_draw_skinned_shader_path,
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize stencil indicator pipeline for wallhack overlay
        let stencil_indicator_shader_path = resources.shader_path("outline/stencil_indicator.wgsl");
        let stencil_indicator_skinned_shader_path =
            resources.shader_path("outline/stencil_indicator_skinned.wgsl");
        renderer
            .init_stencil_indicator_pipelines(
                &stencil_indicator_shader_path,
                &stencil_indicator_skinned_shader_path,
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Compile geometry shader for PBR model rendering
        log::info!("About to compile PBR geometry shader...");
        let geometry_shader_path = resources.shader_path("model_pbr.wgsl");
        let geometry_material = renderer
            .compile_material(
                geometry_shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    alpha_blended: false,
                    ..Default::default()
                },
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        log::info!("PBR geometry shader compiled successfully");

        // Compile particle rendering shader with alpha blending
        let particle_shader_path = resources.shader_path("particles/particle_render.wgsl");

        // Initialize particle render pipeline using the renderer's method
        renderer
            .init_particle_render_pipeline(&particle_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Note: Particle compute pipelines (emit and simulate) will be initialized
        // later in Application::init() after the builder returns.
        // The particle system handles compute passes directly during execution,
        // not as part of the frame graph passes.
        // Workgroup counts are calculated dynamically each frame based on
        // emit_count and alive_count.

        let graph = renderer
            .create_frame_graph()
            // Create HDR color texture for geometry pass output
            .create_resource(GraphResourceDesc {
                name: "hdr_color".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.1, 0.1, 0.1, 1.0]),
                },
                format: TextureImageFormat::R16G16B16A16Sfloat,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            // Create viewport texture for tonemap output (LDR, sRGB for backbuffer compatibility)
            // Use B8G8R8A8Srgb to match backbuffer format (tonemap shader expects this)
            .create_resource(GraphResourceDesc {
                name: "viewport_0".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0, 0.0, 0.0, 1.0]),
                },
                format: TextureImageFormat::B8G8R8A8Srgb,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            // Create shadow atlas (depth texture, 2x2 cascade grid)
            .create_resource(GraphResourceDesc {
                name: "shadow_atlas".to_string(),
                resource_type: GraphResourceType::DepthAttachment {
                    clear_value: 1.0,
                    sampled: true,
                },
                format: TextureImageFormat::D32Sfloat,
                width: 4096,
                height: 4096,
                tracks_swapchain_size: false,
            })
            // Create object-ID picking texture (R32Uint, for GPU picking)
            .create_resource(GraphResourceDesc {
                name: "object_id".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0, 0.0, 0.0, 0.0]),
                },
                format: TextureImageFormat::R32Uint,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            // Create stencil indicator texture (R8, for wallhack overlay).
            // Written by the stencil indicator pass after the outline pass.
            // Sampled by the tonemap shader to apply orange tint over occluded selected objects.
            .create_resource(GraphResourceDesc {
                name: "stencil_indicator".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0, 0.0, 0.0, 0.0]),
                },
                format: TextureImageFormat::R8Unorm,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            // Note: Particle compute passes (emit and simulate) are executed automatically
            // by the render graph before any graphics passes. They don't need to be added here.
            // Sky pass: renders procedural sky (depth=1.0 so geometry appears in front)
            .add_pass(
                FullscreenPass::new("sky")
                    .write("hdr_color", TextureImageFormat::R16G16B16A16Sfloat)
                    .pipeline(sky_pipeline),
            )
            // Shadow pass: renders depth from light's perspective into 2x2 cascade atlas
            .add_pass(
                ShadowPass::new("shadow")
                    .write_depth("shadow_atlas", TextureImageFormat::D32Sfloat)
                    .resolution(4096, 4096),
            )
            // Depth prepass: renders scene depth from camera's perspective.
            // Also outputs object IDs to a R32Uint texture for GPU-based entity picking.
            // Populates the depth buffer before the geometry pass for early-Z rejection.
            .add_pass(DepthPrepass::new("depth_prepass").write_object_id("object_id"))
            // Geometry pass: renders scene to HDR color texture
            // Loads existing contents (sky pass) and writes geometry on top
            // Reuses depth from the depth prepass (LoadOp::Load)
            .add_pass(
                GeometryPass::new("geometry")
                    .write_color_with(
                        "hdr_color",
                        TextureImageFormat::R16G16B16A16Sfloat,
                        LoadOp::Load,
                        StoreOp::Store,
                        ClearValue::OPAQUE_BLACK,
                    )
                    .depth_config(
                        LoadOp::Load,
                        StoreOp::Store,
                        ClearValue::DepthStencil {
                            depth: 0.0,
                            stencil: 0,
                        },
                    )
                    .material(geometry_material)
                    .read("shadow_atlas"),
            )
            // Particle pass: renders GPU-simulated particles with alpha blending.
            // Reads/writes hdr_color (loads existing geometry, composites particles).
            // Depth testing reuses scene depth from the depth prepass.
            .add_pass(
                katla_gfx::ParticlePass::new("particles")
                    .write_color("hdr_color", TextureImageFormat::R16G16B16A16Sfloat),
            )
            // Outline pass: stencil-based selection highlight for editor.
            // Renders after geometry so the outline is drawn on top of the scene.
            // Only draws when entities are selected (filtered draw list).
            .add_pass(
                OutlinePass::new("outline")
                    .write_color("hdr_color", TextureImageFormat::R16G16B16A16Sfloat),
            )
            // Stencil indicator pass: writes R8 mask where selected objects are occluded.
            // Sampled by the wallhack overlay pass to apply tint over occluded selected objects.
            .add_pass(
                StencilIndicatorPass::new("stencil_indicator")
                    .write_color("stencil_indicator", TextureImageFormat::R8Unorm),
            )
            // Tonemap pass: samples HDR color and outputs to viewport texture
            // The viewport texture is then sampled by the UI system to display in the viewport panel
            .add_pass(
                FullscreenPass::new("tonemap")
                    .read("hdr_color")
                    .write("viewport_0", TextureImageFormat::B8G8R8A8Srgb)
                    .pipeline(tonemap_pipeline)
                    .tonemap(tonemap_params),
            )
            // Wallhack overlay pass: reads LDR viewport and stencil indicator mask,
            // applies orange tint where selected objects are occluded.
            .add_pass(
                katla_gfx::OverlayPass::new("wallhack_overlay")
                    .read("viewport_0")
                    .read("stencil_indicator")
                    .write("viewport_0", TextureImageFormat::B8G8R8A8Srgb)
                    .pipeline(overlay_pipeline)
                    .overlay(katla_gfx::OverlayParams {
                        ldr_texture_index: None,       // Set during app init
                        stencil_indicator_index: None, // Set during app init
                    }),
            )
            // Background pass: fills the backbuffer with a solid background color
            // This provides a dark background for the editor UI panels
            .add_pass(
                FullscreenPass::new("background")
                    .write_backbuffer()
                    .pipeline(tonemap_pipeline) // Reuse tonemap pipeline (outputs solid color with no HDR input)
                    .tonemap(katla_gfx::TonemapParams {
                        exposure: 1.0,
                        gamma: 1.0,
                        mode: katla_gfx::TonemapOperator::Aces,
                        hdr_texture_index: None,
                    }),
            )
            // UI pass: draws editor UI to backbuffer
            // Note: UI composites on top of the background pass
            // Declares viewport_0 as a read dependency so the render graph inserts
            // a layout transition (COLOR_ATTACHMENT -> SHADER_READ_ONLY) before
            // the UI shader samples from it via the bindless system.
            .add_pass(
                UIPass::new("ui")
                    .write("backbuffer")
                    .read("viewport_0")
                    .material(ui_material),
            )
            .build()
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        Ok(FrameGraph::from_vulkan(graph))
    }

    pub fn build(self) -> AppResult<(Application, EventLoop<()>)> {
        let event_loop = Self::build_event_loop();

        // Install console logger early so all subsequent log messages are captured.
        // Wraps env_logger as secondary so stderr output is preserved.
        #[cfg(feature = "editor")]
        let log_buffer = {
            use crate::ui::console::ConsoleLoggerHandle;
            let console_handle = ConsoleLoggerHandle::init(
                log::LevelFilter::Debug,
                Box::new(
                    env_logger::Builder::from_env(
                        env_logger::Env::default().default_filter_or("info"),
                    )
                    .build(),
                ),
            );
            let buffer = console_handle.buffer();
            log::set_boxed_logger(console_handle.into_logger())
                .expect("Failed to set console logger");
            log::set_max_level(log::LevelFilter::Debug);
            buffer
        };
        #[cfg(not(feature = "editor"))]
        let _ = (); // no console logger without editor

        // Load user preferences and editor state before moving fields
        let preferences = Preferences::load();
        #[cfg(feature = "editor")]
        let (theme, gui_state) = Self::load_editor_state_static(&preferences);

        let info = ApplicationInfo {
            name: self.app_name,
            validation_mode: self.validation_mode,
            max_frames: self.max_frames,
            check_black_frames: self.check_black_frames,
            scene_path: self.scene_path,
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

        // Create GLTF cache with loader that panics on error (same as old From<PathBuf> impl)
        let gltf_loader = Box::new(|path: &std::path::PathBuf| {
            GLTFModel::new(path)
                .unwrap_or_else(|e| panic!("Failed to load GLTF model from {:?}: {}", path, e))
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
            .unwrap();

        let mut renderer = Self::init_renderer(&event_loop, &window, &info, &resources);

        // Upload initial font atlas texture to GPU
        let (font_atlas_handle, atlas_width, atlas_height) = {
            let fonts = ui_context.fonts();
            let (atlas_width, atlas_height) = fonts.atlas_size();
            let atlas_data = fonts.atlas_data();
            (
                renderer.create_ui_font_atlas(atlas_width, atlas_height, atlas_data),
                atlas_width,
                atlas_height,
            )
        };

        log::info!(
            "Uploaded font atlas texture: {}x{}, handle={:?}, handle_index={}",
            atlas_width,
            atlas_height,
            font_atlas_handle,
            font_atlas_handle.index()
        );

        // Build the frame graph once at startup (needs mutable renderer to compile shader)
        let mut frame_graph = match &mut renderer {
            katla_gfx::AnyRenderer::Vulkan(r) => Self::build_frame_graph(r, &resources)?,
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(r) => Self::build_metal_frame_graph(r)?,
        };

        let pass_ids = super::PassIds {
            depth_prepass: frame_graph
                .pass_id("depth_prepass")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            geometry: frame_graph
                .pass_id("geometry")
                .expect("Frame graph must contain a 'geometry' pass"),
            shadow: frame_graph
                .pass_id("shadow")
                .expect("Frame graph must contain a 'shadow' pass"),
            outline: frame_graph
                .pass_id("outline")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            stencil_indicator: frame_graph
                .pass_id("stencil_indicator")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            ui: frame_graph
                .pass_id("ui")
                .expect("Frame graph must contain a 'ui' pass"),
            tonemap: frame_graph
                .pass_id("tonemap")
                .expect("Frame graph must contain a 'tonemap' pass"),
            wallhack_overlay: frame_graph
                .pass_id("wallhack_overlay")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
        };

        // Initialize transient textures and wire backend-specific resources
        frame_graph
            .initialize_transient_textures(&mut renderer)
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        match &mut renderer {
            katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) => {
                for frame_idx in 0..2 {
                    if let Some(view) = frame_graph
                        .as_vulkan()
                        .transient_texture_view_for_frame("shadow_atlas", frame_idx)
                    {
                        vulkan_renderer.set_shadow_atlas_view(frame_idx, view);
                    }
                }
                log::info!("Shadow atlas views set for all frames");
            }
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => {
                use katla_gfx::RenderGraphBackend;

                let hdr_slot = frame_graph
                    .register_transient_texture_bindless(&mut renderer, "hdr_color")
                    .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
                log::info!("HDR texture registered with bindless at index {}", hdr_slot);

                let vp_slot = frame_graph
                    .register_transient_texture_bindless(&mut renderer, "viewport_0")
                    .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
                log::info!(
                    "Viewport texture registered with bindless at index {}",
                    vp_slot
                );

                let frame_idx = GpuRenderer::current_frame(&renderer);
                if let Some(view) = frame_graph.transient_image_view_metal("hdr_color", frame_idx) {
                    let hdr_transient_slot = frame_graph
                        .transient_texture_metal("hdr_color", frame_idx)
                        .and_then(|t| t.bindless_slot)
                        .unwrap_or(hdr_slot);
                    renderer.set_geometry_hdr_view(view, hdr_transient_slot);
                }

                if let Some(view) = frame_graph.transient_image_view_metal("viewport_0", 0) {
                    renderer.set_tonemap_output_view(view);
                }

                renderer.set_viewport_bindless_slot(vp_slot);
            }
        }

        // Initialize UI renderer with font atlas bindless slot
        #[cfg(feature = "editor")]
        let mut ui_renderer = crate::ui::UIRenderer::new();
        #[cfg(feature = "editor")]
        match &mut renderer {
            katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) => {
                match vulkan_renderer.ui_renderer.font_atlas_bindless_slot() {
                    Some(bindless_slot) => {
                        ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
                        log::info!("Font atlas bindless slot initialized: {}", bindless_slot);
                    }
                    None => {
                        log::error!(
                            "Font atlas bindless slot is None! Text will render as solid colors."
                        );
                    }
                }
            }
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => {
                if let Some(font_handle) = renderer.ui_font_atlas_handle() {
                    if let Some(bindless_slot) = renderer.get_bindless_slot(font_handle) {
                        ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
                        log::info!("Font atlas bindless slot initialized: {}", bindless_slot);
                    }
                }
            }
        }

        world.insert_resource(crate::input::InputState::new());
        world.insert_resource(katla_script::ScriptsActive(false));
        world.insert_resource(katla_script::PendingAudioCommands::default());
        world.insert_resource(katla_script::PendingRaycastCommands::default());
        world.insert_resource(katla_script::PendingRaycastResults::default());
        world.insert_resource(katla_physics::PhysicsWorld::new());

        let app = Application {
            window,
            renderer,
            frame_graph,
            pass_ids,
            camera,
            gltf_cache: GltfCache::new(gltf_loader),
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
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
            gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker::new(
                katla_gfx::MaterialHandle::NONE,
            ),
            point_lights_buffer: Vec::new(),
            on_init: self.on_init,
            on_update: self.on_update,
            on_shutdown: self.on_shutdown,
            #[cfg(feature = "editor")]
            play_mode: super::game_state::PlayMode::Editing,
            #[cfg(feature = "editor")]
            scene_snapshot: None,
        };

        Ok((app, event_loop))
    }

    /// Build, initialize, and run the application in one call.
    ///
    /// Equivalent to `build()`, `init()`, `on_init` callback, and `event_loop.run_app()`.
    /// Returns on error during build; panics if the event loop itself fails.
    pub fn run(self) -> AppResult<()> {
        let (mut application, event_loop) = self.build()?;
        application.init();

        // Run the on_init hook after initialization, before the event loop
        if let Some(hook) = application.on_init.take() {
            hook(&mut application);
        }

        event_loop.run_app(&mut application).unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use katla_ecs::World;

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
}
