//! Headless rendering — render frames offscreen without a window or display.
//!
//! Creates an offscreen Metal texture at 1280×720, runs the frame loop N times,
//! reads back pixels, and saves as PNG. Used for automated visual debugging
//! and feeding screenshots to vision models.

use std::ffi::CString;

use katla_ecs::World;
use katla_gfx::GpuRenderer;
use log::{error, info};

use crate::FrameGraph;
use crate::Renderer;
use crate::application::PassIds;
use crate::application::camera::Camera;
use crate::error::AppResult;
use crate::preferences::Preferences;
use crate::rendering::FrameContext;
use crate::resources::ResourceManager;
use crate::util::Timer;

const HEADLESS_WIDTH: u32 = 1280;
const HEADLESS_HEIGHT: u32 = 720;
const HEADLESS_FONT_SIZES: &[f32] = &[14.0, 16.0];

/// Opaque handle to an offscreen texture.
#[cfg(target_os = "macos")]
pub struct OffscreenTexture(pub(crate) katla_gfx::MetalTextureRetained);
/// Headless application for offscreen rendering and screenshot capture.
///
/// Runs without a window or winit event loop. Creates an offscreen Metal
/// texture, renders N frames, then reads back pixels and saves as PNG.
pub struct HeadlessApplication {
    renderer: Renderer,
    frame_graph: FrameGraph,
    pass_ids: PassIds,
    camera: Camera,
    world: World,
    resources: ResourceManager,
    #[expect(dead_code)]
    ui_context: katla_ui::UiContext,
    timer: Timer,
    frame_count: usize,
    max_frames: usize,
    screenshot_path: String,
    default_material_handle: katla_gfx::MaterialHandle,
    #[expect(dead_code)]
    particle_system: crate::systems::ParticleSystem,
    #[cfg(feature = "editor")]
    dump_layout_path: Option<super::DumpLayoutTarget>,
    layout_dumped: bool,
    #[expect(dead_code)]
    preferences: Preferences,
    #[expect(dead_code)]
    gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker,
    on_update: Option<super::builder::UpdateHook>,
    /// Rendered offscreen texture from the last frame (kept for readback).
    offscreen_texture: Option<OffscreenTexture>,
}

