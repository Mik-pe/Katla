//! Gizmo types, state, geometry generation, and hit testing.

use katla_ecs::EntityId;
use katla_gfx::renderer::DrawCall;
use katla_gfx::{MaterialHandle, MeshHandle};
use katla_math::{Color, Mat4, Vec3, Vec4};

/// Which gizmo transform mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GizmoMode {
    #[default]
    Translate,
    Rotate,
    Scale,
}

impl GizmoMode {
    pub fn next(self) -> Self {
        match self {
            GizmoMode::Translate => GizmoMode::Rotate,
            GizmoMode::Rotate => GizmoMode::Scale,
            GizmoMode::Scale => GizmoMode::Translate,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GizmoMode::Translate => "Move",
            GizmoMode::Rotate => "Rotate",
            GizmoMode::Scale => "Scale",
        }
    }
}

/// Which axis is being manipulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoAxis {
    X,
    Y,
    Z,
}

impl GizmoAxis {
    pub fn direction(self) -> Vec3 {
        match self {
            GizmoAxis::X => Vec3::new(1.0, 0.0, 0.0),
            GizmoAxis::Y => Vec3::new(0.0, 1.0, 0.0),
            GizmoAxis::Z => Vec3::new(0.0, 0.0, 1.0),
        }
    }

    /// The two axes perpendicular to this one (for plane constraints).
    pub fn perpendicular_axes(self) -> (Vec3, Vec3) {
        match self {
            GizmoAxis::X => (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            GizmoAxis::Y => (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
            GizmoAxis::Z => (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        }
    }

    pub fn color(self) -> GizmoColor {
        match self {
            GizmoAxis::X => GizmoColor::Red,
            GizmoAxis::Y => GizmoColor::Green,
            GizmoAxis::Z => GizmoColor::Blue,
        }
    }
}

/// Gizmo axis colors.
#[derive(Debug, Clone, Copy)]
pub enum GizmoColor {
    Red,
    Green,
    Blue,
    White,
    Yellow,
    Cyan,
}

impl GizmoColor {
    pub fn color(self) -> Color {
        match self {
            GizmoColor::Red => Color::new(0.95, 0.2, 0.2, 1.0),
            GizmoColor::Green => Color::new(0.2, 0.9, 0.2, 1.0),
            GizmoColor::Blue => Color::new(0.3, 0.3, 0.95, 1.0),
            GizmoColor::White => Color::new(0.9, 0.9, 0.9, 1.0),
            GizmoColor::Yellow => Color::new(0.95, 0.9, 0.2, 1.0),
            GizmoColor::Cyan => Color::new(0.2, 0.9, 0.95, 1.0),
        }
    }

    pub fn highlight_color(self) -> Color {
        match self {
            GizmoColor::Red => Color::new(1.0, 0.5, 0.5, 1.0),
            GizmoColor::Green => Color::new(0.5, 1.0, 0.5, 1.0),
            GizmoColor::Blue => Color::new(0.6, 0.6, 1.0, 1.0),
            GizmoColor::White => Color::new(1.0, 1.0, 1.0, 1.0),
            GizmoColor::Yellow => Color::new(1.0, 1.0, 0.5, 1.0),
            GizmoColor::Cyan => Color::new(0.5, 1.0, 1.0, 1.0),
        }
    }
}

/// Gizmo render state: holds GPU resources for the gizmo meshes.
pub struct GizmoResources {
    /// Cylinder mesh for axis shafts.
    pub shaft_mesh: MeshHandle,
    /// Cone mesh for translate axis tips.
    pub cone_mesh: MeshHandle,
    /// Cube mesh for scale axis tips.
    pub cube_mesh: MeshHandle,
    /// Torus mesh for rotation rings.
    pub ring_mesh: MeshHandle,
    /// Material handle for gizmo rendering.
    pub material: MaterialHandle,
    /// Whether resources have been initialized.
    pub initialized: bool,
}

impl Default for GizmoResources {
    fn default() -> Self {
        Self {
            shaft_mesh: MeshHandle::NONE,
            cone_mesh: MeshHandle::NONE,
            cube_mesh: MeshHandle::NONE,
            ring_mesh: MeshHandle::NONE,
            material: MaterialHandle::NONE,
            initialized: false,
        }
    }
}

/// Runtime gizmo state: mode, selection, drag state.
pub struct GizmoState {
    /// Current transform mode.
    pub mode: GizmoMode,
    /// Currently hovered axis (for highlight feedback).
    pub hovered_axis: Option<GizmoAxis>,
    /// Currently dragged axis (during mouse drag).
    pub active_axis: Option<GizmoAxis>,
    /// World-space position of the gizmo origin (selected entity position).
    pub origin: Vec3,
    /// Entity being manipulated.
    pub entity: Option<EntityId>,
    /// World-space mouse position when drag started.
    pub drag_start_world: Option<Vec3>,
    /// Entity position when drag started.
    pub drag_start_origin: Option<Vec3>,
    /// Entity rotation euler angles when drag started (for rotate mode).
    pub drag_start_rotation: Option<(f32, f32, f32)>,
    /// Entity scale when drag started (for scale mode).
    pub drag_start_scale: Option<Vec3>,
    /// Cumulative rotation applied during current drag (for rotate mode).
    pub drag_rotation_accum: Vec3,
    /// Whether the gizmo consumed the mouse click (prevents camera orbit).
    pub consumed_click: bool,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Translate,
            hovered_axis: None,
            active_axis: None,
            origin: Vec3::new(0.0, 0.0, 0.0),
            entity: None,
            drag_start_world: None,
            drag_start_origin: None,
            drag_start_rotation: None,
            drag_start_scale: None,
            drag_rotation_accum: Vec3::new(0.0, 0.0, 0.0),
            consumed_click: false,
        }
    }
}

impl GizmoState {
    pub fn set_mode(&mut self, mode: GizmoMode) {
        if self.mode != mode {
            self.mode = mode;
            self.active_axis = None;
            self.hovered_axis = None;
        }
    }

