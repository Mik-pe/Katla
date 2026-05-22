//! Frame rendering implementation.
//!
//! This module implements frame rendering using the FrameGraph API
//! and the new FrameContext for automatic instance allocation.

use super::Application;
use crate::rendering::FrameContext;
use katla_gfx::GpuRenderer;
use katla_gfx::renderer::FrameUniforms;
use katla_gfx::renderer::UIDrawList;
use log::info;

// Shared backend-agnostic helper methods used by both Vulkan and Metal paths.
impl Application {
    /// Collect drawable components from the ECS world and submit to FrameContext.
    ///
    /// This automatically allocates instance indices and builds the draw list.
    /// Also populates entity_instance_map for GPU picking resolution.
    pub(crate) fn collect_draws_with_context(
        &mut self,
        frame: &mut FrameContext,
        frustum: &katla_math::Frustum,
    ) {
        use crate::components::{DrawableComponent, TransformComponent};

        let entity_count = self.world.entity_count();
        let mut drawable_count = 0;
        let mut culled_count = 0;
        #[cfg(feature = "editor")]
        self.editor.draw_entity_map_entries.clear();

        for (entity_id, drawable, transform) in self
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
                    culled_count += 1;
                    continue;
                }
            }

            if drawable.skeleton_handle.is_some() {
                // Skeleton matrices are computed on the GPU via the animation
                // pose evaluation compute pass and copied to the per-entity
                // SkeletonBuffer. No CPU upload needed.
            }

            let mut draw = frame
                .draw(mesh_handle, material_handle)
                .with_transform(transform.transform.make_mat4().to_array());

            // Skeleton for skinned meshes
            if drawable.skeleton_handle.is_some() {
                draw = draw.with_skeleton(drawable.skeleton_handle);
            }

            if let Some(color) = drawable.color {
                draw = draw.with_color(color.to_array());
            }

            draw = draw.with_pbr(drawable.metallic, drawable.roughness, drawable.ao);

            if drawable.emission > 0.0 {
                draw = draw.with_emission(drawable.emission);
            }

            draw.submit();

            #[cfg(feature = "editor")]
            self.editor
                .draw_entity_map_entries
                .push((frame.instance_count() - 1, entity_id));

            drawable_count += 1;
        }

        if culled_count > 0 {
            log::debug!(
                "Submitted {} draw calls, culled {} off-screen ({} total entities)",
                drawable_count,
                culled_count,
                entity_count
            );
        } else {
            log::debug!(
                "Submitted {} draw calls from {} entities",
                drawable_count,
                entity_count
            );
        }

        #[cfg(feature = "editor")]
        {
            let entries = std::mem::take(&mut self.editor.draw_entity_map_entries);
            self.build_entity_instance_map(entries);
        }
    }

    /// Get the viewport size in pixels.
    #[cfg(feature = "editor")]
    pub(crate) fn viewport_size(&self) -> (u32, u32) {
        self.editor.editor_ui.viewport_size()
    }

    #[cfg(not(feature = "editor"))]
    pub(crate) fn viewport_size(&self) -> (u32, u32) {
        let extent = self.renderer.swapchain_extent();
        (extent.width, extent.height)
    }

    /// Build entity_instance_map and entity_to_instance_indices from collected draw entries.
    #[cfg(feature = "editor")]
    pub(crate) fn build_entity_instance_map(&mut self, entries: Vec<(u32, katla_ecs::EntityId)>) {
        self.editor.entity_instance_map.clear();
        self.editor.entity_to_instance_indices.clear();
        for (idx, entity_id) in entries {
            self.editor.entity_instance_map.insert(idx, entity_id);
            self.editor
                .entity_to_instance_indices
                .entry(entity_id)
                .or_default()
                .push(idx);
        }
    }

    #[cfg(not(feature = "editor"))]
    pub(crate) fn build_entity_instance_map(&mut self, _entries: Vec<(u32, katla_ecs::EntityId)>) {}

    /// Prepare draw lists: shadow filtering, outline selection, billboard generation.
    #[cfg(feature = "editor")]
    pub(crate) fn prepare_draw_lists(
        &mut self,
        draw_list: &mut katla_gfx::renderer::DrawList,
    ) -> (
        katla_gfx::renderer::DrawList,
        Option<katla_gfx::renderer::DrawList>,
    ) {
        self.collect_billboard_draw_calls(draw_list);
        self.prepare_editor_draw_lists(draw_list)
    }

    #[cfg(not(feature = "editor"))]
    pub(crate) fn prepare_draw_lists(
        &mut self,
        draw_list: &mut katla_gfx::renderer::DrawList,
    ) -> (
        katla_gfx::renderer::DrawList,
        Option<katla_gfx::renderer::DrawList>,
    ) {
        (draw_list.clone(), None)
    }
}

