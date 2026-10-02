use log::{error, info, warn};

use katla_gfx::GpuRenderer;
use katla_gfx::primitives;

use crate::application::Application;
use crate::error::AppError;
impl Application {
    pub fn init(&mut self) -> Result<(), AppError> {
        info!("Application::init() called");

        let uses_katla_scene = self.frame_graph_runtime.uses_katla_scene();
        if uses_katla_scene {
            self.world
                .insert_resource(crate::resources::AmbientLight::default());

            match &mut self.renderer {
                katla_gfx::AnyRenderer::Vulkan(_) => self.init_vulkan()?,
                #[cfg(target_os = "macos")]
                katla_gfx::AnyRenderer::Metal(_) => self.init_metal()?,
            }
        } else {
            info!("Skipping Katla scene renderer initialization for graph-only runtime");
        }

        match crate::systems::AudioSystem::new() {
            Ok(audio) => {
                info!("Audio system initialized");
                let engine = audio.engine();
                engine.set_master_volume(self.preferences.audio.master_volume);
                engine.set_category_volume(
                    katla_audio::AudioCategory::Sfx,
                    self.preferences.audio.sfx_volume,
                );
                engine.set_category_volume(
                    katla_audio::AudioCategory::Music,
                    self.preferences.audio.music_volume,
                );
                engine.set_category_volume(
                    katla_audio::AudioCategory::Ambient,
                    self.preferences.audio.ambient_volume,
                );
                self.audio_system = Some(audio);
            }
            Err(katla_audio::AudioError::DeviceAccessDenied(ref msg)) => {
                warn!("Audio initialization failed (access denied): {msg}");
                warn!("Audio will be unavailable. Grant audio permissions and restart.");
            }
            Err(katla_audio::AudioError::DeviceNotFound(ref msg)) => {
                warn!("Audio initialization failed (no device): {msg}");
                warn!("Audio will be unavailable until an output device is connected.");
            }
            Err(e) => {
                warn!("Audio initialization failed: {e}");
                warn!("Audio will be unavailable.");
            }
        }

        if uses_katla_scene {
            let scene_path_str = self
                .info
                .scene_path
                .clone()
                .unwrap_or_else(|| crate::scene::default_scene_path().display().to_string());
            let scene_path = std::path::Path::new(&scene_path_str);
            match crate::scene::SceneManager::load_from_file(self, scene_path) {
                Ok(()) => info!("Loaded scene from {}", scene_path_str),
                Err(e) => error!("Failed to load scene from {}: {}", scene_path_str, e),
            }
        } else {
            info!("Skipping implicit scene loading for graph-only runtime");
        }

        info!("Application::init() completed");
        Ok(())
    }
}

impl Application {
    fn init_vulkan(&mut self) -> Result<(), AppError> {
        // Initialize default PBR material
        let shader_path = self.resources.shader_path("model_pbr.wgsl");
        info!(
            "Loading default PBR material from: {}",
            shader_path.display()
        );

        // Create HDR PBR material for rendering to HDR intermediate
        let descriptor =
            katla_gfx::PipelineDescriptor::pbr(shader_path.to_string_lossy().into_owned())
                .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat);
        self.default_material_handle =
            self.renderer.compile_material(&descriptor).map_err(|e| {
                AppError::RendererInitFailed {
                    reason: format!("Failed to create default HDR PBR material: {e}"),
                }
            })?;

        info!("Default HDR PBR material loaded successfully");

        // Set the protected material in the GPU resource tracker so it's never destroyed
        self.gpu_resource_tracker
            .set_protected_material(self.default_material_handle);

        // Initialize editor GPU resources
        #[cfg(feature = "editor")]
        {
            self.init_gizmo_resources();
            self.init_billboard_resources();
        }

        // Initialize animation pose evaluation pipeline
        let anim_shader_path = self
            .resources
            .shader_path("compute/animation/pose_eval.wgsl");
        self.renderer
            .init_animation_pipeline(&anim_shader_path)
            .map_err(|error| AppError::RendererInitFailed {
                reason: error.to_string(),
            })?;

        // Create GPU animation system (ECS queries only, GPU resources on renderer)
        self.gpu_animation_system =
            Some(crate::systems::gpu_animation_system::GpuAnimationSystem::new());