    /// Update the gizmo origin from the selected entity's position.
    pub fn set_entity(&mut self, entity_id: EntityId, position: Vec3) {
        self.entity = Some(entity_id);
        self.origin = position;
        self.active_axis = None;
        self.hovered_axis = None;
    }

    pub fn clear_entity(&mut self) {
        self.entity = None;
        self.active_axis = None;
        self.hovered_axis = None;
    }

    /// Check if the gizmo is currently being dragged.
    pub fn is_dragging(&self) -> bool {
        self.active_axis.is_some()
    }

    /// Begin dragging an axis.
    pub fn begin_drag(&mut self, axis: GizmoAxis, world_pos: Vec3, entity_pos: Vec3) {
        self.active_axis = Some(axis);
        self.drag_start_world = Some(world_pos);
        self.drag_start_origin = Some(entity_pos);
        self.consumed_click = true;
    }

    /// End the current drag.
    pub fn end_drag(&mut self) {
        self.active_axis = None;
        self.drag_start_world = None;
        self.drag_start_origin = None;
        self.drag_start_rotation = None;
        self.drag_start_scale = None;
        self.drag_rotation_accum = Vec3::new(0.0, 0.0, 0.0);
    }
}

/// Compute the world-space scale for the gizmo so it appears at a constant
/// screen-space size regardless of camera distance.
///
/// `gizmo_size` is the desired pixel size on screen. The function uses the
/// camera's vertical field of view and the viewport height to compute
/// the corresponding world-space distance.
pub fn compute_gizmo_scale(
    camera_position: Vec3,
    gizmo_origin: Vec3,
    fov_rad: f32,
    viewport_height_pixels: f32,
    desired_screen_size: f32,
) -> f32 {
    let distance = (camera_position - gizmo_origin).length();
    if distance < 0.001 {
        return desired_screen_size * 0.01;
    }

    // World-space height of the view frustum at the gizmo's distance
    let frustum_height = 2.0 * distance * (fov_rad * 0.5).tan();

    // Scale factor: how many world units per pixel at this distance
    let world_units_per_pixel = frustum_height / viewport_height_pixels;

    desired_screen_size * world_units_per_pixel
}

/// Compute a ray from screen coordinates through the camera.
///
/// Returns (ray_origin, ray_direction) in world space.
pub fn screen_to_ray(
    screen_pos: (f32, f32),
    viewport: (f32, f32, f32, f32), // x, y, width, height
    view_matrix: &Mat4,
    proj_matrix: &Mat4,
) -> (Vec3, Vec3) {
    let vp = *proj_matrix * *view_matrix;
    let inv_vp = vp.inverse();

    // Convert screen pos to NDC (-1 to 1)
    let ndc_x = ((screen_pos.0 - viewport.0) / viewport.2) * 2.0 - 1.0;
    let ndc_y = ((screen_pos.1 - viewport.1) / viewport.3) * 2.0 - 1.0;

    // Unproject two points at near and far planes
    // Reverse-Z infinite projection: ndc_z=1 is near plane, ndc_z=0 is infinity
    let near_clip = inv_vp * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let far_clip = inv_vp * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);

