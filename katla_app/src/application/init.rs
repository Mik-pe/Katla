use log::{error, info, warn};

use crate::application::Application;

impl Application {
    pub fn init(&mut self) {
        info!("Application::init() called");

        // Register scene resources
        self.world
            .insert_resource(crate::resources::AmbientLight::default());

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
            .init_particle_emit_pipeline(&particle_emit_shader_path)
            .expect("Failed to initialize particle emit pipeline");

        // Initialize particle simulate pipeline
        let particle_simulate_shader_path = self
            .resources
            .shader_path("particles/particle_simulate.wgsl");
        self.renderer
            .init_particle_simulate_pipeline(&particle_simulate_shader_path)
            .expect("Failed to initialize particle simulate pipeline");

        // Initialize particle draw command pipeline (writes indirect draw buffer after simulate)
        let particle_draw_command_shader_path = self
            .resources
            .shader_path("particles/particle_draw_command.wgsl");
        self.renderer
            .init_particle_draw_command_pipeline(&particle_draw_command_shader_path)
            .expect("Failed to initialize particle draw command pipeline");

        // Add particle compute passes to frame graph
        // These must be added after particle pipelines are initialized
        if let Some(ref particle_system) = self.renderer.particle_system {
            let emit_pipeline = particle_system
                .emit_pipeline_handle()
                .expect("Particle emit pipeline not initialized");
            let simulate_pipeline = particle_system
                .simulate_pipeline_handle()
                .expect("Particle simulate pipeline not initialized");

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
                                    RenderGraphError::VulkanError(format!(
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
                                    RenderGraphError::VulkanError(format!(
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
                                    RenderGraphError::VulkanError(format!(
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
                                    RenderGraphError::VulkanError(format!(
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
        // Inserted at position 0 so it runs before other compute passes (light culling,
        // particle emit/simulate) and all graphics passes. This ensures skeleton matrices
        // are ready for the subsequent copy commands and vertex shader skinning.
        if let Some(pipeline_handle) = self.renderer.animation_pipeline_handle() {
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
                        let output_buf = buffers.output_buffer();
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
        // This must run before geometry passes so culling results are available for PBR shading.
        // Inserted at position 0 — after animation_pose_eval, before particle passes.
        if self.renderer.has_light_culling() {
            use katla_gfx::render_graph::{PassDesc, PassType, RenderGraphError};
            self.frame_graph.insert_pass(
                0,
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

        // Initialize transient textures and register with bindless system
        self.frame_graph
            .initialize_transient_textures(&self.renderer)
            .expect("Failed to initialize transient textures");

        // Register HDR texture with bindless system for tonemapping
        let hdr_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "hdr_color")
            .expect("Failed to register HDR texture with bindless system");

        info!(
            "HDR texture registered with bindless system at index {}",
            hdr_bindless_index
        );

        // Set HDR texture index on tonemap pass
        self.frame_graph
            .set_tonemap_texture_index(self.tonemap_pass_id, hdr_bindless_index)
            .expect("Failed to set tonemap texture index");

        // Register viewport texture with bindless system for viewport rendering
        let viewport_bindless_index = self
            .frame_graph
            .register_transient_texture_bindless(&mut self.renderer, "viewport_0")
            .expect("Failed to register viewport texture with bindless system");

        info!(
            "Viewport texture registered with bindless system at index {}",
            viewport_bindless_index
        );

        // Set LDR texture base index for compositing shader to use
        self.frame_graph
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
                .expect("Failed to register stencil indicator texture with bindless system");

            info!(
                "Stencil indicator texture registered with bindless at index {}",
                stencil_indicator_index
            );

            self.editor.stencil_indicator_bindless_index = Some(stencil_indicator_index);

            self.frame_graph
                .set_overlay_texture_indices(
                    "wallhack_overlay",
                    viewport_bindless_index,
                    stencil_indicator_index,
                )
                .expect("Failed to set wallhack overlay texture indices");
        }

        // Load default scene from disk
        let scene_path = std::path::Path::new(crate::scene::DEFAULT_SCENE_PATH);
        match crate::scene::SceneManager::load_from_file(self, scene_path) {
            Ok(()) => info!(
                "Loaded default scene from {}",
                crate::scene::DEFAULT_SCENE_PATH
            ),
            Err(e) => error!(
                "Failed to load default scene from {}: {}",
                crate::scene::DEFAULT_SCENE_PATH,
                e
            ),
        }

        info!("Application::init() completed");
    }
}

#[cfg(feature = "editor")]
impl Application {
    /// Initialize GPU resources for billboard icons (mesh + material + icon textures).
    pub(crate) fn init_billboard_resources(&mut self) {
        use crate::billboard::BillboardResources;
        use crate::components::BillboardIcon;

        let mesh = self.renderer.create_plane_xy_mesh(1.0, 1.0, 1);

        let shader_path = self.resources.shader_path("billboard.wgsl");
        let material = self
            .renderer
            .compile_material(
                &shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    alpha_blended: true,
                    depth_test: true,
                    double_sided: true,
                    ..Default::default()
                },
            )
            .expect("Failed to create billboard material");

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
        let camera_entity = self.camera.borrow().entity;
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