#[cfg(not(target_os = "macos"))]
impl Application {
    /// Render a single frame using the frame graph.
    ///
    /// Uses FrameContext for draw submission with automatic instance allocation.
    pub fn render_frame(
        &mut self,
        ui_draw_list: Option<UIDrawList>,
        delta_time: f32,
        frame_count: usize,
    ) {
        // If the swapchain was signaled as out-of-date on the previous frame,
        // recreate it before rendering. This handles the common macOS/MoltenVK
        // case where the first few frames return VK_SUBOPTIMAL_KHR or
        // VK_ERROR_OUT_OF_DATE_KHR until the CAMetalLayer.drawableSize settles.
        if self.needs_swapchain_recreate {
            self.needs_swapchain_recreate = false;
            self.recreate_swapchain_resources();
            info!("=== Resize complete ===");
        }

        // Note: viewport bindless index is updated BEFORE generate_ui_draw_list()
        // in the RedrawRequested handler to ensure the UI samples from the
        // correct per-frame transient texture.

        let (viewport_width, viewport_height) = self.viewport_size();
        let viewport_aspect = if viewport_height > 0 {
            viewport_width as f32 / viewport_height as f32
        } else {
            16.0 / 9.0 // Fallback to default aspect ratio
        };
        self.camera
            .aspect_ratio_changed(&mut self.world, viewport_aspect);

        let mut frame = FrameContext::new();

        let view_mat = self.camera.get_view_mat(&self.world);
        let proj_mat = self.camera.get_proj_mat(&self.world);
        let frustum = katla_math::Frustum::from_proj_and_view(&proj_mat, &view_mat);
        let camera_entity = self.camera.entity;

        use crate::components::TransformComponent;
        let cam_pos = if let Some(transform) = self
            .world
            .get_component::<TransformComponent>(camera_entity)
        {
            [
                transform.transform.position.x(),
                transform.transform.position.y(),
                transform.transform.position.z(),
                1.0,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let inv_view_proj = {
            use katla_math::Mat4;
            (proj_mat * view_mat)
                .inverse()
                .unwrap_or_else(Mat4::identity)
        };

        // Wait for the current frame's previous GPU submission to complete
        // before writing to per-frame storage buffers.
        if let Err(e) = self.renderer.wait_for_frame() {
            log::error!("Failed to wait for frame: {}", e);
            return;
        }

        // Tile grid dimensions for Forward+ light culling.
        // Must match the render target (swapchain) size, NOT the editor viewport panel size,
        // because clip_position in the fragment shader covers the full render target.
        let extent = self.renderer.swapchain_extent();
        let tiles_x = extent.width.div_ceil(16);
        let tiles_y = extent.height.div_ceil(16);

        let frame_uniforms = FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            // Sunlight defaults
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: [
                1.0,
                self.renderer
                    .depth_texture_base_index()
                    .map(|base| base + self.renderer.current_frame() as u32)
                    .unwrap_or(0) as f32,
                0.0,
                0.0,
            ],
            tiles: [tiles_x, tiles_y, 0, 0],
            tonemap: [1.0, 2.2, 0.0, 0.0],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        };
        frame.set_frame_uniforms(frame_uniforms.clone());

        // Collect draw calls from ECS world using FrameContext
        self.collect_draws_with_context(&mut frame, &frustum);

        // Collect point lights for Forward+ culling
        self.collect_and_upload_lights();

        // Must be before update_shadows so CSM uses the current frame's view/proj matrices
        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());

        self.renderer.update_shadows([
            frame_uniforms.light_direction[0],
            frame_uniforms.light_direction[1],
            frame_uniforms.light_direction[2],
        ]);

        self.renderer.upload_shadow_cascades();

        let mut draw_list = frame.take_draw_list();
        draw_list.sort_by_material();

        let (shadow_draw_list, outline_draw_list) = self.prepare_draw_lists(&mut draw_list);

        if let Err(e) = self.renderer.execute_draw_calls(&draw_list) {
            log::error!("Failed to execute draw calls: {}", e);
            return; // Skip rendering this frame
        }

        log::debug!(
            "About to submit {} draw calls to geometry pass",
            draw_list.len()
        );

        let frame_index = self.renderer.current_frame() as u32;
        if let Some(ref mut particle_system) = self.renderer.unwrap_vulkan().particle_system {
            match particle_system.update(delta_time, frame_index) {
                Ok((_max_alive, emit_count)) => {
                    let emit_workgroups = if emit_count > 0 {
                        emit_count.div_ceil(katla_gfx::particles::PARTICLE_EMIT_WORKGROUP_SIZE)
                    } else {
                        0
                    };

                    if emit_workgroups == 0 && emit_count > 0 {
                        log::warn!(
                            "Frame {}: emit_count={} but emit_workgroups=0! Particles won't be emitted!",
                            frame_count,
                            emit_count
                        );
                    }

                    // Simulate workgroups use a generous upper bound based on emitter configs:
                    //   sum(emit_rate_i * base_lifetime_i * (1 + lifetime_variation_i))
                    // No GPU readback needed — simulate shader self-bounds via counters.
                    // Over-dispatching is cheap (extra workgroups exit immediately).
                    let max_alive = particle_system.max_estimated_alive();
                    let total_particles_to_simulate = max_alive + emit_count;
                    let simulate_workgroups = if total_particles_to_simulate > 0 {
                        total_particles_to_simulate
                            .div_ceil(katla_gfx::particles::PARTICLE_SIMULATE_WORKGROUP_SIZE)
                    } else {
                        1 // ALWAYS run at least 1 workgroup for swap to happen
                    };

                    log::debug!(
                        "Particle compute workgroups: emit {} particles = {} workgroups, simulate ~{} max_alive + {} emit = {} total particles = {} workgroups",
                        emit_count,
                        emit_workgroups,
                        max_alive,
                        emit_count,
                        total_particles_to_simulate,
                        simulate_workgroups
                    );

                    // Update frame graph with workgroup counts for this frame
                    self.frame_graph
                        .as_vulkan_mut()
                        .set_particle_emit_workgroup_count(emit_workgroups);
                    self.frame_graph
                        .as_vulkan_mut()
                        .set_particle_simulate_workgroup_count(simulate_workgroups);
                }
                Err(e) => {
                    log::error!("Failed to update particle system: {}", e);
                }
            }
        } else {
            log::warn!("⚠️ No particle system in renderer!");
        }

        if let Err(e) = self.renderer.render(&mut self.frame_graph, |frame| {
            log::debug!(
                "Inside render closure: submitting {} draw calls to geometry pass",
                draw_list.len()
            );

            let ids = &self.pass_ids;

            if !draw_list.is_empty() {
                frame.submit(ids.depth_prepass, &draw_list);
                frame.submit(ids.geometry, &draw_list);
                frame.submit(ids.shadow, &shadow_draw_list);
                log::debug!(
                    "Submitted {} draw calls to depth_prepass + geometry, {} to shadow",
                    draw_list.len(),
                    shadow_draw_list.len()
                );
            } else {
                log::warn!("No draw calls to submit to geometry pass!");
            }

            if let Some(ref outline_dl) = outline_draw_list
                && !outline_dl.is_empty()
            {
                frame.submit(ids.outline, outline_dl);
                frame.submit(ids.stencil_indicator, outline_dl);
                log::debug!(
                    "Submitted {} selected draw calls to outline + stencil_indicator passes",
                    outline_dl.len()
                );
            }

            if let Some(ref ui_list) = ui_draw_list {
                log::debug!("Submitting {} UI draw commands", ui_list.commands.len());
                frame.submit_ui(ids.ui, ui_list);
            }
        }) {
            match &e {
                katla_gfx::error::RendererError::SwapchainOutOfDate => {
                    log::debug!("Swapchain out of date, triggering recreation on next frame");
                    // Defer recreation to the next frame to avoid complex re-entrancy.
                    // The next RedrawRequested will call recreate_swapchain via a flag.
                    self.needs_swapchain_recreate = true;
                    return;
                }
                _ => {
                    log::error!("Frame render failed, skipping frame: {}", e);
                    return;
                }
            }
        }
    }

