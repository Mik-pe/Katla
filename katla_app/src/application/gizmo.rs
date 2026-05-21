use log::info;
use winit::event::ElementState;
use winit::keyboard::KeyCode;

use crate::application::Application;
use crate::gizmo::*;
use katla_gfx::GpuRenderer;
use katla_math::Vec2;

#[cfg(feature = "editor")]
impl Application {
    /// Initialize GPU resources for the 3D gizmo (meshes + material).
    pub(crate) fn init_gizmo_resources(&mut self) {
        use crate::gizmo::GizmoResources;

        let shaft_mesh = self.renderer.create_cylinder_mesh(1.0, 0.05, 16);
        let cone_mesh = self.renderer.create_cone_mesh(1.0, 0.5, 16);
        let cube_mesh = self.renderer.create_cube_mesh([1.0, 1.0, 1.0]);
        let ring_mesh = self.renderer.create_torus_mesh(0.5, 0.02, 48, 24);

        let unlit_shader_path = self.resources.shader_path("unlit.wgsl");
        #[cfg(feature = "vulkan")]
        let material = self
            .renderer
            .unwrap_vulkan()
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
        #[cfg(not(feature = "vulkan"))]
        let material = self
            .renderer
            .compile_material(&unlit_shader_path.to_string_lossy(), "pbr")
            .expect("Failed to create gizmo unlit material");

        self.gpu_resource_tracker.set_protected_material(material);

        self.editor.gizmo_resources = GizmoResources {
            shaft_mesh,
            cone_mesh,
            cube_mesh,
            ring_mesh,
            material,
            initialized: true,
        };

        info!("Gizmo GPU resources initialized");
    }