        self.install_scene_compute_graph()?;

        // Initialize transient textures and register with bindless system
        self.frame_graph
            .initialize_transient_textures(&mut self.renderer)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize transient textures: {e}"),
            })?;
        self.frame_graph
            .initialize_transient_buffers(&mut self.renderer)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize transient buffers: {e}"),
            })?;

        let hdr_bindless_index =
            if let Some(name) = self.frame_graph_bindings.resources.hdr_color.as_deref() {
                let index = self
                    .frame_graph
                    .register_transient_texture_bindless(&mut self.renderer, name)
                    .map_err(|e| AppError::RendererInitFailed {
                        reason: format!(
                            "Failed to register HDR resource '{name}' with bindless system: {e}"
                        ),
                    })?;
                info!("HDR resource '{name}' registered at bindless index {index}");
                Some(index)
            } else {
                None
            };

        if let (Some(pass_id), Some(texture_index)) = (self.pass_ids.tonemap, hdr_bindless_index) {
            self.frame_graph
                .set_tonemap_texture_index(pass_id, texture_index)
                .map_err(|e| AppError::RendererInitFailed {
                    reason: format!("Failed to set tonemap texture index: {e}"),
                })?;
        }

        let viewport_bindless_index =
            if let Some(name) = self.frame_graph_bindings.resources.viewport.as_deref() {
                let index = self
                .frame_graph
                .register_transient_texture_bindless(&mut self.renderer, name)
                .map_err(|e| AppError::RendererInitFailed {
                    reason: format!(
                        "Failed to register viewport resource '{name}' with bindless system: {e}"
                    ),
                })?;
                self.frame_graph
                    .as_vulkan_mut()
                    .set_ldr_texture_base_index(index);
                info!("Viewport resource '{name}' registered at bindless index {index}");
                Some(index)
            } else {
                None
            };

        #[cfg(feature = "editor")]
        {
            if let Some(viewport_index) = viewport_bindless_index {
                self.editor
                    .editor_ui
                    .set_viewport_bindless_index(viewport_index);
            }

            let stencil_indicator_index = if let Some(name) = self
                .frame_graph_bindings
                .resources
                .stencil_indicator
                .as_deref()
            {
                Some(
                    self.frame_graph
                        .register_transient_texture_bindless(&mut self.renderer, name)
                        .map_err(|e| AppError::RendererInitFailed {
                            reason: format!(
                                "Failed to register stencil-indicator resource '{name}': {e}"
                            ),
                        })?,
                )
            } else {
                None
            };

            if let (Some(pass_id), Some(viewport_index), Some(stencil_index)) = (
                self.pass_ids.wallhack_overlay,
                viewport_bindless_index,
                stencil_indicator_index,
            ) {
                self.frame_graph
                    .as_vulkan_mut()
                    .set_overlay_texture_indices(pass_id, viewport_index, stencil_index)
                    .map_err(|e| AppError::RendererInitFailed {
                        reason: format!("Failed to set wallhack overlay texture indices: {e}"),
                    })?;
            }
        }

        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Application {
    fn init_metal(&mut self) -> Result<(), AppError> {
        // Initialize default PBR material via GpuRenderer trait
        let shader_path = self.resources.shader_path("model_pbr.wgsl");
        let shader_str = shader_path.to_string_lossy();
        self.default_material_handle = self
            .renderer
            .compile_material(&katla_gfx::PipelineDescriptor::pbr(shader_str.into_owned()))
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to create default PBR material: {e}"),
            })?;

        // Propagate to the renderer so its default_material() returns the correct handle
        self.renderer
            .set_default_material(self.default_material_handle);

        self.gpu_resource_tracker
            .set_protected_material(self.default_material_handle);

        info!("Default PBR material loaded (Metal)");

        // Initialize editor GPU resources
        #[cfg(feature = "editor")]
        {
            self.init_gizmo_resources();
            self.init_billboard_resources();
        }

        // Initialize Forward+ light culling
        let extent = self.renderer.swapchain_extent();
        let light_culling_shader_path = self.resources.shader_path("light_culling.wgsl");
        if let Err(e) = self.renderer.init_light_culling(
            extent.width,
            extent.height,
            &light_culling_shader_path,
        ) {
            warn!("Failed to initialize Metal light culling: {}", e);
        } else {
            info!("Light culling initialized (Metal)");
        }

        // Initialize shadow map resources
        if let Err(e) = self.renderer.init_shadow_resources() {
            warn!("Failed to initialize Metal shadow resources: {}", e);
        } else {
            info!("Shadow resources initialized (Metal)");
        }

        // Initialize shadow depth pipeline
        let shadow_shader_path = self.resources.shader_path("shadow/shadow_depth.wgsl");
        if let Err(e) = self
            .renderer
            .init_pass_pipeline(katla_gfx::PipelineKind::Shadow, &[&shadow_shader_path])
        {
            warn!("Failed to initialize Metal shadow pipeline: {}", e);
        } else {
            info!("Shadow pipeline initialized (Metal)");
        }

        // Initialize skinned shadow depth pipeline
        let shadow_skinned_shader_path = self
            .resources
            .shader_path("shadow/shadow_depth_skinned.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::ShadowSkinned,
            &[&shadow_skinned_shader_path],
        ) {
            warn!("Failed to initialize Metal skinned shadow pipeline: {}", e);
        } else {
            info!("Skinned shadow pipeline initialized (Metal)");
        }

        // Initialize GPU animation compute pipeline
        let anim_shader_path = self
            .resources
            .shader_path("compute/animation/pose_eval.wgsl");
        self.renderer
            .init_animation_pipeline(&anim_shader_path)
            .map_err(|error| AppError::RendererInitFailed {
                reason: error.to_string(),
            })?;

        // Initialize sky pipeline for procedural atmosphere
        let sky_shader_path = self.resources.shader_path("sky.wgsl");
        if let Err(e) = self
            .renderer
            .init_pass_pipeline(katla_gfx::PipelineKind::Sky, &[&sky_shader_path])
        {
            warn!("Failed to initialize Metal sky pipeline: {}", e);
        } else {
            info!("Sky pipeline initialized (Metal)");
        }

        // Initialize tonemapping pipeline for HDR-to-LDR conversion
        let tonemap_shader_path = self.resources.shader_path("tonemapping.wgsl");
        if let Err(e) = self
            .renderer
            .init_pass_pipeline(katla_gfx::PipelineKind::Tonemap, &[&tonemap_shader_path])
        {
            warn!("Failed to initialize Metal tonemap pipeline: {}", e);
        } else {
            info!("Tonemap pipeline initialized (Metal)");
        }

        // Initialize depth prepass pipeline
        let depth_prepass_shader_path = self.resources.shader_path("depth_prepass.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::DepthPrepass,
            &[&depth_prepass_shader_path],
        ) {
            warn!("Failed to initialize Metal depth prepass pipeline: {}", e);
        } else {
            info!("Depth prepass pipeline initialized (Metal)");
        }

        // Initialize skinned depth prepass pipeline
        let depth_prepass_skinned_shader_path =
            self.resources.shader_path("depth_prepass_skinned.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::DepthPrepassSkinned,
            &[&depth_prepass_skinned_shader_path],
        ) {
            warn!(
                "Failed to initialize Metal skinned depth prepass pipeline: {}",
                e
            );
        } else {
            info!("Skinned depth prepass pipeline initialized (Metal)");
        }

        // Initialize billboard depth prepass pipeline
        let billboard_depth_shader_path = self.resources.shader_path("billboard_depth.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::DepthPrepassBillboard,
            &[&billboard_depth_shader_path],
        ) {
            warn!(
                "Failed to initialize Metal billboard depth prepass pipeline: {}",
                e
            );
        } else {
            info!("Billboard depth prepass pipeline initialized (Metal)");
        }

        // Initialize outline pipelines for stencil-based selection highlight
        let stencil_mark_shader_path = self.resources.shader_path("outline/stencil_mark.wgsl");
        let stencil_mark_skinned_shader_path = self
            .resources
            .shader_path("outline/stencil_mark_skinned.wgsl");
        let outline_draw_shader_path = self.resources.shader_path("outline/outline_draw.wgsl");
        let outline_draw_skinned_shader_path = self
            .resources
            .shader_path("outline/outline_draw_skinned.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::Outline,
            &[
                &stencil_mark_shader_path,
                &stencil_mark_skinned_shader_path,
                &outline_draw_shader_path,
                &outline_draw_skinned_shader_path,
            ],
        ) {
            warn!("Failed to initialize Metal outline pipelines: {}", e);
        } else {
            info!("Outline pipelines initialized (Metal)");
        }

        // Initialize GPU picking pipeline
        let picking_shader_path = self.resources.shader_path("picking/object_id.wgsl");
        if let Err(e) = self
            .renderer
            .init_pass_pipeline(katla_gfx::PipelineKind::Picking, &[&picking_shader_path])
        {
            warn!("Failed to initialize Metal picking pipeline: {}", e);
        } else {
            info!("Picking pipeline initialized (Metal)");
        }

        let picking_skinned_shader_path =
            self.resources.shader_path("picking/object_id_skinned.wgsl");
        if let Err(e) = self.renderer.init_pass_pipeline(
            katla_gfx::PipelineKind::PickingSkinned,
            &[&picking_skinned_shader_path],
        ) {
            warn!("Failed to initialize Metal skinned picking pipeline: {}", e);
        } else {
            info!("Skinned picking pipeline initialized (Metal)");
        }

        // Set tonemap texture index on the tonemap pass
        if let (Some(pass_id), Some(hdr_idx)) = (
            self.pass_ids.tonemap,
            self.renderer.geometry_hdr_bindless_index(),
        ) {
            self.frame_graph
                .set_tonemap_texture_index(pass_id, hdr_idx)
                .map_err(|e| AppError::RendererInitFailed {
                    reason: format!("Failed to set Metal tonemap texture index: {e}"),
                })?;
            info!("Tonemap pass HDR texture index set to {} (Metal)", hdr_idx);
        }

        // Set viewport bindless index in editor UI
        #[cfg(feature = "editor")]
        {
            if let Some(vp_idx) = self.renderer.viewport_bindless_index() {
                self.editor.editor_ui.set_viewport_bindless_index(vp_idx);
            }
        }

        self.gpu_animation_system =
            Some(crate::systems::gpu_animation_system::GpuAnimationSystem::new());
        self.install_scene_compute_graph()?;

        Ok(())
    }
}