    /// Collect point lights from the ECS world and upload to the GPU
    /// for Forward+ tile-based light culling.
    fn collect_and_upload_lights(&mut self) {
        use crate::components::{PointLight, TransformComponent};
        use katla_gfx::PointLightGPU;

        self.point_lights_buffer.clear();
        for (_entity, point_light, transform) in
            self.world.query::<(&PointLight, &TransformComponent)>()
        {
            let pos = transform.transform.position;
            self.point_lights_buffer.push(PointLightGPU {
                position: [pos.x(), pos.y(), pos.z()],
                range: point_light.range,
                color: point_light.color,
                intensity: point_light.intensity,
            });
        }

        if !self.point_lights_buffer.is_empty() {
            log::debug!(
                "Uploading {} point lights to GPU for Forward+ culling",
                self.point_lights_buffer.len()
            );
        }
        self.renderer.upload_lights(&self.point_lights_buffer);
    }

    /// Recreate the swapchain and update all dependent resources.
    ///
    /// This is called when:
    /// - The window is resized (`WindowEvent::Resized`)
    /// - The window is unoccluded (`WindowEvent::Occluded(false)`)
    /// - `acquire_next_image` or `queue_present` returns `VK_SUBOPTIMAL_KHR` / `VK_ERROR_OUT_OF_DATE_KHR`
    pub(crate) fn recreate_swapchain_resources(&mut self) {
        let recreated_textures = match self.renderer.recreate_swapchain(&mut self.frame_graph) {
            Ok(textures) => textures,
            Err(e) => {
                log::error!("Failed to recreate swapchain: {}", e);
                return;
            }
        };

        let extent = self.renderer.swapchain_extent();

        for (name, slot) in recreated_textures {
            if name == "hdr_color" {
                self.frame_graph
                    .set_tonemap_texture_index(self.pass_ids.tonemap, slot)
                    .expect("Failed to update tonemap texture index");
            } else if name == "viewport_0" {
                self.on_viewport_texture_recreated(slot);
            }
        }

        for frame_idx in 0..2 {
            if let Some(view) = self
                .frame_graph
                .as_vulkan()
                .transient_texture_view_for_frame("shadow_atlas", frame_idx)
            {
                self.renderer
                    .unwrap_vulkan()
                    .set_shadow_atlas_view(frame_idx, view);
            }
        }

        let aspect = extent.width as f32 / extent.height as f32;
        self.camera.aspect_ratio_changed(&mut self.world, aspect);
    }
}