    let near = Vec3::new(
        near_clip.x() / near_clip.w(),
        near_clip.y() / near_clip.w(),
        near_clip.z() / near_clip.w(),
    );
    let far = Vec3::new(
        far_clip.x() / far_clip.w(),
        far_clip.y() / far_clip.w(),
        far_clip.z() / far_clip.w(),
    );

    let ray_origin = near;
    let ray_dir = (far - near).normalize();

    (ray_origin, ray_dir)
}

/// Intersect a ray with an infinite plane defined by a point and normal.
///
/// Returns the intersection point, or None if the ray is parallel to the plane.
pub fn ray_plane_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> Option<Vec3> {
    let denom = ray_dir.dot(plane_normal);
    if denom.abs() < 1e-6 {
        return None;
    }

    let t = (plane_point - ray_origin).dot(plane_normal) / denom;
    if t < 0.0 {
        return None;
    }

    Some(ray_origin + ray_dir * t)
}

/// Project a world-space point to screen coordinates.
///
/// Returns (screen_x, screen_y) in the viewport's coordinate space, or None
/// if the point is behind the camera.
pub fn world_to_screen(
    world_pos: Vec3,
    view_matrix: &Mat4,
    proj_matrix: &Mat4,
    viewport: (f32, f32, f32, f32),
) -> Option<(f32, f32)> {
    let clip =
        *proj_matrix * *view_matrix * Vec4::new(world_pos.x(), world_pos.y(), world_pos.z(), 1.0);

    if clip.w() <= 0.0 {
        return None;
    }

    // NDC
    let ndc_x = clip.x() / clip.w();
    let ndc_y = clip.y() / clip.w();

    // Screen coordinates
    let screen_x = viewport.0 + (ndc_x + 1.0) * 0.5 * viewport.2;
    let screen_y = viewport.1 + (ndc_y + 1.0) * 0.5 * viewport.3;

    Some((screen_x, screen_y))
}

/// Hit-test the gizmo axes against a screen-space mouse position.
///
/// For translate and scale modes, projects each axis endpoint to screen space
/// and finds the closest axis within a pixel threshold.
/// For rotate mode, projects the rotation ring to screen space and checks
/// distance to the circle arc.
#[allow(clippy::too_many_arguments)]
pub fn hit_test_axes(
    mouse_screen: (f32, f32),
    gizmo_origin: Vec3,
    gizmo_scale: f32,
    view_matrix: &Mat4,
    proj_matrix: &Mat4,
    viewport: (f32, f32, f32, f32),
    mode: GizmoMode,
    pixel_threshold: f32,
) -> Option<GizmoAxis> {
    let origin_screen = world_to_screen(gizmo_origin, view_matrix, proj_matrix, viewport)?;

    match mode {
        GizmoMode::Rotate => hit_test_rotate_rings(
            mouse_screen,
            gizmo_origin,
            gizmo_scale,
            view_matrix,
            proj_matrix,
            viewport,
            origin_screen,
            pixel_threshold,
        ),
        _ => hit_test_linear_axes(
            mouse_screen,
            gizmo_origin,
            gizmo_scale,
            view_matrix,
            proj_matrix,
            viewport,
            origin_screen,
            mode,
            pixel_threshold,
        ),
    }
}

fn hit_test_linear_axes(
    mouse_screen: (f32, f32),
    gizmo_origin: Vec3,
    gizmo_scale: f32,
    view_matrix: &Mat4,
    proj_matrix: &Mat4,
    viewport: (f32, f32, f32, f32),
    origin_screen: (f32, f32),
    mode: GizmoMode,
    pixel_threshold: f32,
) -> Option<GizmoAxis> {
    let axis_length = match mode {
        GizmoMode::Translate => gizmo_scale * 1.5,
        GizmoMode::Scale => gizmo_scale * 1.2,
        _ => return None,
    };

    let mut best_axis = None;
    let mut best_dist = pixel_threshold;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let end = gizmo_origin + axis.direction() * axis_length;

        if let Some(end_screen) = world_to_screen(end, view_matrix, proj_matrix, viewport) {
            let dist = point_to_segment_dist_2d(mouse_screen, origin_screen, end_screen);

            if dist < best_dist {
                best_dist = dist;
                best_axis = Some(axis);
            }
        }
    }

    best_axis
}