    /// Hit-test gizmo handles at the given screen position.
    ///
    /// Returns the hit handle (axis or plane), or None if nothing is close enough.
    pub(crate) fn hit_test_gizmo(&self, mouse_pos: Vec2) -> Option<GizmoHandle> {
        use crate::components::PerspectiveComponent;

        if self.editor.gizmo_state.entity.is_none() || !self.editor.gizmo_resources.initialized {
            return None;
        }

        let vp = &self.editor.editor_ui.last_viewport_bounds;
        let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

        if !vp.contains(mouse_pos) {
            return None;
        }

        let view_mat = self.camera.get_view_mat(&self.world);
        let proj_mat = self.camera.get_proj_mat(&self.world);

        let fov = self
            .world
            .get_component::<PerspectiveComponent>(self.camera.entity)
            .map(|p| p.fov)
            .unwrap_or(60.0);

        let viewport_height = self.editor.editor_ui.viewport_size().1 as f32;
        let cam_pos = self
            .world
            .get_component::<crate::components::TransformComponent>(self.camera.entity)
            .map(|t| t.transform.position)
            .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));

        let gizmo_scale = compute_gizmo_scale(
            cam_pos,
            self.editor.gizmo_state.origin,
            fov.to_radians(),
            viewport_height,
            120.0,
        );

        hit_test_axes(&crate::gizmo::HitTestParams {
            mouse_screen: (mouse_pos.x(), mouse_pos.y()),
            gizmo_origin: self.editor.gizmo_state.origin,
            gizmo_scale,
            view_matrix: &view_mat,
            proj_matrix: &proj_mat,
            viewport,
            mode: self.editor.gizmo_state.mode,
            pixel_threshold: 12.0,
        })
    }

    /// Begin dragging a gizmo handle (axis or plane).
    pub(crate) fn begin_gizmo_drag(&mut self, handle: GizmoHandle, mouse_pos: Vec2) {
        if let Some(entity_id) = self.editor.gizmo_state.entity {
            let entity_pos = self
                .world
                .get_component::<crate::components::TransformComponent>(entity_id)
                .map(|t| t.transform.position)
                .unwrap_or(self.editor.gizmo_state.origin);

            // Compute a world-space reference point on the drag plane
            let vp = &self.editor.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());
            let view_mat = self.camera.get_view_mat(&self.world);
            let proj_mat = self.camera.get_proj_mat(&self.world);

            let (ray_origin, ray_dir) = screen_to_ray(
                (mouse_pos.x(), mouse_pos.y()),
                viewport,
                &view_mat,
                &proj_mat,
            );
            {
                let cam_rot = self.camera.get_view_rotation(&self.world);
                let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

                let world_pos = match handle {
                    GizmoHandle::Axis(axis) => {
                        if let Some(delta) = compute_translate_delta(
                            axis,
                            ray_origin,
                            ray_dir,
                            entity_pos,
                            camera_forward,
                        ) {
                            entity_pos + delta
                        } else {
                            entity_pos
                        }
                    }
                    GizmoHandle::Plane(plane) => {
                        if let Some(delta) =
                            compute_translate_plane_delta(plane, ray_origin, ray_dir, entity_pos)
                        {
                            entity_pos + delta
                        } else {
                            entity_pos
                        }
                    }
                };

                self.editor
                    .gizmo_state
                    .begin_drag(handle, world_pos, entity_pos);

                // Store initial rotation/scale for rotate/scale modes
                if let Some(transform) = self
                    .world
                    .get_component::<crate::components::TransformComponent>(entity_id)
                {
                    let euler = transform.transform.rotation.to_euler();
                    self.editor.gizmo_state.drag_start_rotation = Some(euler);
                    self.editor.gizmo_state.drag_start_scale = Some(transform.transform.scale);
                    self.editor.gizmo_state.drag_rotation_accum =
                        katla_math::Vec3::new(0.0, 0.0, 0.0);
                }
            }
        }
    }

    /// Update gizmo interaction on mouse move: hover highlight and drag application.
    pub(crate) fn update_gizmo_interaction(&mut self, mouse_pos: Vec2) {
        // Store previous screen position for rotation delta
        let prev_screen = self.editor.prev_mouse_screen;
        let current_screen = (mouse_pos.x(), mouse_pos.y());
        self.editor.prev_mouse_screen = Some(current_screen);

        if self.editor.gizmo_state.is_dragging() {
            // Apply the drag based on the current mode
            let Some(entity_id) = self.editor.gizmo_state.entity else {
                return;
            };

            let Some(active_handle) = self.editor.gizmo_state.active_handle else {
                return;
            };

            let vp = &self.editor.editor_ui.last_viewport_bounds;
            let viewport = (vp.min.x(), vp.min.y(), vp.width(), vp.height());

            if !vp.contains(mouse_pos) {
                return;
            }

            let view_mat = self.camera.get_view_mat(&self.world);
            let proj_mat = self.camera.get_proj_mat(&self.world);

            let cam_rot = self.camera.get_view_rotation(&self.world);
            let camera_forward = cam_rot * katla_math::Vec3::new(0.0, 0.0, -1.0);

            let (ray_origin, ray_dir) =
                screen_to_ray(current_screen, viewport, &view_mat, &proj_mat);

            // Precompute zoom-aware scale sensitivity for the fallback path (initial_dist ≈ 0)
            let scale_fallback_sensitivity = {
                let fov = self
                    .world
                    .get_component::<crate::components::PerspectiveComponent>(self.camera.entity)
                    .map(|p| p.fov)
                    .unwrap_or(60.0);
                let viewport_height = self.editor.editor_ui.viewport_size().1 as f32;
                let cam_pos = self
                    .world
                    .get_component::<crate::components::TransformComponent>(self.camera.entity)
                    .map(|t| t.transform.position)
                    .unwrap_or(katla_math::Vec3::new(0.0, 2.0, 10.0));
                let gs = compute_gizmo_scale(
                    cam_pos,
                    self.editor.gizmo_state.origin,
                    fov.to_radians(),
                    viewport_height,
                    120.0,
                );
                1.0 / (gs * 5.0)
            };

            {
                if let Some(transform) = self
                    .world
                    .get_component_mut::<crate::components::TransformComponent>(entity_id)
                {
                    match self.editor.gizmo_state.mode {
                        GizmoMode::Translate => {
                            if let Some(start_origin) = self.editor.gizmo_state.drag_start_origin {
                                let delta = match active_handle {
                                    GizmoHandle::Axis(axis) => compute_translate_delta(
                                        axis,
                                        ray_origin,
                                        ray_dir,
                                        start_origin,
                                        camera_forward,
                                    ),
                                    GizmoHandle::Plane(plane) => compute_translate_plane_delta(
                                        plane,
                                        ray_origin,
                                        ray_dir,
                                        start_origin,
                                    ),
                                };
                                if let Some(delta) = delta {
                                    transform.transform.position = start_origin + delta;
                                    self.editor.gizmo_state.origin = transform.transform.position;
                                }
                            }
                        }
                        GizmoMode::Rotate => {
                            if let Some(axis) = active_handle.axis()
                                && let Some(prev) = prev_screen
                            {
                                // Project gizmo origin to screen space for rotation center
                                let origin_screen = world_to_screen(
                                    self.editor.gizmo_state.origin,
                                    &view_mat,
                                    &proj_mat,
                                    viewport,
                                );

                                if let Some(center) = origin_screen {
                                    let delta =
                                        compute_rotate_delta(axis, center, current_screen, prev);
                                    self.editor.gizmo_state.drag_rotation_accum =
                                        katla_math::Vec3::new(
                                            self.editor.gizmo_state.drag_rotation_accum.x()
                                                + if axis == GizmoAxis::X { delta } else { 0.0 },
                                            self.editor.gizmo_state.drag_rotation_accum.y()
                                                + if axis == GizmoAxis::Y { delta } else { 0.0 },
                                            self.editor.gizmo_state.drag_rotation_accum.z()
                                                + if axis == GizmoAxis::Z { delta } else { 0.0 },
                                        );

                                    if let Some((start_pitch, start_yaw, start_roll)) =
                                        self.editor.gizmo_state.drag_start_rotation
                                    {
                                        let new_pitch = start_pitch
                                            + self.editor.gizmo_state.drag_rotation_accum.x();
                                        let new_yaw = start_yaw
                                            + self.editor.gizmo_state.drag_rotation_accum.y();
                                        let new_roll = start_roll
                                            + self.editor.gizmo_state.drag_rotation_accum.z();
                                        transform.transform.rotation = katla_math::Quat::from_euler(
                                            new_pitch, new_yaw, new_roll,
                                        );
                                    }
                                }
                            }
                        }
                        GizmoMode::Scale => {
                            if let Some(start_origin) = self.editor.gizmo_state.drag_start_origin
                                && let Some(start_scale) = self.editor.gizmo_state.drag_start_scale
                            {
                                let mut scale = [start_scale.x(), start_scale.y(), start_scale.z()];

                                match active_handle {
                                    GizmoHandle::Axis(axis) => {
                                        if let Some(axis_dist) = compute_scale_delta(
                                            axis,
                                            ray_origin,
                                            ray_dir,
                                            start_origin,
                                            camera_forward,
                                        ) {
                                            let axis_idx = match axis {
                                                GizmoAxis::X => 0,
                                                GizmoAxis::Y => 1,
                                                GizmoAxis::Z => 2,
                                            };
                                            if self.editor.gizmo_state.drag_start_world.is_none() {
                                                self.editor.gizmo_state.drag_start_world = Some(
                                                    katla_math::Vec3::new(axis_dist, 0.0, 0.0),
                                                );
                                            }
                                            let initial_dist = self
                                                .editor
                                                .gizmo_state
                                                .drag_start_world
                                                .unwrap()
                                                .x();

                                            let scale_factor = if initial_dist.abs() > 1e-6 {
                                                axis_dist / initial_dist
                                            } else {
                                                1.0 + axis_dist * scale_fallback_sensitivity
                                            };
                                            scale[axis_idx] =
                                                (scale[axis_idx] * scale_factor).max(0.01);
                                        }
                                    }
                                    GizmoHandle::Plane(plane) => {
                                        if let Some((d1, d2)) = compute_scale_plane_delta(
                                            plane,
                                            ray_origin,
                                            ray_dir,
                                            start_origin,
                                        ) {
                                            let (a1, a2) = plane.axes();
                                            let idx1 = match a1 {
                                                GizmoAxis::X => 0,
                                                GizmoAxis::Y => 1,
                                                GizmoAxis::Z => 2,
                                            };
                                            let idx2 = match a2 {
                                                GizmoAxis::X => 0,
                                                GizmoAxis::Y => 1,
                                                GizmoAxis::Z => 2,
                                            };

                                            if self.editor.gizmo_state.drag_start_world.is_none() {
                                                self.editor.gizmo_state.drag_start_world =
                                                    Some(katla_math::Vec3::new(d1, d2, 0.0));
                                            }
                                            let init =
                                                self.editor.gizmo_state.drag_start_world.unwrap();

                                            let f1 = if init.x().abs() > 1e-6 {
                                                d1 / init.x()
                                            } else {
                                                1.0 + d1 * scale_fallback_sensitivity
                                            };
                                            let f2 = if init.y().abs() > 1e-6 {
                                                d2 / init.y()
                                            } else {
                                                1.0 + d2 * scale_fallback_sensitivity
                                            };

                                            scale[idx1] = (scale[idx1] * f1).max(0.01);
                                            scale[idx2] = (scale[idx2] * f2).max(0.01);
                                        }
                                    }
                                }

                                transform.transform.scale =
                                    katla_math::Vec3::new(scale[0], scale[1], scale[2]);
                            }
                        }
                    }
                }
            }
        } else if self.editor.gizmo_state.entity.is_some() {
            // Update hover highlight
            self.editor.gizmo_state.hovered_handle = self.hit_test_gizmo(mouse_pos);
        }
    }

    /// Handle editor-specific mouse press: focused panel update, picking, gizmo start.
    pub(crate) fn handle_editor_mouse_press(
        &mut self,
        state: &winit::event::ElementState,
        button: &winit::event::MouseButton,
    ) {
        if let ElementState::Pressed = state {
            let mouse_pos = self.ui_context.input().mouse_pos;
            self.editor
                .editor_ui
                .update_focused_panel_from_click(mouse_pos);

            if *button == winit::event::MouseButton::Left
                && self.editor.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
                && self
                    .editor
                    .editor_ui
                    .last_viewport_bounds
                    .contains(mouse_pos)
                && !self.editor.editor_ui.prev_want_capture_mouse
                && self.ui_context.prev_hover_z_index() == katla_ui::z_index::DEFAULT
            {
                self.editor.gizmo_state.consumed_click = false;

                if let Some(handle) = self.hit_test_gizmo(mouse_pos) {
                    self.begin_gizmo_drag(handle, mouse_pos);
                } else {
                    let vp = self.editor.editor_ui.last_viewport_bounds;
                    let rel_x = mouse_pos.x() - vp.min.x();
                    let rel_y = mouse_pos.y() - vp.min.y();
                    self.editor.pending_pick = Some((self.frame_count, rel_x, rel_y));
                }
            }
        }
    }

    /// Handle editor-specific mouse release: end gizmo drag.
    pub(crate) fn handle_editor_mouse_release(
        &mut self,
        state: &winit::event::ElementState,
        button: &winit::event::MouseButton,
    ) {
        if matches!(state, ElementState::Released)
            && *button == winit::event::MouseButton::Left
            && self.editor.gizmo_state.is_dragging()
        {
            self.editor.gizmo_state.end_drag();
        }
    }

    /// Handle editor keyboard shortcuts: focus entity (F), particle inspector (Ctrl+P), save (Ctrl+S).
    pub(crate) fn handle_editor_keyboard_shortcuts(
        &mut self,
        event: &winit::event::KeyEvent,
        keycode: KeyCode,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }

        if keycode == KeyCode::KeyF
            && self.editor.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport
            && !self.current_modifiers.control_key()
            && !self.current_modifiers.shift_key()
            && !self.current_modifiers.alt_key()
            && let Some(entity_id) = self.editor.editor_ui.selected_entity
        {
            self.focus_camera_on_entity(entity_id);
        }

        if keycode == KeyCode::KeyP && self.current_modifiers.control_key() {
            let state = &mut self.editor.editor_ui.particle_inspector_state;
            if state.panel.is_visible() {
                state.panel.close();
            } else {
                state.panel.open();
            }
            info!(
                "Particle inspector: {}",
                if state.panel.is_visible() {
                    "visible"
                } else {
                    "hidden"
                }
            );
        }

        if keycode == KeyCode::KeyS
            && self.current_modifiers.control_key()
            && !self.current_modifiers.shift_key()
            && !self.current_modifiers.alt_key()
            && !self.editor.editor_ui.prev_want_capture_keyboard
        {
            self.editor
                .editor_ui
                .pending_actions
                .push(crate::ui::EditorAction::SaveScene);
        }

        if keycode == KeyCode::KeyA
            && self.current_modifiers.control_key()
            && self.current_modifiers.shift_key()
            && !self.current_modifiers.alt_key()
        {
            let co_creator = &mut self.editor.editor_ui.co_creator;
            if co_creator.is_open() {
                co_creator.close();
            } else {
                co_creator.open();
            }
        }

        if keycode == KeyCode::KeyZ
            && self.current_modifiers.control_key()
            && !self.current_modifiers.shift_key()
            && !self.current_modifiers.alt_key()
            && !self.editor.editor_ui.prev_want_capture_keyboard
        {
            if self.editor.perform_undo(&mut self.world) {
                info!("Undo performed");
            }
        }

        if keycode == KeyCode::KeyZ
            && self.current_modifiers.control_key()
            && self.current_modifiers.shift_key()
            && !self.current_modifiers.alt_key()
            && !self.editor.editor_ui.prev_want_capture_keyboard
        {
            if self.editor.perform_redo(&mut self.world) {
                info!("Redo performed");
            }
        }
    }

    /// Handle gizmo mode shortcuts (W/E/R) and Escape in viewport.
    pub(crate) fn handle_editor_gizmo_shortcuts(
        &mut self,
        event: &winit::event::KeyEvent,
        keycode: KeyCode,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }
        if self.editor.editor_ui.focused_panel != crate::ui::FocusedPanel::Viewport {
            return;
        }
        if self.ui_context.input().want_capture_keyboard {
            return;
        }

        if keycode == KeyCode::KeyW {
            self.editor
                .gizmo_state
                .set_mode(crate::gizmo::GizmoMode::Translate);
        } else if keycode == KeyCode::KeyE {
            self.editor
                .gizmo_state
                .set_mode(crate::gizmo::GizmoMode::Rotate);
        } else if keycode == KeyCode::KeyR {
            self.editor
                .gizmo_state
                .set_mode(crate::gizmo::GizmoMode::Scale);
        }

        if keycode == KeyCode::Escape {
            event_loop.exit()
        }
    }
}