#[cfg(feature = "editor")]
impl Application {
    /// Prepare editor draw lists: gizmo draws, shadow filtering, outline selection.
    fn prepare_editor_draw_lists(
        &mut self,
        draw_list: &mut katla_gfx::renderer::DrawList,
    ) -> (
        katla_gfx::renderer::DrawList,
        Option<katla_gfx::renderer::DrawList>,
    ) {
        self.collect_gizmo_draw_calls(draw_list);

        let shadow_draw_list = {
            let draws = draw_list
                .iter()
                .filter(|dc| dc.material != self.editor.billboard_resources.material)
                .cloned()
                .collect::<Vec<_>>();
            katla_gfx::renderer::DrawList { draws }
        };

        let selected_outline_indices = self
            .editor
            .editor_ui
            .selected_entity
            .map(|entity| self.collect_selected_instance_indices(entity));

        let outline_draw_list = selected_outline_indices.as_ref().map(|indices| {
            let draws = draw_list
                .iter()
                .filter(|dc| indices.contains(&dc.instance_index))
                .cloned()
                .collect::<Vec<_>>();
            katla_gfx::renderer::DrawList { draws }
        });

        (shadow_draw_list, outline_draw_list)
    }

    /// Collect instance indices for the selected entity and all its children.
    ///
    /// Used to build the filtered draw list for the outline pass.
    fn collect_selected_instance_indices(&self, root_entity: katla_ecs::EntityId) -> Vec<u32> {
        use crate::components::Children;

        let mut entity_set = std::collections::HashSet::new();
        entity_set.insert(root_entity);

        let mut queue = vec![root_entity];
        while let Some(entity) = queue.pop() {
            if let Some(children) = self.world.get_component::<Children>(entity) {
                for &child in &children.children {
                    if entity_set.insert(child) {
                        queue.push(child);
                    }
                }
            }
        }

        let mut indices = Vec::new();
        for entity_id in &entity_set {
            if let Some(entity_indices) = self.editor.entity_to_instance_indices.get(entity_id) {
                indices.extend_from_slice(entity_indices);
            }
        }
        indices
    }