fn hit_test_rotate_rings(
    mouse_screen: (f32, f32),
    gizmo_origin: Vec3,
    gizmo_scale: f32,
    view_matrix: &Mat4,
    proj_matrix: &Mat4,
    viewport: (f32, f32, f32, f32),
    origin_screen: (f32, f32),
    pixel_threshold: f32,
) -> Option<GizmoAxis> {
    let ring_radius = gizmo_scale * 1.0;
    let sample_count = 48;

    let mut best_axis = None;
    let mut best_dist = pixel_threshold;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        // Sample points along the ring in 3D, project to screen, and measure distance
        for i in 0..sample_count {
            let angle = (i as f32 / sample_count as f32) * 2.0 * std::f32::consts::PI;

            let (perp1, perp2) = axis.perpendicular_axes();
            let ring_point = gizmo_origin
                + perp1 * (ring_radius * angle.cos())
                + perp2 * (ring_radius * angle.sin());

            if let Some(screen_pos) =
                world_to_screen(ring_point, view_matrix, proj_matrix, viewport)
            {
                let dx = mouse_screen.0 - screen_pos.0;
                let dy = mouse_screen.1 - screen_pos.1;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist < best_dist {
                    best_dist = dist;
                    best_axis = Some(axis);
                }
            }
        }
    }

    best_axis
}

/// 2D distance from a point to a line segment.
fn point_to_segment_dist_2d(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len_sq = dx * dx + dy * dy;

    let t = if len_sq < 1e-8 {
        0.0
    } else {
        ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len_sq
    };

    let t = t.clamp(0.0, 1.0);
    let closest_x = a.0 + t * dx;
    let closest_y = a.1 + t * dy;

    let ex = p.0 - closest_x;
    let ey = p.1 - closest_y;

    (ex * ex + ey * ey).sqrt()
}

/// Apply a translate drag to get the new position.
///
/// Projects the mouse ray onto the plane defined by the active axis and
/// the camera's view direction, then returns the delta along the axis.
pub fn compute_translate_delta(
    axis: GizmoAxis,
    ray_origin: Vec3,
    ray_dir: Vec3,
    gizmo_origin: Vec3,
    camera_forward: Vec3,
) -> Option<Vec3> {
    let plane_normal = build_axis_drag_plane_normal(axis, camera_forward)?;
    let hit = ray_plane_intersection(ray_origin, ray_dir, gizmo_origin, plane_normal)?;

    let delta = hit - gizmo_origin;
    let axis_component = axis.direction() * delta.dot(axis.direction());

    Some(axis_component)
}

/// Apply a rotation drag to get the rotation delta.
///
/// Computes the angle swept between two screen positions around a center point.
/// The center should be the gizmo origin projected to screen space.
pub fn compute_rotate_delta(
    axis: GizmoAxis,
    center_screen: (f32, f32),
    current_screen: (f32, f32),
    previous_screen: (f32, f32),
) -> f32 {
    let prev_dx = previous_screen.0 - center_screen.0;
    let prev_dy = previous_screen.1 - center_screen.1;
    let curr_dx = current_screen.0 - center_screen.0;
    let curr_dy = current_screen.1 - center_screen.1;

    // Skip degenerate case where both points are at the center
    if prev_dx.abs() < 1e-6 && prev_dy.abs() < 1e-6 {
        return 0.0;
    }
    if curr_dx.abs() < 1e-6 && curr_dy.abs() < 1e-6 {
        return 0.0;
    }

    let prev_angle = prev_dy.atan2(prev_dx);
    let curr_angle = curr_dy.atan2(curr_dx);

    let mut delta = curr_angle - prev_angle;

    // Wrap to [-PI, PI] to avoid large jumps when crossing the +-PI boundary
    if delta > std::f32::consts::PI {
        delta -= 2.0 * std::f32::consts::PI;
    } else if delta < -std::f32::consts::PI {
        delta += 2.0 * std::f32::consts::PI;
    }

    // Negate for X and Z axes so all axes rotate consistently with screen drag direction
    match axis {
        GizmoAxis::X => delta = -delta,
        GizmoAxis::Y => {}
        GizmoAxis::Z => delta = -delta,
    }

    delta
}

