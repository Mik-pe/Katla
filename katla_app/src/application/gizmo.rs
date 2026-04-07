use log::info;

use crate::application::Application;
use crate::gizmo::*;
use katla_math::Vec2;

impl Application {
    /// Initialize GPU resources for the 3D gizmo (meshes + material).
    #[cfg(feature = "editor")]
    pub(crate) fn init_gizmo_resources(&mut self) {
        use crate::gizmo::GizmoResources;

        let shaft_mesh = self.renderer.create_cylinder_mesh(1.0, 0.05, 16);
        let cone_mesh = self.renderer.create_cone_mesh(1.0, 0.5, 16);
        let cube_mesh = self.renderer.create_cube_mesh([1.0, 1.0, 1.0]);
        let ring_mesh = self.renderer.create_torus_mesh(0.5, 0.02, 48, 24);

        let unlit_shader_path = self.resources.shader_path("unlit.wgsl");
        let material = self
            .renderer
            .compile_material(
                &unlit_shader_path,
                katla_gfx::MaterialOptions {
                    vertex_type: katla_gfx::VertexType::Pbr,
                    color_format: katla_gfx::ImageFormat::R16G16B16A16Sfloat,
                    depth_test: false,
                    ..Default::default()
                },
            )
            .expect("Failed to create gizmo unlit material");

        self.gpu_resource_tracker.set_protected_material(material);

        self.gizmo_resources = GizmoResources {
            shaft_mesh,
            cone_mesh,
            cube_mesh,
            ring_mesh,
            material,
            initialized: true,
        };

        info!("Gizmo GPU resources initialized");
    }