    /// Generate gizmo draw calls and append them to the main draw list.
    fn collect_gizmo_draw_calls(&mut self, draw_list: &mut katla_gfx::renderer::DrawList) {
        use crate::components::{PerspectiveComponent, TransformComponent};
        use crate::gizmo::*;

        let Some(entity_id) = self.editor.editor_ui.selected_entity else {
            self.editor.gizmo_state.clear_entity();
            return;
        };

        let Some(transform) = self.world.get_component::<TransformComponent>(entity_id) else {
            self.editor.gizmo_state.clear_entity();
            return;
        };

        if !self.editor.gizmo_resources.initialized {
            return;
        }

        let position = transform.transform.position;
        self.editor.gizmo_state.set_entity(entity_id, position);

        // Get camera FOV and viewport height for screen-space scaling
        let fov = if let Some(proj) = self
            .world
            .get_component::<PerspectiveComponent>(self.camera.entity)
        {
            proj.fov
        } else {
            60.0
        };

        let viewport_height = self.editor.editor_ui.viewport_size().1 as f32;
        let cam_pos = if let Some(t) = self
            .world
            .get_component::<TransformComponent>(self.camera.entity)
        {
            t.transform.position
        } else {
            katla_math::Vec3::new(0.0, 2.0, 10.0)
        };

        let fov_rad = fov.to_radians();
        let desired_screen_size = 120.0; // pixels
        let gizmo_scale = compute_gizmo_scale(
            cam_pos,
            position,
            fov_rad,
            viewport_height,
            desired_screen_size,
        );

        // Allocate instance indices starting after existing draws
        let mut next_instance = draw_list
            .iter()
            .map(|d| d.instance_index)
            .max()
            .unwrap_or(0)
            + 1;

        let gizmo_draws = match self.editor.gizmo_state.mode {
            GizmoMode::Translate => generate_translate_draw_calls(
                &self.editor.gizmo_resources,
                position,
                gizmo_scale,
                self.editor.gizmo_state.hovered_handle,
                self.editor.gizmo_state.active_handle,
                &mut next_instance,
            ),
            GizmoMode::Rotate => generate_rotate_draw_calls(
                &self.editor.gizmo_resources,
                position,
                gizmo_scale,
                self.editor.gizmo_state.hovered_handle,
                self.editor.gizmo_state.active_handle,
                &mut next_instance,
            ),
            GizmoMode::Scale => generate_scale_draw_calls(
                &self.editor.gizmo_resources,
                position,
                gizmo_scale,
                self.editor.gizmo_state.hovered_handle,
                self.editor.gizmo_state.active_handle,
                &mut next_instance,
            ),
        };

        for draw in gizmo_draws {
            draw_list.push(draw);
        }
    }