/// Apply a scale drag to get the signed distance along the axis.
///
/// Projects the mouse ray onto the axis drag plane and returns the signed
/// distance from the gizmo origin to the hit point, projected onto the axis.
pub fn compute_scale_delta(
    axis: GizmoAxis,
    ray_origin: Vec3,
    ray_dir: Vec3,
    gizmo_origin: Vec3,
    camera_forward: Vec3,
) -> Option<f32> {
    let plane_normal = build_axis_drag_plane_normal(axis, camera_forward)?;
    let hit = ray_plane_intersection(ray_origin, ray_dir, gizmo_origin, plane_normal)?;

    let delta = hit - gizmo_origin;
    Some(delta.dot(axis.direction()))
}

/// Build a plane normal for dragging along an axis, perpendicular to the camera view.
///
/// Returns None if no valid plane can be constructed (axis parallel to camera forward
/// and no perpendicular fallback available).
fn build_axis_drag_plane_normal(axis: GizmoAxis, camera_forward: Vec3) -> Option<Vec3> {
    let axis_dir = axis.direction();

    let right = axis_dir.cross(camera_forward);
    if right.length() < 1e-6 {
        // Axis is parallel to camera forward; use a fallback plane from perpendicular axes
        let (_, perp1) = axis.perpendicular_axes();
        let plane_normal = axis_dir.cross(perp1);
        if plane_normal.length() < 1e-6 {
            return None;
        }
        return Some(plane_normal.normalize());
    }

    Some(axis_dir.cross(right.normalize()).normalize())
}

/// Generate draw calls for the translate gizmo.
///
/// Produces three colored arrow shafts + cone tips along X, Y, Z axes.
pub fn generate_translate_draw_calls(
    resources: &GizmoResources,
    origin: Vec3,
    scale: f32,
    hovered_axis: Option<GizmoAxis>,
    active_axis: Option<GizmoAxis>,
    next_instance_index: &mut u32,
) -> Vec<DrawCall> {
    let mut draws = Vec::with_capacity(6);

    let shaft_length = scale * 1.2;
    let shaft_radius = scale * 0.025;
    let tip_length = scale * 0.3;
    let tip_radius = scale * 0.08;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let is_highlighted = hovered_axis == Some(axis) || active_axis == Some(axis);
        let color = if is_highlighted {
            axis.color().highlight_color()
        } else {
            axis.color().color()
        };

        let axis_dir = axis.direction();
        let axis_end = origin + axis_dir * shaft_length;

        // Shaft: cylinder along the axis
        let shaft_transform = make_axis_cylinder_transform(origin, axis_end, shaft_radius);

        let shaft_idx = *next_instance_index;
        *next_instance_index += 1;

        draws.push(
            DrawCall::new(resources.shaft_mesh, resources.material)
                .with_transform(shaft_transform)
                .with_color(color.to_array())
                .with_pbr(0.0, 1.0, 1.0) // Non-metallic, rough, full AO for flat look
                .with_instance_index(shaft_idx),
        );

        // Tip: cone at the end of the shaft
        let tip_transform = make_axis_cylinder_transform(
            axis_end,
            origin + axis_dir * (shaft_length + tip_length),
            tip_radius,
        );

        let tip_idx = *next_instance_index;
        *next_instance_index += 1;

        draws.push(
            DrawCall::new(resources.cone_mesh, resources.material)
                .with_transform(tip_transform)
                .with_color(color.to_array())
                .with_pbr(0.0, 1.0, 1.0)
                .with_instance_index(tip_idx),
        );
    }

    draws
}

/// Generate draw calls for the rotate gizmo.
///
/// Produces three colored torus rings along X, Y, Z axes.
pub fn generate_rotate_draw_calls(
    resources: &GizmoResources,
    origin: Vec3,
    scale: f32,
    hovered_axis: Option<GizmoAxis>,
    active_axis: Option<GizmoAxis>,
    next_instance_index: &mut u32,
) -> Vec<DrawCall> {
    let mut draws = Vec::with_capacity(3);

    let ring_radius = scale * 1.0;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let is_highlighted = hovered_axis == Some(axis) || active_axis == Some(axis);
        let color = if is_highlighted {
            axis.color().highlight_color()
        } else {
            axis.color().color()
        };

        let rotation = match axis {
            GizmoAxis::X => Mat4::from_euler_angles(0.0, std::f32::consts::FRAC_PI_2, 0.0),
            GizmoAxis::Y => Mat4::identity(),
            GizmoAxis::Z => Mat4::from_euler_angles(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
        };

        let translation = mat4_from_translation(origin);
        let scale_mat = Mat4::from_scale(Vec3::new(ring_radius, ring_radius, ring_radius));
        let ring_transform = (translation * rotation * scale_mat).to_array();

        let idx = *next_instance_index;
        *next_instance_index += 1;

        draws.push(
            DrawCall::new(resources.ring_mesh, resources.material)
                .with_transform(ring_transform)
                .with_color(color.to_array())
                .with_pbr(0.0, 1.0, 1.0)
                .with_instance_index(idx),
        );
    }

    draws
}