    /// Hit-test gizmo axes at the given screen position.
    ///
    /// Returns the hit axis, or None if no axis is close enough to the mouse.
    #[cfg(feature = "editor")]
    pub(crate) fn hit_test_gizmo(&self, mouse_pos: Vec2) -> Option<GizmoAxis> {
        use crate::components::PerspectiveComponent;

        if self.gizmo_state.entity.is_none() || !self.gizmo_resources.initialized {
            return None;
        }

        let vp = &self.editor_ui.last_viewport_bounds;
        let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

        if !vp.contains(mouse_pos) {
            return None;
        }

        let camera = self.camera.borrow();
        let view_mat = camera.get_view_mat(&self.world);
        let proj_mat = camera.get_proj_mat(&self.world);
        drop(camera);

        let fov = self
            .world
            .get_component::<PerspectiveComponent>(self.camera.borrow().entity)
            .map(|p| p.fov)
            .unwrap_or(60.0);

        let viewport_height = self.editor_ui.viewport_size().1 as f32;
        let cam_pos = self
            .world
            .get_component::<crate::components::TransformComponent>(self.camera.borrow().entity)
            .map(|t| t.transform.position)
            .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));

        let gizmo_scale = compute_gizmo_scale(
            cam_pos,
            self.gizmo_state.origin,
            fov.to_radians(),
            viewport_height,
            120.0,
        );

        hit_test_axes(
            (mouse_pos.x(), mouse_pos.y()),
            self.gizmo_state.origin,
            gizmo_scale,
            &view_mat,
            &proj_mat,
            viewport,
            self.gizmo_state.mode,
            12.0, // pixel threshold
        )
    }

    /// Begin dragging a gizmo axis.
    #[cfg(feature = "editor")]
    pub(crate) fn begin_gizmo_drag(&mut self, axis: GizmoAxis, mouse_pos: Vec2) {
        if let Some(entity_id) = self.gizmo_state.entity {
            let entity_pos = self
                .world
                .get_component::<crate::components::TransformComponent>(entity_id)
                .map(|t| t.transform.position)
                .unwrap_or(self.gizmo_state.origin);

            // Compute a world-space reference point on the drag plane
            let vp = &self.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());
            let camera = self.camera.borrow();
            let view_mat = camera.get_view_mat(&self.world);
            let proj_mat = camera.get_proj_mat(&self.world);
            drop(camera);

            let (ray_origin, ray_dir) = screen_to_ray(
                (mouse_pos.x(), mouse_pos.y()),
                viewport,
                &view_mat,
                &proj_mat,
            );
            {
                // Compute camera forward for the drag plane
                let _cam_pos = self
                    .world
                    .get_component::<crate::components::TransformComponent>(
                        self.camera.borrow().entity,
                    )
                    .map(|t| t.transform.position)
                    .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));
                let cam_rot = self.camera.borrow().get_view_rotation(&self.world);
                let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

                if let Some(delta) =
                    compute_translate_delta(axis, ray_origin, ray_dir, entity_pos, camera_forward)
                {
                    // Store the initial world position on the plane (not the entity position)
                    let world_pos = entity_pos + delta;
                    self.gizmo_state.begin_drag(axis, world_pos, entity_pos);
                } else {
                    self.gizmo_state.begin_drag(axis, entity_pos, entity_pos);
                }

                // Store initial rotation/scale for rotate/scale modes
                if let Some(transform) = self
                    .world
                    .get_component::<crate::components::TransformComponent>(entity_id)
                {
                    let euler = transform.transform.rotation.to_euler();
                    self.gizmo_state.drag_start_rotation = Some(euler);
                    self.gizmo_state.drag_start_scale = Some(transform.transform.scale);
                    self.gizmo_state.drag_rotation_accum = katla_math::Vec3::new(0.0, 0.0, 0.0);
                }
            }
        }
    }

    /// Update gizmo interaction on mouse move: hover highlight and drag application.
    #[cfg(feature = "editor")]
    pub(crate) fn update_gizmo_interaction(&mut self, mouse_pos: Vec2) {
        // Store previous screen position for rotation delta
        let prev_screen = self.prev_mouse_screen;
        let current_screen = (mouse_pos.x(), mouse_pos.y());
        self.prev_mouse_screen = Some(current_screen);

        if self.gizmo_state.is_dragging() {
            // Apply the drag based on the current mode
            let Some(entity_id) = self.gizmo_state.entity else {
                return;
            };

            let Some(axis) = self.gizmo_state.active_axis else {
                return;
            };

            let vp = &self.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

            if !vp.contains(mouse_pos) {
                return;
            }

            let camera = self.camera.borrow();
            let view_mat = camera.get_view_mat(&self.world);
            let proj_mat = camera.get_proj_mat(&self.world);
            drop(camera);

            let cam_rot = self.camera.borrow().get_view_rotation(&self.world);
            let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

            let (ray_origin, ray_dir) =
                screen_to_ray(current_screen, viewport, &view_mat, &proj_mat);
            {
                if let Some(transform) = self
                    .world
                    .get_component_mut::<crate::components::TransformComponent>(entity_id)
                {
                    match self.gizmo_state.mode {
                        GizmoMode::Translate => {
                            if let Some(start_origin) = self.gizmo_state.drag_start_origin
                                && let Some(delta) = compute_translate_delta(
                                    axis,
                                    ray_origin,
                                    ray_dir,
                                    start_origin,
                                    camera_forward,
                                )
                            {
                                transform.transform.position = start_origin + delta;
                                self.gizmo_state.origin = transform.transform.position;
                            }
                        }
                        GizmoMode::Rotate => {
                            if let Some(prev) = prev_screen {
                                // Project gizmo origin to screen space for rotation center
                                let origin_screen = world_to_screen(
                                    self.gizmo_state.origin,
                                    &view_mat,
                                    &proj_mat,
                                    viewport,
                                );

                                if let Some(center) = origin_screen {
                                    let delta =
                                        compute_rotate_delta(axis, center, current_screen, prev);
                                    self.gizmo_state.drag_rotation_accum = katla_math::Vec3::new(
                                        self.gizmo_state.drag_rotation_accum.x()
                                            + if axis == GizmoAxis::X { delta } else { 0.0 },
                                        self.gizmo_state.drag_rotation_accum.y()
                                            + if axis == GizmoAxis::Y { delta } else { 0.0 },
                                        self.gizmo_state.drag_rotation_accum.z()
                                            + if axis == GizmoAxis::Z { delta } else { 0.0 },
                                    );

                                    if let Some((start_pitch, start_yaw, start_roll)) =
                                        self.gizmo_state.drag_start_rotation
                                    {
                                        let new_pitch =
                                            start_pitch + self.gizmo_state.drag_rotation_accum.x();
                                        let new_yaw =
                                            start_yaw + self.gizmo_state.drag_rotation_accum.y();
                                        let new_roll =
                                            start_roll + self.gizmo_state.drag_rotation_accum.z();
                                        transform.transform.rotation = katla_math::Quat::from_euler(
                                            new_pitch, new_yaw, new_roll,
                                        );
                                    }
                                }
                            }
                        }
                        GizmoMode::Scale => {
                            if let Some(start_origin) = self.gizmo_state.drag_start_origin
                                && let Some(axis_dist) = compute_scale_delta(
                                    axis,
                                    ray_origin,
                                    ray_dir,
                                    start_origin,
                                    camera_forward,
                                )
                                && let Some(start_scale) = self.gizmo_state.drag_start_scale
                            {
                                let axis_idx = match axis {
                                    GizmoAxis::X => 0,
                                    GizmoAxis::Y => 1,
                                    GizmoAxis::Z => 2,
                                };
                                // Store the initial axis distance on the first drag frame
                                // to compute relative scale from the drag start
                                if self.gizmo_state.drag_start_world.is_none() {
                                    self.gizmo_state.drag_start_world =
                                        Some(katla_math::Vec3::new(axis_dist, 0.0, 0.0));
                                }
                                let initial_dist = self.gizmo_state.drag_start_world.unwrap().x();
                                // Scale relative to drag start: ratio of current distance to initial distance
                                let scale_factor = if initial_dist.abs() > 1e-6 {
                                    axis_dist / initial_dist
                                } else {
                                    1.0 + axis_dist * 0.01
                                };
                                let mut scale = [start_scale.x(), start_scale.y(), start_scale.z()];
                                scale[axis_idx] = (scale[axis_idx] * scale_factor).max(0.01);
                                transform.transform.scale =
                                    katla_math::Vec3::new(scale[0], scale[1], scale[2]);
                            }
                        }
                    }
                }
            }
        } else if self.gizmo_state.entity.is_some() {
            // Update hover highlight
            self.gizmo_state.hovered_axis = self.hit_test_gizmo(mouse_pos);
        }
    }
}