    /// Generate billboard draw calls for entities with BillboardComponent.
    fn collect_billboard_draw_calls(&mut self, draw_list: &mut katla_gfx::renderer::DrawList) {
        use crate::components::{
            BillboardComponent, EditorHidden, PerspectiveComponent, TransformComponent,
        };
        use crate::gizmo::compute_gizmo_scale;
        use katla_gfx::renderer::DrawCall;
        use katla_math::Mat4;

        if !self.editor.billboard_resources.initialized {
            return;
        }

        let cam_entity = self.camera.entity;

        let (cam_pos, fov) = {
            let cam_pos = self
                .world
                .get_component::<TransformComponent>(cam_entity)
                .map(|t| t.transform.position)
                .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));
            let fov = self
                .world
                .get_component::<PerspectiveComponent>(cam_entity)
                .map(|p| p.fov)
                .unwrap_or(60.0);
            (cam_pos, fov)
        };

        let viewport_height = self.editor.editor_ui.viewport_size().1 as f32;
        let fov_rad = fov.to_radians();

        let mut next_instance = draw_list
            .iter()
            .map(|d| d.instance_index)
            .max()
            .unwrap_or(0)
            + 1;

        for (entity_id, billboard) in self.world.query_ref::<&BillboardComponent>() {
            if self
                .world
                .get_component::<EditorHidden>(entity_id)
                .is_some()
            {
                continue;
            }

            let Some(transform) = self.world.get_component::<TransformComponent>(entity_id) else {
                continue;
            };

            let position = transform.transform.position;

            let Some(texture_handle) = self
                .editor
                .billboard_resources
                .icon_textures
                .get(&billboard.icon)
            else {
                continue;
            };
            let bindless_idx = self
                .renderer
                .get_bindless_slot(*texture_handle)
                .unwrap_or(0);

            let desired_screen_size = 40.0 * billboard.size;
            let world_scale = compute_gizmo_scale(
                cam_pos,
                position,
                fov_rad,
                viewport_height,
                desired_screen_size,
            );

            let transform_mat = Mat4::from_translation([position.x(), position.y(), position.z()])
                * Mat4::from_scale(katla_math::Vec3::new(world_scale, world_scale, world_scale));

            let idx = next_instance;
            next_instance += 1;

            let color = billboard.color.to_linear();

            let draw = DrawCall::new(
                self.editor.billboard_resources.mesh,
                self.editor.billboard_resources.material,
            )
            .with_transform(transform_mat.to_array())
            .with_color(color.to_array())
            .with_instance_index(idx)
            .with_emission(bindless_idx as f32)
            .with_billboard();

            draw_list.push(draw);

            self.editor.entity_instance_map.insert(idx, entity_id);
            self.editor
                .entity_to_instance_indices
                .entry(entity_id)
                .or_default()
                .push(idx);
        }
    }
}