#[cfg(feature = "editor")]
impl Application {
    /// Initialize GPU resources for billboard icons (mesh + material + icon textures).
    pub(crate) fn init_billboard_resources(&mut self) {
        use crate::billboard::BillboardResources;
        use crate::components::BillboardIcon;

        let mesh = match primitives::create_plane_xy(&mut self.renderer, 1.0, 1.0, 1) {
            Ok(mesh) => mesh,
            Err(error) => {
                log::error!("Failed to create billboard quad mesh: {error}");
                return;
            }
        };

        let shader_path = self.resources.shader_path("billboard.wgsl");
        let descriptor =
            katla_gfx::PipelineDescriptor::billboard(shader_path.to_string_lossy().into_owned())
                .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat);
        let material = match self.renderer.compile_material(&descriptor) {
            Ok(m) => m,
            Err(e) => {
                log::error!("Failed to create billboard material: {e}");
                return;
            }
        };

        self.gpu_resource_tracker.set_protected_material(material);

        let mut icon_textures = std::collections::HashMap::new();
        for icon in [BillboardIcon::Lightbulb, BillboardIcon::Fire] {
            let rasterized = crate::rendering::rasterize_billboard_icon(icon, 64);
            let desc =
                katla_gfx::TextureDescriptor::rgba8_srgb(rasterized.width, rasterized.height);
            // Icons are optional chrome: a failed upload skips the icon
            // (consumers skip missing entries) instead of failing init.
            match self.renderer.create_texture(&desc, &rasterized.pixels) {
                Ok(texture_handle) => {
                    icon_textures.insert(icon, texture_handle);
                }
                Err(error) => {
                    warn!("Billboard icon {:?} upload failed: {}", icon, error);
                }
            }
        }

        self.editor.billboard_resources = BillboardResources {
            mesh,
            material,
            icon_textures,
            initialized: true,
        };

        info!("Billboard GPU resources initialized");
    }

    pub(crate) fn focus_camera_on_entity(&mut self, entity_id: katla_ecs::EntityId) {
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
        let camera_entity = self.camera.entity;
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
}
