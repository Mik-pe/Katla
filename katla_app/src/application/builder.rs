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

use katla_ecs::{System, SystemExecutionOrder, World};
use katla_ui::{FontId, ForkAwesome};
use winit::dpi::LogicalSize;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::Window;

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
    dump_layout_path: Option<super::DumpLayoutTarget>,
    headless: bool,
    screenshot_path: Option<String>,
    ui_test_path: Option<String>,
    on_init: Option<InitHook>,
    on_update: Option<UpdateHook>,
    on_shutdown: Option<ShutdownHook>,
    frame_graph_factory: Option<FrameGraphFactory>,
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
                    info.validation_mode,
                    app_name,
                    engine_name,
                )
                .map_err(|e| crate::error::AppError::Graphics { source: e })?
            }
        };

        Ok(renderer)
    }

    /// Build the semantic frame graph used by the Metal backend.
    ///
    /// Metal owns its encoder implementations, but pass presence and order are
    /// validated against this compiled graph before command encoding starts.
    #[cfg(target_os = "macos")]
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
                name: "object_id".to_string(),
                resource_type: GraphResourceType::ColorAttachment {
                    clear_value: Some([0.0, 0.0, 0.0, 0.0]),
                },
                format: ImageFormat::R32Uint,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            .export_resource("object_id")
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
            .add_side_effect_pass(
                SimplePass::new("shadow", PassType::Graphics).with_kind(PassKind::Shadow),
            )
            .add_side_effect_pass(
                SimplePass::new("depth_prepass", PassType::Graphics)
                    .with_kind(PassKind::DepthPrepass),
            )
            .add_pass(
                SimplePass::new("geometry", PassType::Graphics)
                    .write("hdr_color")
                    .with_kind(PassKind::Geometry),
            )
            .add_pass(
                SimplePass::new("outline", PassType::Graphics)
                    .read("hdr_color")
                    .write("hdr_color")
                    .with_kind(PassKind::Outline),
            )
            .add_pass(
                SimplePass::new("object_id", PassType::Graphics)
                    .write("object_id")
                    .with_kind(PassKind::ObjectId),
            )
            .add_pass(
                SimplePass::new("tonemap", PassType::Graphics)
                    .read("hdr_color")
                    .write("viewport_0")
                    .with_kind(PassKind::Fullscreen),
            )
            .add_pass(
                SimplePass::new("ui", PassType::Graphics)
                    .read("viewport_0")
                    .write("backbuffer")
                    .with_kind(PassKind::Ui),
            )
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
            .init_pass_pipeline(katla_gfx::PipelineKind::Shadow, &[&shadow_shader_path])
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let shadow_skinned_shader_path = resources.shader_path("shadow/shadow_depth_skinned.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::ShadowSkinned,
                &[&shadow_skinned_shader_path],
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let depth_prepass_shader_path = resources.shader_path("depth_prepass.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::DepthPrepass,
                &[&depth_prepass_shader_path],
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let depth_prepass_skinned_shader_path = resources.shader_path("depth_prepass_skinned.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::DepthPrepassSkinned,
                &[&depth_prepass_skinned_shader_path],
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let billboard_depth_shader_path = resources.shader_path("billboard_depth.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::DepthPrepassBillboard,
                &[&billboard_depth_shader_path],
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize outline pipelines for stencil-based selection highlight
        let stencil_mark_shader_path = resources.shader_path("outline/stencil_mark.wgsl");
        let stencil_mark_skinned_shader_path =
            resources.shader_path("outline/stencil_mark_skinned.wgsl");
        let outline_draw_shader_path = resources.shader_path("outline/outline_draw.wgsl");
        let outline_draw_skinned_shader_path =
            resources.shader_path("outline/outline_draw_skinned.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::Outline,
                &[
                    &stencil_mark_shader_path,
                    &stencil_mark_skinned_shader_path,
                    &outline_draw_shader_path,
                    &outline_draw_skinned_shader_path,
                ],
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Initialize stencil indicator pipeline for wallhack overlay
        let stencil_indicator_shader_path = resources.shader_path("outline/stencil_indicator.wgsl");
        let stencil_indicator_skinned_shader_path =
            resources.shader_path("outline/stencil_indicator_skinned.wgsl");
        renderer
            .init_pass_pipeline(
                katla_gfx::PipelineKind::StencilIndicator,
                &[
                    &stencil_indicator_shader_path,
                    &stencil_indicator_skinned_shader_path,
                ],
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
            // Picking readback is externally observable even though it is not presented.
            .export_resource("object_id")
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
        renderer: &mut Renderer,
    ) -> AppResult<(
        FrameGraph,
        super::PassIds,
        super::frame_graph_config::FrameGraphBindings,
        FrameGraphRuntime,
    )> {
        let (mut frame_graph, bindings, runtime) = configured.into_parts();
        bindings.validate_resources(&frame_graph)?;
        let pass_ids = super::PassIds::resolve(&frame_graph, &bindings.passes)?;

        frame_graph
            .initialize_transient_textures(renderer)
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        match renderer {
            katla_gfx::AnyRenderer::Vulkan(vulkan_renderer) => {
                if let Some(shadow_atlas) = bindings.resources.shadow_atlas.as_deref() {
                    for frame_idx in 0..2 {
                        if let Some(view) = frame_graph
                            .as_vulkan()
                            .transient_texture_view_for_frame(shadow_atlas, frame_idx)
                        {
                            vulkan_renderer.set_shadow_atlas_view(frame_idx, view);
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => {
                if let Some(hdr_color) = bindings.resources.hdr_color.as_deref() {
                    let hdr_slot = frame_graph
                        .register_transient_texture_bindless(renderer, hdr_color)
                        .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
                    let frame_idx = GpuRenderer::current_frame(renderer);
                    if let Some(view) = frame_graph.transient_image_view_metal(hdr_color, frame_idx)
                    {
                        let transient_slot = frame_graph
                            .transient_texture_metal(hdr_color, frame_idx)
                            .and_then(|texture| texture.bindless_slot)
                            .unwrap_or(hdr_slot);
                        renderer.set_geometry_hdr_view(view, transient_slot);
                    }
                }

                if let Some(viewport) = bindings.resources.viewport.as_deref() {
                    let viewport_slot = frame_graph
                        .register_transient_texture_bindless(renderer, viewport)
                        .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
                    if let Some(view) = frame_graph.transient_image_view_metal(viewport, 0) {
                        renderer.set_tonemap_output_view(view);
                    }
                    renderer.set_viewport_bindless_slot(viewport_slot);
                }
            }
        }

        Ok((frame_graph, pass_ids, bindings, runtime))
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
    #[cfg(target_os = "macos")]
    pub fn build_headless(
        mut self,
        max_frames: usize,
        screenshot_path: String,
    ) -> AppResult<Application> {
        // Install logger
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
            screenshot_path: Some(screenshot_path),
            headless: true,
            ui_test_path: self.ui_test_path.clone(),
        };

        let mut world = self.world;
        let camera = Camera::new(&mut world);
        let resources = ResourceManager::discover()?;

        // Create UI context and load fonts
        let mut ui_context = katla_ui::UiContext::new();
        let scale_factor = crate::application::headless::HEADLESS_SCALE_FACTOR;

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

        // Create headless Metal renderer
        let engine_name =
            CString::new("Katla Engine").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;
        let app_name =
            CString::new("Katla Headless").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;

        let mut renderer = Renderer::new_metal_headless(
            crate::application::headless::HEADLESS_WIDTH,
            crate::application::headless::HEADLESS_HEIGHT,
            self.validation_mode,
            app_name,
            engine_name,
        )
        .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Upload font atlas
        let (font_atlas_handle, atlas_width, atlas_height) = {
            let fonts = ui_context.fonts();
            let (w, h) = fonts.atlas_size();
            let data = fonts.atlas_data_rgba();
            (renderer.create_ui_font_atlas(w, h, &data), w, h)
        };
        log::info!(
            "Uploaded font atlas: {}x{}, handle={:?}",
            atlas_width,
            atlas_height,
            font_atlas_handle
        );

        let configured_frame_graph =
            Self::build_selected_frame_graph(frame_graph_factory, &mut renderer, &resources)?;
        let (frame_graph, pass_ids, frame_graph_bindings, frame_graph_runtime) =
            Self::prepare_frame_graph(configured_frame_graph, &mut renderer)?;

        // Initialize UI renderer for Metal
        #[cfg(feature = "editor")]
        let mut ui_renderer = crate::ui::UIRenderer::new();
        #[cfg(feature = "editor")]
        if let Some(font_handle) = renderer.ui_font_atlas_handle()
            && let Some(bindless_slot) = renderer.get_bindless_slot(font_handle)
        {
            ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
            log::info!("Font atlas bindless slot initialized: {}", bindless_slot);
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
            point_lights_buffer: Vec::new(),
            on_init: self.on_init,
            on_update: self.on_update,
            on_shutdown: self.on_shutdown,
            #[cfg(feature = "editor")]
            play_mode: super::game_state::PlayMode::Editing,
            #[cfg(feature = "editor")]
            scene_snapshot: None,
            #[cfg(feature = "editor")]
            asset_watcher: None,
            layout_dumped: false,
        };

        Ok(app)
    }

    pub fn build(mut self) -> AppResult<(Application, EventLoop<()>)> {
        let event_loop = Self::build_event_loop()?;

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
            log::set_boxed_logger(console_handle.into_logger()).map_err(|e| {
                crate::error::AppError::Other {
                    message: e.to_string(),
                }
            })?;
            log::set_max_level(log::LevelFilter::Info);
            buffer
        };
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
            screenshot_path: None,
            headless: false,
            ui_test_path: None,
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

        // Upload initial font atlas texture to GPU
        let (font_atlas_handle, atlas_width, atlas_height) = {
            let fonts = ui_context.fonts();
            let (atlas_width, atlas_height) = fonts.atlas_size();
            let atlas_data = fonts.atlas_data_rgba();
            (
                renderer.create_ui_font_atlas(atlas_width, atlas_height, &atlas_data),
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

        let configured_frame_graph =
            Self::build_selected_frame_graph(frame_graph_factory, &mut renderer, &resources)?;
        let (frame_graph, pass_ids, frame_graph_bindings, frame_graph_runtime) =
            Self::prepare_frame_graph(configured_frame_graph, &mut renderer)?;

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
                if let Some(font_handle) = renderer.ui_font_atlas_handle()
                    && let Some(bindless_slot) = renderer.get_bindless_slot(font_handle)
                {
                    ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
                    log::info!("Font atlas bindless slot initialized: {}", bindless_slot);
                }
            }
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
            panel_rt_size: katla_gfx::Size2D::new(0, 0),
            gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker::new(
                katla_gfx::MaterialHandle::NONE,
            ),
            geometry_cache: crate::geometry_cache::GeometryCache::default(),
            point_lights_buffer: Vec::new(),
            on_init: self.on_init,
            on_update: self.on_update,
            on_shutdown: self.on_shutdown,
            #[cfg(feature = "editor")]
            play_mode: super::game_state::PlayMode::Editing,
            #[cfg(feature = "editor")]
            scene_snapshot: None,
            #[cfg(feature = "editor")]
            asset_watcher: Self::create_asset_watcher(),
            layout_dumped: false,
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

impl KatlaEditorFrameGraphPreset {
    /// Build Katla's explicit scene + editor graph preset for the active backend.
    pub fn build(
        renderer: &mut Renderer,
        resources: &ResourceManager,
    ) -> AppResult<ApplicationFrameGraph> {
        // Heavy scene subsystems belong to this explicit preset, not renderer
        // construction. A graph-only application therefore pays no particle
        // buffer allocation or simulation setup cost.
        renderer
            .init_particle_system()
            .map_err(|source| crate::error::AppError::Graphics { source })?;

        let (graph, bindings) = match renderer {
            katla_gfx::AnyRenderer::Vulkan(renderer) => (
                ApplicationBuilder::build_frame_graph(renderer, resources)?,
                super::frame_graph_config::FrameGraphBindings::katla_editor(),
            ),
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(renderer) => (
                ApplicationBuilder::build_metal_frame_graph(renderer)?,
                super::frame_graph_config::FrameGraphBindings::katla_editor_metal(),
            ),
        };

        Ok(ApplicationFrameGraph::new(graph)
            .with_bindings(bindings)
            .with_runtime(FrameGraphRuntime::KatlaScene))
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