impl HeadlessApplication {
    /// Build a headless application from builder state.
    pub(crate) fn build(
        mut world: World,
        dump_layout_path: Option<super::DumpLayoutTarget>,
        max_frames: usize,
        screenshot_path: String,
        _on_init: Option<super::builder::InitHook>,
        on_update: Option<super::builder::UpdateHook>,
    ) -> AppResult<Self> {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init()
            .ok();

        let resources = ResourceManager::discover()?;
        let preferences = Preferences::load();

        let engine_name =
            CString::new("Katla Engine").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;
        let app_name =
            CString::new("Katla Headless").map_err(|e| crate::error::AppError::Other {
                message: e.to_string(),
            })?;

        let mut renderer = Renderer::new_metal_headless(
            HEADLESS_WIDTH,
            HEADLESS_HEIGHT,
            katla_gfx::ValidationMode::Disabled,
            app_name,
            engine_name,
        )
        .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        renderer
            .init_particle_system()
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        // Create UI context and load default font
        let mut ui_context = katla_ui::UiContext::new();
        let font_path = resources.font_path("roboto-regular.ttf");
        if font_path.exists()
            && let Ok(font_bytes) = std::fs::read(&font_path)
        {
            let font_id = ui_context.fonts_mut().add_font(&font_bytes).ok();
            if let Some(font_id) = font_id {
                for &size in HEADLESS_FONT_SIZES {
                    ui_context.fonts_mut().precache_ascii(font_id, size, 1.0);
                }
                ui_context.set_font(font_id);
                info!("Loaded default font for headless");
            }
        }

        let icon_font_path = resources.font_path("forkawesome-webfont.ttf");
        if icon_font_path.exists()
            && let Ok(font_bytes) = std::fs::read(&icon_font_path)
        {
            let icon_font_result = ui_context
                .fonts_mut()
                .add_font_with_id(&font_bytes, katla_ui::FontId::ICON);
            match icon_font_result {
                Ok(()) => {
                    for &size in HEADLESS_FONT_SIZES {
                        ui_context.fonts_mut().precache_icons(
                            katla_ui::FontId::ICON,
                            size,
                            1.0,
                            katla_ui::ForkAwesome::common_icons(),
                        );
                    }
                }
                Err(e) => log::warn!("Failed to parse icon font: {}", e),
            }
        }

        let (font_atlas_handle, atlas_width, atlas_height) = {
            let fonts = ui_context.fonts();
            let (w, h) = fonts.atlas_size();
            let data = fonts.atlas_data_rgba();
            (renderer.create_ui_font_atlas(w, h, &data), w, h)
        };
        info!(
            "Uploaded font atlas: {}x{}, handle={:?}",
            atlas_width, atlas_height, font_atlas_handle
        );

        let camera = Camera::new(&mut world);

        // Build frame graph — extract the MetalRenderer from AnyRenderer
        let mut frame_graph = {
            let metal_renderer = match &mut renderer {
                katla_gfx::AnyRenderer::Metal(r) => r,
                _ => unreachable!(),
            };
            Self::build_metal_frame_graph(metal_renderer)?
        };

        frame_graph
            .initialize_transient_textures(&mut renderer)
            .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;

        // Wire up bindless slots
        {
            let hdr_slot = frame_graph
                .register_transient_texture_bindless(&mut renderer, "hdr_color")
                .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
            info!("HDR texture registered with bindless at index {}", hdr_slot);

            let vp_slot = frame_graph
                .register_transient_texture_bindless(&mut renderer, "viewport_0")
                .map_err(|e| crate::error::AppError::Graphics { source: e.into() })?;
            info!(
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

        let pass_ids = PassIds {
            depth_prepass: frame_graph
                .pass_id("depth_prepass")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            geometry: frame_graph
                .pass_id("geometry")
                .ok_or(crate::error::AppError::Other {
                    message: "Frame graph must contain a 'geometry' pass".to_string(),
                })?,
            shadow: frame_graph
                .pass_id("shadow")
                .ok_or(crate::error::AppError::Other {
                    message: "Frame graph must contain a 'shadow' pass".to_string(),
                })?,
            outline: frame_graph
                .pass_id("outline")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            stencil_indicator: frame_graph
                .pass_id("stencil_indicator")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
            ui: frame_graph
                .pass_id("ui")
                .ok_or(crate::error::AppError::Other {
                    message: "Frame graph must contain a 'ui' pass".to_string(),
                })?,
            tonemap: frame_graph
                .pass_id("tonemap")
                .ok_or(crate::error::AppError::Other {
                    message: "Frame graph must contain a 'tonemap' pass".to_string(),
                })?,
            wallhack_overlay: frame_graph
                .pass_id("wallhack_overlay")
                .unwrap_or(katla_gfx::render_graph::PassId(0)),
        };

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

        Ok(Self {
            renderer,
            frame_graph,
            pass_ids,
            camera,
            world,
            resources,
            ui_context,
            timer: Timer::new(100),
            frame_count: 0,
            max_frames,
            screenshot_path,
            default_material_handle: katla_gfx::MaterialHandle::NONE,
            particle_system: crate::systems::ParticleSystem::new(),
            #[cfg(feature = "editor")]
            dump_layout_path,
            layout_dumped: false,
            preferences,
            gpu_resource_tracker: crate::gpu_resource_tracker::GpuResourceTracker::new(
                katla_gfx::MaterialHandle::NONE,
            ),
            on_update,
            offscreen_texture: None,
        })
    }

    fn build_metal_frame_graph(renderer: &mut katla_gfx::MetalRenderer) -> AppResult<FrameGraph> {
        use katla_gfx::render_graph::{
            FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassType, SimplePass,
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

    /// Initialize renderer resources (shaders, pipelines, materials).
    pub fn init(&mut self) -> AppResult<()> {
        info!("HeadlessApplication::init() called");

        if let katla_gfx::AnyRenderer::Metal(_) = self.renderer {
            self.init_metal()?;
        }

        // Spawn default scene primitives
        self.spawn_default_scene();

        info!("HeadlessApplication::init() completed");
        Ok(())
    }

    fn init_metal(&mut self) -> AppResult<()> {
        let resources = &self.resources;

        let sky_shader_path = resources.shader_path("sky.wgsl");
        self.renderer
            .init_sky_pipeline(&sky_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let tonemap_shader_path = resources.shader_path("tonemapping.wgsl");
        self.renderer
            .init_tonemap_pipeline(&tonemap_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;

        let light_cull_shader_path = resources.shader_path("lighting/light_cull.wgsl");
        let extent = self.renderer.swapchain_extent();
        self.renderer
            .init_light_culling(extent.width, extent.height, &light_cull_shader_path)
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;
        self.renderer.upload_lights(&[]);

        let geometry_shader_path = resources.shader_path("model_pbr.wgsl");
        let default_material = self
            .renderer
            .compile_material(
                geometry_shader_path.to_str().unwrap_or("model_pbr.wgsl"),
                "pbr",
            )
            .map_err(|e| crate::error::AppError::Graphics { source: e })?;
        self.default_material_handle = default_material;
        self.renderer.set_default_material(default_material);

        info!("Headless Metal pipelines initialized");
        Ok(())
    }

    /// Spawn a basic scene for headless rendering.
    fn spawn_default_scene(&mut self) {
        use crate::components::{
            DirectionalLight, DrawableComponent, PointLight, TransformComponent,
        };
        use katla_gfx::primitives;
        use katla_math::{Color, Vec3};

        let default_mat = self.default_material_handle;

        // Ground plane
        let plane_mesh = primitives::create_plane(&mut self.renderer, 20.0, 20.0);
        self.world.spawn((
            TransformComponent::from_position(Vec3::new(0.0, -1.0, 0.0)),
            DrawableComponent::with_handles_and_material(
                plane_mesh,
                default_mat,
                Some(Color::new(0.157, 0.173, 0.204, 1.0).to_linear()),
                0.0,
                1.0,
                1.0,
            ),
        ));

        // Center cube
        let cube_mesh = primitives::create_cube(&mut self.renderer, [1.0, 1.0, 1.0]);
        self.world.spawn((
            TransformComponent::from_position(Vec3::new(-5.0, 0.0, -5.0)),
            DrawableComponent::with_handles_and_material(
                cube_mesh,
                default_mat,
                Some(Color::new(1.0, 0.471, 0.314, 1.0).to_linear()),
                0.0,
                0.5,
                1.0,
            ),
        ));

        // Sphere
        let sphere_mesh = primitives::create_sphere(&mut self.renderer, 0.7, 32, 16);
        self.world.spawn((
            TransformComponent::from_position(Vec3::new(-7.0, 0.0, -5.0)),
            DrawableComponent::with_handles_and_material(
                sphere_mesh,
                default_mat,
                Some(Color::new(0.314, 0.863, 1.0, 1.0).to_linear()),
                0.0,
                0.5,
                1.0,
            ),
        ));

        // Cylinder
        let cyl_mesh = primitives::create_cylinder(&mut self.renderer, 1.5, 0.5, 32);
        self.world.spawn((
            TransformComponent::from_position(Vec3::new(5.0, 0.0, -5.0)),
            DrawableComponent::with_handles_and_material(
                cyl_mesh,
                default_mat,
                Some(Color::new(1.0, 0.314, 0.784, 1.0).to_linear()),
                0.0,
                0.5,
                1.0,
            ),
        ));

        // PBR sphere grid (3x3)
        let grid_size = 3usize;
        let half = (grid_size - 1) as f32 / 2.0;
        for y in 0..grid_size {
            for x in 0..grid_size {
                let metallic = y as f32 / (grid_size - 1).max(1) as f32;
                let roughness = x as f32 / (grid_size - 1).max(1) as f32;
                let sphere = primitives::create_sphere(&mut self.renderer, 0.4, 32, 16);
                self.world.spawn((
                    TransformComponent::from_position(Vec3::new(
                        (x as f32 - half) * 1.2,
                        2.0 + (y as f32 - half) * 1.2,
                        -6.0,
                    )),
                    DrawableComponent::with_handles_and_material(
                        sphere,
                        default_mat,
                        Some(
                            Color::new(0.4 + metallic * 0.2, 0.6 + metallic * 0.2, 1.0, 1.0)
                                .to_linear(),
                        ),
                        metallic,
                        roughness,
                        1.0,
                    ),
                ));
            }
        }

        // Directional light
        self.world.spawn((
            TransformComponent::default(),
            DirectionalLight {
                direction: Vec3::new(0.3, 1.0, 0.2).normalize(),
                color: [1.0, 0.98, 0.95],
                intensity: 1.0,
            },
        ));

        // Point lights
        let lights: [([f32; 3], [f32; 3], f32, f32); 3] = [
            ([-5.0, 3.0, -3.0], [1.0, 0.6, 0.2], 15.0, 12.0),
            ([5.0, 2.5, -3.0], [1.0, 0.2, 0.8], 14.0, 10.0),
            ([0.0, 6.0, -3.0], [0.9, 0.85, 0.8], 8.0, 15.0),
        ];
        for (pos, color, intensity, range) in lights {
            self.world.spawn((
                TransformComponent::from_position(Vec3::new(pos[0], pos[1], pos[2])),
                PointLight {
                    color,
                    intensity,
                    range,
                },
            ));
        }

        info!(
            "Spawned default headless scene ({} entities)",
            self.world.entity_count()
        );
    }

    /// Run the headless frame loop and save the screenshot.
    pub fn run_headless(&mut self) -> AppResult<()> {
        info!(
            "Running {} headless frames at {}x{}",
            self.max_frames, HEADLESS_WIDTH, HEADLESS_HEIGHT
        );

        for _ in 0..self.max_frames {
            self.render_one_frame();
            self.frame_count += 1;
        }

        // Wait for GPU to finish the last frame
        if let Err(e) = self.renderer.wait_for_frame() {
            log::error!("Failed to wait for frame: {}", e);
        }

        self.save_screenshot()?;

        // Layout dump (if both --headless and --dump-layout are set)
        self.dump_layout_if_needed();

        // Cleanup
        self.frame_graph.cleanup();
        self.renderer.wait_for_device();
        self.renderer.destroy();

        info!("Headless render complete");
        Ok(())
    }

    fn render_one_frame(&mut self) {
        self.timer.add_timestamp();
        let dt = self.timer.get_delta() as f32;

        // ECS systems
        self.world.update_parallel(dt);

        // Clear per-frame input
        if let Some(input) = self.world.get_resource_mut::<crate::input::InputState>() {
            input.mouse_delta = (0.0, 0.0);
            input.mouse_wheel_delta = 0.0;
        }

        // Update hook
        if let Some(ref mut hook) = self.on_update {
            hook(&mut self.world, dt);
        }

        // 3D scene rendering (no editor UI in headless mode)
        self.render_scene();
    }

    fn render_scene(&mut self) {
        let viewport_aspect = HEADLESS_WIDTH as f32 / HEADLESS_HEIGHT as f32;
        self.camera
            .aspect_ratio_changed(&mut self.world, viewport_aspect);

        let mut frame = FrameContext::new();
        let view_mat = self.camera.get_view_mat(&self.world);
        let proj_mat = self.camera.get_proj_mat(&self.world);
        let frustum = katla_math::Frustum::from_proj_and_view(&proj_mat, &view_mat);
        let camera_entity = self.camera.entity;

        use crate::components::TransformComponent;
        let cam_pos = self
            .world
            .get_component::<TransformComponent>(camera_entity)
            .map(|t| {
                [
                    t.transform.position.x(),
                    t.transform.position.y(),
                    t.transform.position.z(),
                    1.0,
                ]
            })
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);

        let inv_view_proj = {
            use katla_math::Mat4;
            (proj_mat * view_mat)
                .inverse()
                .unwrap_or_else(Mat4::identity)
        };

        let extent = self.renderer.swapchain_extent();
        let tiles_x = extent.width.div_ceil(16);
        let tiles_y = extent.height.div_ceil(16);

        let frame_uniforms = katla_gfx::renderer::FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: [1.0, 0.0, 0.0, 0.0],
            tiles: [tiles_x, tiles_y, 0, 0],
            tonemap: [1.0, 2.2, 0.0, 0.0],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        };
        frame.set_frame_uniforms(frame_uniforms.clone());

        self.collect_draws(&mut frame, &frustum);
        self.collect_and_upload_lights();

        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());
        self.renderer.update_shadows([0.3, 1.0, 0.2]);

        let mut draw_list = frame.take_draw_list();
        draw_list.sort_by_material();

        if let Err(e) = self.renderer.execute_draw_calls(&draw_list) {
            log::error!("Failed to execute draw calls: {}", e);
            return;
        }

        // Create fresh offscreen texture and set as drawable for this frame
        // Create fresh offscreen texture and set as drawable for this frame.
        // Clone the Retained handle before passing to the renderer — the renderer
        // takes ownership via .take() during render_frame, but the Shared-storage
        // texture persists on the GPU. We keep a clone for readback.
        let offscreen = self
            .renderer
            .create_offscreen_texture(HEADLESS_WIDTH, HEADLESS_HEIGHT);
        let offscreen_clone = offscreen.clone();
        self.renderer.set_headless_drawable(offscreen);

        if let Err(e) = self.renderer.render(&mut self.frame_graph, |frame| {
            let ids = &self.pass_ids;

            if !draw_list.is_empty() {
                frame.submit(ids.geometry, &draw_list);
                frame.submit(ids.shadow, &draw_list);
                frame.submit(ids.depth_prepass, &draw_list);
            }
        }) {
            log::error!("Headless frame render failed: {}", e);
        }

        // Keep the clone for readback after all frames complete
        self.offscreen_texture = Some(OffscreenTexture(offscreen_clone));
    }

    fn collect_draws(&mut self, frame: &mut FrameContext, frustum: &katla_math::Frustum) {
        use crate::components::{DrawableComponent, TransformComponent};
        for (_entity_id, drawable, transform) in self
            .world
            .query::<(&DrawableComponent, &TransformComponent)>()
        {
            let mesh_handle = drawable.mesh_handle;
            if mesh_handle.is_none() {
                continue;
            }
            let material_handle = drawable.material_handle;
            if material_handle.is_none() {
                continue;
            }

            if let Some(local_bounds) = drawable.bounds {
                let world_mat = transform.transform.make_mat4();
                let world_bounds = local_bounds.transform(&world_mat);
                if !frustum.intersects_aabb(&world_bounds) {
                    continue;
                }
            }

            let mut draw = frame
                .draw(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array());

            if let Some(color) = drawable.color {
                draw = draw.with_color(color.to_array());
            }
            draw = draw.with_pbr(drawable.metallic, drawable.roughness, drawable.ao);
            if drawable.emission > 0.0 {
                draw = draw.with_emission(drawable.emission);
            }
            draw.submit();
        }
    }

    fn collect_and_upload_lights(&mut self) {
        use crate::components::{PointLight, TransformComponent};
        use katla_gfx::PointLightGPU;

        let mut lights = Vec::new();
        for (_entity, point_light, transform) in
            self.world.query::<(&PointLight, &TransformComponent)>()
        {
            let pos = transform.transform.position;
            lights.push(PointLightGPU {
                position: [pos.x(), pos.y(), pos.z()],
                color: point_light.color,
                intensity: point_light.intensity,
                range: point_light.range,
            });
        }
        self.renderer.upload_lights(&lights);
    }

    fn save_screenshot(&self) -> AppResult<()> {
        let Some(texture) = &self.offscreen_texture else {
            error!("No offscreen texture available for screenshot");
            return Err(crate::error::AppError::Other {
                message: "No offscreen texture available".to_string(),
            });
        };

        let width = HEADLESS_WIDTH;
        let height = HEADLESS_HEIGHT;

        let bgra_data = Renderer::readback_bgra_texture(&texture.0, width, height);

        // Convert BGRA to RGBA for PNG
        let rgba_data: Vec<u8> = bgra_data
            .chunks_exact(4)
            .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], 255])
            .collect();

        // Encode PNG
        let mut png_data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_data, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| crate::error::AppError::Other {
                    message: format!("PNG encode error: {}", e),
                })?;
            writer
                .write_image_data(&rgba_data)
                .map_err(|e| crate::error::AppError::Other {
                    message: format!("PNG write error: {}", e),
                })?;
        }

        std::fs::write(&self.screenshot_path, &png_data).map_err(|e| {
            crate::error::AppError::Other {
                message: format!("Failed to write screenshot: {}", e),
            }
        })?;

        info!(
            "Saved screenshot to {} ({}x{}, {} bytes)",
            self.screenshot_path,
            width,
            height,
            png_data.len()
        );

        Ok(())
    }

    fn dump_layout_if_needed(&mut self) {
        if self.layout_dumped {
            return;
        }

        // Layout dump is not supported in headless mode without the editor UI.
        // When both --headless and --dump-layout are set, just log that layout
        // dump was requested but no UI tree is available.
        if self.dump_layout_path.is_some() {
            log::info!("Layout dump requested in headless mode (no UI tree available)");
        }

        self.layout_dumped = true;
    }
}

impl HeadlessApplication {
    pub fn default_material(&self) -> katla_gfx::MaterialHandle {
        self.default_material_handle
    }
}
