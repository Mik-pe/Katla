use log::{error, info, warn};

use katla_gfx::GpuRenderer;
use katla_gfx::primitives;

use crate::application::Application;
use crate::error::AppError;
impl Application {
    pub fn init(&mut self) -> Result<(), AppError> {
        info!("Application::init() called");

        // Register scene resources
        self.world
            .insert_resource(crate::resources::AmbientLight::default());

        match &mut self.renderer {
            katla_gfx::AnyRenderer::Vulkan(_) => self.init_vulkan()?,
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => self.init_metal()?,
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

        // Load scene from disk
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
        self.default_material_handle = self
            .renderer
            .unwrap_vulkan()
            .compile_material(
                &shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    ..Default::default()
                },
            )
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to create default HDR PBR material: {e}"),
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

        // Initialize particle emit pipeline
        let particle_emit_shader_path = self.resources.shader_path("particles/particle_emit.wgsl");
        self.renderer
            .unwrap_vulkan()
            .init_particle_emit_pipeline(&particle_emit_shader_path)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize particle emit pipeline: {e}"),
            })?;

        // Initialize particle simulate pipeline
        let particle_simulate_shader_path = self
            .resources
            .shader_path("particles/particle_simulate.wgsl");
        self.renderer
            .unwrap_vulkan()
            .init_particle_simulate_pipeline(&particle_simulate_shader_path)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize particle simulate pipeline: {e}"),
            })?;

        // Initialize particle draw command pipeline (writes indirect draw buffer after simulate)
        let particle_draw_command_shader_path = self
            .resources
            .shader_path("particles/particle_draw_command.wgsl");
        self.renderer
            .unwrap_vulkan()
            .init_particle_draw_command_pipeline(&particle_draw_command_shader_path)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize particle draw command pipeline: {e}"),
            })?;

        // Add particle compute passes to frame graph
        // These must be added after particle pipelines are initialized
        if let Some(ref particle_system) = self.renderer.unwrap_vulkan().particle_system {
            let emit_pipeline = particle_system.emit_pipeline_handle().ok_or_else(|| {
                AppError::RendererInitFailed {
                    reason: "Particle emit pipeline not initialized".to_string(),
                }
            })?;
            let simulate_pipeline =
                particle_system.simulate_pipeline_handle().ok_or_else(|| {
                    AppError::RendererInitFailed {
                        reason: "Particle simulate pipeline not initialized".to_string(),
                    }
                })?;

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
                                    RenderGraphError::BackendError(format!(
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
                                    RenderGraphError::BackendError(format!(
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
                                    RenderGraphError::BackendError(format!(
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
                                    RenderGraphError::BackendError(format!(
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
        // Inserted at position 0 so it runs before light_culling, particle passes,
        // and all graphics passes. This ensures skeleton matrices are ready
        // for the subsequent copy commands and vertex shader skinning.
        if let Some(pipeline_handle) = self.renderer.unwrap_vulkan().animation_pipeline_handle() {
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
                        let output_buf = match buffers.output_buffer() {
                            Ok(buf) => buf,
                            Err(_) => return Ok(()),
                        };
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
        // This must run after animation_pose_eval (which computes skeleton matrices)
        // and before particle passes and geometry rendering.
        // Inserted at position 1 so it follows animation_pose_eval at position 0.
        if self.renderer.capabilities().supports_light_culling {
            use katla_gfx::render_graph::{PassDesc, PassType, RenderGraphError};
            self.frame_graph.insert_pass(
                1,
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

        // Re-resolve pass IDs after insert_pass calls may have shifted indices
        self.pass_ids.refresh(&self.frame_graph);

        // Initialize transient textures and register with bindless system
        self.frame_graph
            .initialize_transient_textures(&mut self.renderer)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to initialize transient textures: {e}"),
            })?;

        // Register HDR texture with bindless system for tonemapping
        let hdr_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "hdr_color")
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to register HDR texture with bindless system: {e}"),
            })?;

        info!(
            "HDR texture registered with bindless system at index {}",
            hdr_bindless_index
        );

        // Set HDR texture index on tonemap pass
        self.frame_graph
            .set_tonemap_texture_index(self.pass_ids.tonemap, hdr_bindless_index)
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to set tonemap texture index: {e}"),
            })?;

        // Register viewport texture with bindless system for viewport rendering
        let viewport_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "viewport_0")
            .map_err(|e| AppError::RendererInitFailed {
                reason: format!("Failed to register viewport texture with bindless system: {e}"),
            })?;

        info!(
            "Viewport texture registered with bindless system at index {}",
            viewport_bindless_index
        );

        // Set LDR texture base index for compositing shader to use
        self.frame_graph
            .as_vulkan_mut()
            .set_ldr_texture_base_index(viewport_bindless_index);

        // Set viewport bindless index in editor UI
        #[cfg(feature = "editor")]
        {
            self.editor
                .editor_ui
                .set_viewport_bindless_index(viewport_bindless_index);

            let stencil_indicator_index = self
                .frame_graph
                .register_transient_texture_bindless(&mut self.renderer, "stencil_indicator")
                .map_err(|e| AppError::RendererInitFailed {
                    reason: format!(
                        "Failed to register stencil indicator texture with bindless system: {e}"
                    ),
                })?;

            info!(
                "Stencil indicator texture registered with bindless at index {}",
                stencil_indicator_index
            );

            self.editor.stencil_indicator_bindless_index = Some(stencil_indicator_index);

            self.frame_graph
                .as_vulkan_mut()
                .set_overlay_texture_indices(
                    self.pass_ids.wallhack_overlay,
                    viewport_bindless_index,
                    stencil_indicator_index,
                )
                .map_err(|e| AppError::RendererInitFailed {
                    reason: format!("Failed to set wallhack overlay texture indices: {e}"),
                })?;
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
        self.default_material_handle =
            self.renderer
                .compile_material(&shader_str, "pbr")
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

        // Initialize GPU animation compute pipeline
        let anim_shader_path = self
            .resources
            .shader_path("compute/animation/pose_eval.wgsl");
        if let Err(e) = self.renderer.init_animation_pipeline(&anim_shader_path) {
            warn!("Failed to initialize Metal animation pipeline: {}", e);
        } else {
            info!("Animation pipeline initialized (Metal)");
        }

        // Compile UI shader and store the material handle for Metal UI rendering
        let ui_shader_path = self.resources.shader_path("ui/ui.wgsl");
        match self
            .renderer
            .compile_material(&ui_shader_path.to_string_lossy(), "ui")
        {
            Ok(ui_material) => {
                self.renderer.set_ui_material(ui_material);
                info!("UI material compiled and set (Metal)");
            }
            Err(e) => {
                warn!("Failed to compile Metal UI material: {}", e);
            }
        }

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
        if let Some(hdr_idx) = self.renderer.geometry_hdr_bindless_index() {
            self.frame_graph
                .set_tonemap_texture_index(self.pass_ids.tonemap, hdr_idx)
                .ok();
            info!("Tonemap pass HDR texture index set to {} (Metal)", hdr_idx);
        }

        // Set viewport bindless index in editor UI
        #[cfg(feature = "editor")]
        {
            if let Some(vp_idx) = self.renderer.viewport_bindless_index() {
                self.editor.editor_ui.set_viewport_bindless_index(vp_idx);
            }
        }

        Ok(())
    }
}

#[cfg(feature = "editor")]
impl Application {
    /// Initialize GPU resources for billboard icons (mesh + material + icon textures).
    pub(crate) fn init_billboard_resources(&mut self) {
        use crate::billboard::BillboardResources;
        use crate::components::BillboardIcon;

        let mesh = primitives::create_plane_xy(&mut self.renderer, 1.0, 1.0, 1);

        let shader_path = self.resources.shader_path("billboard.wgsl");
        let material = match &mut self.renderer {
            katla_gfx::AnyRenderer::Vulkan(r) => match r.compile_material(
                &shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    alpha_blended: true,
                    depth_test: true,
                    double_sided: true,
                    ..Default::default()
                },
            ) {
                Ok(m) => m,
                Err(e) => {
                    log::error!("Failed to create billboard material: {e}");
                    return;
                }
            },
            #[cfg(target_os = "macos")]
            katla_gfx::AnyRenderer::Metal(_) => match self
                .renderer
                .compile_material(&shader_path.to_string_lossy(), "billboard")
            {
                Ok(m) => m,
                Err(e) => {
                    log::error!("Failed to create billboard material: {e}");
                    return;
                }
            },
        };

        self.gpu_resource_tracker.set_protected_material(material);

        let mut icon_textures = std::collections::HashMap::new();
        for icon in [BillboardIcon::Lightbulb, BillboardIcon::Fire] {
            let rasterized = crate::rendering::rasterize_billboard_icon(icon, 64);
            let desc =
                katla_gfx::TextureDescriptor::rgba8_srgb(rasterized.width, rasterized.height);
            let texture_handle = self.renderer.create_texture(&desc, &rasterized.pixels);
            icon_textures.insert(icon, texture_handle);
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