/// Generate draw calls for the scale gizmo.
///
/// Produces three colored axes with cube-shaped tips (instead of cones).
pub fn generate_scale_draw_calls(
    resources: &GizmoResources,
    origin: Vec3,
    scale: f32,
    hovered_axis: Option<GizmoAxis>,
    active_axis: Option<GizmoAxis>,
    next_instance_index: &mut u32,
) -> Vec<DrawCall> {
    let mut draws = Vec::with_capacity(6);

    let shaft_length = scale * 1.0;
    let shaft_radius = scale * 0.025;
    let cube_size = scale * 0.12;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let is_highlighted = hovered_axis == Some(axis) || active_axis == Some(axis);
        let color = if is_highlighted {
            axis.color().highlight_color()
        } else {
            axis.color().color()
        };

        let axis_dir = axis.direction();
        let axis_end = origin + axis_dir * shaft_length;

        // Shaft
        let shaft_transform = make_axis_cylinder_transform(origin, axis_end, shaft_radius);

        let shaft_idx = *next_instance_index;
        *next_instance_index += 1;

        draws.push(
            DrawCall::new(resources.shaft_mesh, resources.material)
                .with_transform(shaft_transform)
                .with_color(color.to_array())
                .with_pbr(0.0, 1.0, 1.0)
                .with_instance_index(shaft_idx),
        );

        // Cube tip at the end
        let cube_center = origin + axis_dir * (shaft_length + cube_size * 0.5);
        let cube_transform = mat4_from_translation(cube_center)
            * Mat4::from_scale(Vec3::new(cube_size, cube_size, cube_size));

        let cube_idx = *next_instance_index;
        *next_instance_index += 1;

        draws.push(
            DrawCall::new(resources.cube_mesh, resources.material)
                .with_transform(cube_transform.to_array())
                .with_color(color.to_array())
                .with_pbr(0.0, 1.0, 1.0)
                .with_instance_index(cube_idx),
        );
    }

    draws
}

/// Build a transform matrix for a cylinder oriented along an arbitrary axis.
///
/// Given two endpoints and a radius, creates a matrix that positions and
/// scales a unit cylinder (along Y) to match.
fn make_axis_cylinder_transform(from: Vec3, to: Vec3, radius: f32) -> [f32; 16] {
    let direction = to - from;
    let length = direction.length();

    if length < 1e-6 {
        return Mat4::identity().to_array();
    }

    let dir_normalized = direction / length;

    // Build a rotation that maps Y-up to the direction vector
    // Using a simple approach: compute the rotation axis and angle
    let up = Vec3::new(0.0, 1.0, 0.0);

    let dot = up.dot(dir_normalized);

    if dot > 0.9999 {
        // Already aligned with Y
        let m = mat4_from_translation(from) * Mat4::from_scale(Vec3::new(radius, length, radius));
        return m.to_array();
    }

    if dot < -0.9999 {
        // Opposite direction - rotate 180 around X
        let m = mat4_from_translation(from)
            * Mat4::from_euler_angles(std::f32::consts::PI, 0.0, 0.0)
            * Mat4::from_scale(Vec3::new(radius, length, radius));
        return m.to_array();
    }

    // General case: rotation axis = cross(up, dir)
    let rot_axis = up.cross(dir_normalized).normalize();
    let angle = dot.acos();

    // Build rotation matrix from axis-angle using Mat4::from_rotaxis
    let rot = Mat4::from_rotaxis(&angle, [rot_axis.x(), rot_axis.y(), rot_axis.z()]);

    let m = mat4_from_translation(from) * rot * Mat4::from_scale(Vec3::new(radius, length, radius));
    m.to_array()
}

/// Helper to build a translation Mat4 from a Vec3.
fn mat4_from_translation(v: Vec3) -> Mat4 {
    Mat4::from_translation([v.x(), v.y(), v.z()])
}