#[cfg(target_os = "macos")]
impl Application {
    pub fn render_frame(
        &mut self,
        ui_draw_list: Option<katla_gfx::renderer::UIDrawList>,
        delta_time: f32,
        frame_count: usize,
    ) {
        let _ = (delta_time, frame_count);

        if self.needs_swapchain_recreate {
            self.needs_swapchain_recreate = false;
            let (w, h) = self.viewport_size();
            if w > 0 && h > 0 {
                if let Err(e) = self.renderer.wait_for_frame() {
                    log::error!("Failed to wait for GPU before resize: {}", e);
                }
                if let Err(e) = self.renderer.resize(w, h) {
                    log::error!("Failed to resize Metal renderer: {}", e);
                }

                let phys = self.renderer.swapchain_extent();
                if let Ok(textures) = self.frame_graph.recreate_transient_textures(
                    &mut self.renderer,
                    phys.width,
                    phys.height,
                ) {
                    for (name, slot) in &textures {
                        if name == "hdr_color" {
                            self.frame_graph
                                .set_tonemap_texture_index(self.pass_ids.tonemap, *slot)
                                .ok();
                        } else if name == "viewport_0" {
                            self.on_viewport_texture_recreated(*slot);
                            self.renderer.set_viewport_bindless_slot(*slot);
                        }
                    }

                    if let Some(view) = self.frame_graph.transient_image_view_metal("hdr_color", 0)
                    {
                        let hdr_transient_slot = self
                            .frame_graph
                            .transient_texture_metal("hdr_color", 0)
                            .and_then(|t| t.bindless_slot)
                            .unwrap_or(0);
                        self.renderer
                            .set_geometry_hdr_view(view, hdr_transient_slot);
                    }

                    if let Some(view) = self.frame_graph.transient_image_view_metal("viewport_0", 0)
                    {
                        self.renderer.set_tonemap_output_view(view);
                    }
                }
            }
        }

        let (viewport_width, viewport_height) = self.viewport_size();
        let viewport_aspect = if viewport_height > 0 {
            viewport_width as f32 / viewport_height as f32
        } else {
            16.0 / 9.0
        };
        self.camera
            .aspect_ratio_changed(&mut self.world, viewport_aspect);

        let mut frame = FrameContext::new();

        let view_mat = self.camera.get_view_mat(&self.world);
        let proj_mat = self.camera.get_proj_mat(&self.world);
        let frustum = katla_math::Frustum::from_proj_and_view(&proj_mat, &view_mat);
        let camera_entity = self.camera.entity;

        use crate::components::TransformComponent;
        let cam_pos = if let Some(transform) = self
            .world
            .get_component::<TransformComponent>(camera_entity)
        {
            [
                transform.transform.position.x(),
                transform.transform.position.y(),
                transform.transform.position.z(),
                1.0,
            ]
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let inv_view_proj = {
            use katla_math::Mat4;
            (proj_mat * view_mat)
                .inverse()
                .unwrap_or_else(Mat4::identity)
        };

        let extent = self.renderer.swapchain_extent();
        let tiles_x = extent.width.div_ceil(16);
        let tiles_y = extent.height.div_ceil(16);

        let frame_uniforms = FrameUniforms {
            view_matrix: view_mat.to_array(),
            proj_matrix: proj_mat.to_array(),
            inv_view_proj_matrix: inv_view_proj.to_array(),
            camera_position: cam_pos,
            light_direction: [0.3, 1.0, 0.2, 0.0],
            light_color: [1.0, 0.98, 0.95, 0.0],
            light_intensity: [
                1.0,
                self.renderer
                    .depth_texture_base_index()
                    .map(|base| base + self.renderer.current_frame() as u32)
                    .unwrap_or(0) as f32,
                0.0,
                0.0,
            ],
            tiles: [tiles_x, tiles_y, 0, 0],
            tonemap: [1.0, 2.2, 0.0, 0.0],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        };
        frame.set_frame_uniforms(frame_uniforms.clone());

        self.collect_draws_with_context(&mut frame, &frustum);

        self.collect_and_upload_lights();

        self.renderer
            .set_frame_uniforms(frame.frame_uniforms().clone());

        self.renderer.update_shadows([
            frame_uniforms.light_direction[0],
            frame_uniforms.light_direction[1],
            frame_uniforms.light_direction[2],
        ]);

        self.renderer.upload_shadow_cascades();

        let mut draw_list = frame.take_draw_list();
        draw_list.sort_by_material();

        if let Err(e) = self.renderer.execute_draw_calls(&draw_list) {
            log::error!("Failed to execute draw calls: {}", e);
            return;
        }

        log::debug!(
            "About to submit {} draw calls to Metal renderer",
            draw_list.len()
        );

        let (shadow_draw_list, outline_draw_list) = self.prepare_draw_lists(&mut draw_list);

        if let Err(e) = self.renderer.render(&mut self.frame_graph, |frame| {
            let ids = &self.pass_ids;

            if !draw_list.is_empty() {
                frame.submit(ids.geometry, &draw_list);
                frame.submit(ids.shadow, &shadow_draw_list);
                frame.submit(ids.depth_prepass, &draw_list);
                log::debug!(
                    "Submitted {} draw calls to geometry + shadow + depth_prepass",
                    draw_list.len()
                );
            }

            if let Some(ref outline_dl) = outline_draw_list
                && !outline_dl.is_empty()
            {
                frame.submit(ids.outline, outline_dl);
                log::debug!(
                    "Submitted {} selected draw calls to outline pass",
                    outline_dl.len()
                );
            }

            if let Some(ref ui_list) = ui_draw_list {
                log::debug!("Submitting {} UI draw commands", ui_list.commands.len());
                frame.submit_ui(ids.ui, ui_list);
            }
        }) {
            log::error!("Metal frame render failed: {}", e);
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
                range: point_light.range,
                color: point_light.color,
                intensity: point_light.intensity,
            });
        }

        if !lights.is_empty() {
            log::debug!(
                "Uploading {} point lights for Metal Forward+ culling",
                lights.len()
            );
        }
        self.renderer.upload_lights(&lights);
    }
}
