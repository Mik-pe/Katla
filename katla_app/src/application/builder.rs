use std::ffi::CString;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};

use katla_ecs::{System, SystemExecutionOrder, World};
use katla_gfx::renderer::VulkanRenderer;
use katla_ui::{FontId, ForkAwesome};
use winit::dpi::LogicalSize;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::Window;

use crate::{
    application::{Application, ApplicationInfo},
    entities::Camera,
    error::AppResult,
    gui_state::GuiState,
    input::InputMapper,
    preferences::Preferences,
    resources::ResourceManager,
    ui::Theme,
    util::{BackgroundLoader, FileCache, GLTFModel, Timer},
};

/// Default font sizes for UI text (in pixels)
const DEFAULT_UI_FONT_SIZES: &[f32] = &[14.0, 16.0];

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_mode: katla_gfx::ValidationMode,
    max_frames: Option<usize>,
    check_black_frames: bool,
    world: World,
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
        self.validation_mode = if on {
            katla_gfx::ValidationMode::GpuAssisted
        } else {
            katla_gfx::ValidationMode::Disabled
        };
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

    fn build_event_loop() -> EventLoop<()> {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
    }

    /// Initialize the Vulkan renderer and load materials.
    fn init_renderer(
        event_loop: &EventLoop<()>,
        window: &Window,
        info: &ApplicationInfo,
        _resources: &ResourceManager,
    ) -> VulkanRenderer {
        let engine_name = CString::new("Katla Engine").unwrap();
        let mut renderer = VulkanRenderer::init(
            event_loop,
            window,
            info.validation_mode,
            CString::new(info.name.as_str()).unwrap(),
            engine_name,
        )
        .expect("Failed to initialize Vulkan renderer");

        // Initialize particle system
        renderer
            .init_particle_system()
            .expect("Failed to initialize particle system");

        renderer
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
        renderer: &mut VulkanRenderer,
        resources: &ResourceManager,
    ) -> AppResult<katla_gfx::FrameGraph> {
        use katla_gfx::render_graph::UIPass;
        use katla_gfx::render_graph::{
            ComputePass, FullscreenPass, GeometryPass, GraphResourceDesc, GraphResourceType,
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
            .map_err(|e| crate::error::AppError::Graphics {
                message: format!("Failed to compile sky shader: {}", e),
            })?;

        // Compile tonemap shader for post-processing
        let tonemap_shader_path = resources.shader_path("tonemapping.wgsl");
        let tonemap_pipeline = renderer
            .compile_fullscreen_shader(tonemap_shader_path)
            .map_err(|e| crate::error::AppError::Graphics {
                message: format!("Failed to compile tonemap shader: {}", e),
            })?;

        // We'll get the HDR texture index after registering with bindless
        // For now, use None - it will be set during app init
        let tonemap_params = katla_gfx::TonemapParams {
            exposure: 0.4,
            gamma: 2.2,
            mode: katla_gfx::TonemapOperator::Aces,
            hdr_texture_index: None, // Will be set after registration
        };

        // Compile UI shader for editor UI rendering
        let ui_shader_path = resources.shader_path("ui/ui.wgsl");
        let ui_material = renderer
            .compile_material(
                ui_shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Ui,
                    alpha_blended: true,
                    ..Default::default()
                },
            )
            .map_err(|e| crate::error::AppError::Graphics {
                message: format!("Failed to compile UI shader: {}", e),
            })?;

        // Compile geometry shader for PBR model rendering
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
            .map_err(|e| crate::error::AppError::Graphics {
                message: format!("Failed to compile geometry shader: {}", e),
            })?;

        // Compile particle rendering shader with alpha blending
        let particle_shader_path = resources.shader_path("particles/particle_render.wgsl");

        // Initialize particle render pipeline using the renderer's method
        renderer
            .init_particle_render_pipeline(&particle_shader_path)
            .map_err(|e| crate::error::AppError::Graphics {
                message: format!("Failed to initialize particle render pipeline: {}", e),
            })?;

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
            })
            // Note: Particle compute passes (emit and simulate) are executed automatically
            // by the render graph before any graphics passes. They don't need to be added here.
            // Sky pass: renders procedural sky (depth=1.0 so geometry appears in front)
            .add_pass(
                FullscreenPass::new("sky")
                    .write("hdr_color", TextureImageFormat::R16G16B16A16Sfloat)
                    .pipeline(sky_pipeline),
            )
            // Geometry pass: renders scene to HDR color texture
            // Loads existing contents (sky pass) and writes geometry on top
            // Note: Depth is implicit and uses the global depth buffer
            .add_pass(
                GeometryPass::new("geometry")
                    .write_color_with(
                        "hdr_color",
                        TextureImageFormat::R16G16B16A16Sfloat,
                        LoadOp::Load,
                        StoreOp::Store,
                        ClearValue::OPAQUE_BLACK,
                    )
                    .material(geometry_material),
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
                        hdr_texture_index: None, // No HDR texture, will output black
                    }),
            )
            // UI pass: draws editor UI to backbuffer
            // Note: UI composites on top of the background pass
            .add_pass(UIPass::new("ui").write("backbuffer").material(ui_material))
            .build()
            .map_err(|e| crate::error::AppError::Graphics {
                message: e.to_string(),
            })?;

        Ok(graph)
    }

    pub fn build(self) -> AppResult<(Application, EventLoop<()>)> {
        let event_loop = Self::build_event_loop();

        let info = ApplicationInfo {
            name: self.app_name,
            validation_mode: self.validation_mode,
            max_frames: self.max_frames,
            check_black_frames: self.check_black_frames,
        };
        let mut world = self.world;
        let camera = Rc::new(RefCell::new(Camera::new(&mut world)));

        let resources = ResourceManager::discover()?;

        // Load user preferences
        let preferences = Preferences::load();
        let theme = Theme::by_name(&preferences.theme).unwrap_or_default();
        log::info!(
            "Loaded preferences: theme={}, show_grid={}, show_stats={}, font_scale={}",
            preferences.theme,
            preferences.show_grid,
            preferences.show_stats,
            preferences.font_scale
        );

        // Load GUI layout state
        let gui_state = GuiState::load();
        log::info!(
            "Loaded GUI state: left_panel={}, right_panel={}, asset_browser_height={}",
            gui_state.left_panel_width,
            gui_state.right_panel_width,
            gui_state.asset_browser_height
        );

        // Create UI context and load default font
        let mut ui_context = katla_ui::UiContext::new();

        // Load default font for text rendering
        let font_path = resources.font_path("roboto-regular.ttf");
        if font_path.exists() {
            match std::fs::read(&font_path) {
                Ok(font_bytes) => {
                    match ui_context.fonts.add_font(&font_bytes) {
                        Ok(font_id) => {
                            // Precache common ASCII characters at typical UI sizes
                            // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                            // actual DPI scale on first use if different
                            for &size in DEFAULT_UI_FONT_SIZES {
                                ui_context.fonts.precache_ascii(font_id, size, 1.0);
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
                    match ui_context.fonts.add_font_with_id(&font_bytes, FontId::ICON) {
                        Ok(()) => {
                            // Precache common icons at typical UI sizes
                            // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                            // actual DPI scale on first use if different
                            for &size in DEFAULT_UI_FONT_SIZES {
                                ui_context.fonts.precache_icons(
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
        let (atlas_width, atlas_height) = ui_context.fonts.atlas_size();
        let atlas_data = ui_context.fonts.atlas_data();
        let font_atlas_handle =
            renderer.create_ui_font_atlas(atlas_width, atlas_height, atlas_data);

        ui_context
            .fonts
            .set_atlas_id(katla_ui::TextureId::FONT_ATLAS);
        log::info!(
            "Uploaded font atlas texture: {}x{}, handle={:?}, handle_index={}",
            atlas_width,
            atlas_height,
            font_atlas_handle,
            font_atlas_handle.index()
        );

        // Build the frame graph once at startup (needs mutable renderer to compile shader)
        let frame_graph = Self::build_frame_graph(&mut renderer, &resources)?;

        // Initialize UI renderer with font atlas bindless slot
        let mut ui_renderer = crate::ui::UIRenderer::new();
        match renderer.ui_renderer.font_atlas_bindless_slot() {
            Some(bindless_slot) => {
                ui_renderer.set_font_atlas_bindless_slot(bindless_slot);
                log::info!("Font atlas bindless slot initialized: {}", bindless_slot);
            }
            None => {
                log::error!("Font atlas bindless slot is None! Text will render as solid colors.");
            }
        }

        let app = Application {
            window,
            renderer,
            frame_graph,
            camera,
            gltf_cache: FileCache::new(gltf_loader),
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
            resources,
            ui_context,
            ui_renderer,
            debug_overlay: crate::ui::DebugOverlay::new(),
            editor_ui: {
                let mut editor = crate::ui::EditorUI::with_theme(theme);
                editor.show_grid = preferences.show_grid;
                editor.show_stats = preferences.show_stats;
                editor.set_font_scale(preferences.font_scale);
                // Apply GUI layout state
                editor.left_panel_width = gui_state.left_panel_width;
                editor.right_panel_width = gui_state.right_panel_width;
                editor.asset_browser.panel_height = gui_state.asset_browser_height;
                editor
            },
            use_editor_ui: true, // Default to editor UI mode
            preferences,
            gui_state,
            scale_factor: 1.0, // Will be updated when window is created
            background_loader: BackgroundLoader::new(),
            thumbnail_texture_handles: HashMap::new(),
            start_time: Instant::now(),
            default_material_handle: katla_gfx::MaterialHandle::NONE,
            pending_readback: None,
            cleaned_up: false,
            particle_system: crate::systems::ParticleSystem::new(),
        };

        Ok((app, event_loop))
    }
}
