//! Frame-scoped shared editor view. Geometry candidates do not imply occlusion visibility.
use crate::application::Application;
use crate::components::{
    DrawableComponent, EditorHidden, NameComponent, OrbitCameraControllerComponent,
    PerspectiveComponent, TransformComponent,
};
use katla_agent::mcp::EditorViewOp;
use katla_ecs::EntityId;
use katla_math::{AABB, Frustum, Mat4, Quat, Vec3, Vec4};
use serde_json::{Value, json};

pub(super) fn apply(app: &mut Application, op: &EditorViewOp) -> Result<(), String> {
    if app.play_mode != crate::application::game_state::PlayMode::Editing {
        return Err("Shared editor view is available only in edit mode".into());
    }
    match op {
        EditorViewOp::Observe { .. } => {}
        EditorViewOp::SetCamera { position, target } => set_camera(app, *position, *target)?,
        EditorViewOp::Select { entity_id } => {
            let entity = entity_id.as_deref().map(|id| entity(app, id)).transpose()?;
            app.editor.editor_ui.selected_entity = entity;
            // An older outstanding click must not overwrite this explicit selection.
            app.editor_features.latest_pick_sequence += 1;
        }
        EditorViewOp::Focus { entity_id, select } => {
            let id = entity(app, entity_id)?;
            let bounds = object_bounds(app, id).ok_or("Object has no render bounds to focus")?;
            let projection = app
                .world
                .get_component::<PerspectiveComponent>(app.camera.entity)
                .ok_or("Missing editor projection")?;
            let half_angle = projection.fov.to_radians() * 0.5;
            let fit = half_angle.min((half_angle.tan() * projection.aspect_ratio).atan());
            let distance = (bounds.extent.length() / fit.sin() * 1.3).max(0.5);
            let orbit = app
                .world
                .get_component::<OrbitCameraControllerComponent>(app.camera.entity)
                .ok_or("Missing editor camera controller")?;
            let rotation = Quat::new_from_yaw_pitch(orbit.yaw, orbit.pitch);
            let position = bounds.center + rotation.rotate_vec3(Vec3::new(0.0, 0.0, distance));
            set_camera(app, array(position), array(bounds.center))?;
            if *select {
                app.editor.editor_ui.selected_entity = Some(id);
                app.editor_features.latest_pick_sequence += 1;
            }
        }
        EditorViewOp::Undo => {
            if !app.editor.perform_agent_undo(&mut app.world) {
                return Err("No agent scene change to undo".into());
            }
            super::process_gpu_cleanup_for_destroyed_entities(app);
            if app
                .editor
                .editor_ui
                .selected_entity
                .is_some_and(|id| !app.world.entity_exists(id))
            {
                app.editor.editor_ui.selected_entity = None;
            }
        }
    }
    Ok(())
}

fn entity(app: &Application, id: &str) -> Result<EntityId, String> {
    let entity = EntityId::from_raw(
        id.parse()
            .map_err(|_| "Expected a generational entity_id string")?,
    );
    if !app.world.entity_exists(entity) || app.world.get_component::<EditorHidden>(entity).is_some()
    {
        return Err(format!("Entity {id} is stale or editor-private"));
    }
    Ok(entity)
}

fn set_camera(app: &mut Application, position: [f32; 3], target: [f32; 3]) -> Result<(), String> {
    let (yaw, pitch, distance) = camera_pose(position, target)?;
    let id = app.camera.entity;
    let orbit = app
        .world
        .get_component_mut::<OrbitCameraControllerComponent>(id)
        .ok_or("Missing editor camera")?;
    if distance < orbit.min_distance
        || distance > orbit.max_distance
        || pitch.abs() > orbit.pitch_limit
    {
        return Err("Pose exceeds editor distance/pitch limits".into());
    }
    orbit.target = Vec3::new(target[0], target[1], target[2]);
    orbit.distance = distance;
    orbit.yaw = yaw;
    orbit.pitch = pitch;
    orbit.focus = None;
    let transform = app
        .world
        .get_component_mut::<TransformComponent>(id)
        .ok_or("Missing editor transform")?;
    transform.transform.position = Vec3::new(position[0], position[1], position[2]);
    transform.transform.rotation = Quat::new_from_yaw_pitch(yaw, pitch);
    Ok(())
}

fn camera_pose(position: [f32; 3], target: [f32; 3]) -> Result<(f32, f32, f32), String> {
    if position.iter().chain(&target).any(|v| !v.is_finite()) {
        return Err("Camera coordinates must be finite".into());
    }
    let delta = Vec3::new(
        position[0] - target[0],
        position[1] - target[1],
        position[2] - target[2],
    );
    let distance = delta.length();
    if distance < 0.001 || !distance.is_finite() {
        return Err("Camera needs distinct position and target".into());
    }
    Ok((
        delta.x().atan2(delta.z()),
        -(delta.y() / distance).asin(),
        distance,
    ))
}

fn array(v: Vec3) -> [f32; 3] {
    [v.x(), v.y(), v.z()]
}

// Match the transform consumed by collect_draws_with_context exactly.
fn object_bounds(app: &Application, id: EntityId) -> Option<AABB> {
    let drawable = app.world.get_component::<DrawableComponent>(id)?;
    let transform = app.world.get_component::<TransformComponent>(id)?;
    drawable
        .bounds
        .map(|b| b.transform(&transform.transform.make_mat4()))
}

pub(super) fn snapshot(app: &Application, limit: usize) -> Value {
    let view = app.camera.get_view_mat(&app.world);
    let proj = app.camera.get_proj_mat(&app.world);
    let frustum = Frustum::from_proj_and_view(&proj, &view);
    let vp = proj * view;
    let lookat = app.camera.get_lookat_mat(&app.world);
    let position = Vec3::new(lookat[3][0], lookat[3][1], lookat[3][2]);
    let direction = Vec3::new(-lookat[2][0], -lookat[2][1], -lookat[2][2]);
    let mut candidates = Vec::new();
    let mut missing_bounds = 0;
    for (id, drawable, transform) in app
        .world
        .query_ref::<(&DrawableComponent, &TransformComponent)>()
    {
        if drawable.mesh_handle.is_none()
            || drawable.material_handle.is_none()
            || app.world.get_component::<EditorHidden>(id).is_some()
        {
            continue;
        }
        let Some(bounds) = drawable
            .bounds
            .map(|b| b.transform(&transform.transform.make_mat4()))
        else {
            missing_bounds += 1;
            continue;
        };
        if !frustum.intersects_aabb(&bounds) {
            continue;
        }
        let name = app
            .world
            .get_component::<NameComponent>(id)
            .map(|n| n.name.clone());
        candidates.push((id.id(), json!({
            "entity_id": id.id().to_string(), "name": name,
            "world_bounds": {"center": array(bounds.center), "extent": array(bounds.extent)},
            "distance": (bounds.center-position).length(),
            "screen_rect": project_bounds(bounds, vp, proj[3][2]),
            "bounds_fully_in_frustum": frustum.contains_aabb(&bounds),
            "visibility": "frustum_candidate_occlusion_unknown",
            "parent_id": app.world.get_component::<crate::components::Parent>(id).map(|p| p.parent.id().to_string()),
        })));
    }
    candidates.sort_by_key(|(id, _)| *id);
    let total = candidates.len();
    candidates.truncate(limit.clamp(1, 256));
    json!({
        "frame": app.frame_count, "captured_unix_ms": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0),
        "camera": {"position": array(position), "direction": array(direction), "view_matrix": view.to_array(), "projection_matrix": proj.to_array(),
            "orbit": app.world.get_component::<OrbitCameraControllerComponent>(app.camera.entity).map(|c| json!({"target": array(c.target), "yaw": c.yaw, "pitch": c.pitch, "distance": c.distance}))},
        "selected_entity_id": app.editor.editor_ui.selected_entity.map(|id| id.id().to_string()),
        "candidates": candidates.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
        "candidate_count": total, "truncated": total > limit.clamp(1, 256), "renderables_without_bounds": missing_bounds,
        "coordinates": "normalized image coordinates: top-left (0,0), bottom-right (1,1); rectangles conservatively clipped to image",
        "visibility_contract": "Bounds intersect the render frustum; occlusion and room membership are unknown. GPU center/pointer picks identify the foremost pickable draw at those pixels only. No selection is needed. Use image and scene queries; clarify ambiguous references before edits.",
        "game_camera_modified": false,
    })
}

/// Project the near-clipped edges of an AABB, including origins outside the image.
fn project_bounds(bounds: AABB, vp: Mat4, near: f32) -> Option<[f32; 4]> {
    let mut corners = Vec::with_capacity(8);
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for z in [-1.0, 1.0] {
                let p = bounds.center
                    + Vec3::new(
                        bounds.extent.x() * x,
                        bounds.extent.y() * y,
                        bounds.extent.z() * z,
                    );
                corners.push(vp * Vec4::new(p.x(), p.y(), p.z(), 1.0));
            }
        }
    }
    let mut points: Vec<Vec4> = corners.iter().copied().filter(|p| p.w() >= near).collect();
    for i in 0..8 {
        for bit in [1, 2, 4] {
            let j = i ^ bit;
            if j <= i {
                continue;
            }
            let (a, b) = (corners[i], corners[j]);
            if (a.w() >= near) != (b.w() >= near) {
                points.push(a + (b - a) * ((near - a.w()) / (b.w() - a.w())));
            }
        }
    }
    if points.is_empty() {
        return None;
    }
    let mut rect = [1.0_f32, 1.0, 0.0, 0.0];
    for p in points {
        let x = (p.x() / p.w() * 0.5 + 0.5).clamp(0.0, 1.0);
        let y = (0.5 + p.y() / p.w() * 0.5).clamp(0.0, 1.0);
        rect[0] = rect[0].min(x);
        rect[1] = rect[1].min(y);
        rect[2] = rect[2].max(x);
        rect[3] = rect[3].max(y);
    }
    Some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_frustum_partial_bounds_and_left_right_without_selection() {
        let proj = Mat4::create_proj_reverse_z(60.0, 1.0, 0.01);
        let f = Frustum::from_proj_and_view(&proj, &Mat4::identity());
        let left = AABB::from_min_max(Vec3::new(-2.0, -0.5, -5.5), Vec3::new(-1.0, 0.5, -4.5));
        let right = AABB::from_min_max(Vec3::new(1.0, -0.5, -5.5), Vec3::new(2.0, 0.5, -4.5));
        assert!(f.intersects_aabb(&left) && f.intersects_aabb(&right));
        assert!(project_bounds(left, proj, 0.01).unwrap()[2] < 0.5);
        assert!(project_bounds(right, proj, 0.01).unwrap()[0] > 0.5);
        let partial = AABB::from_min_max(Vec3::new(2.0, -0.5, -5.5), Vec3::new(6.0, 0.5, -4.5));
        assert!(!f.contains_point(partial.center));
        assert!(f.intersects_aabb(&partial));
        assert_eq!(project_bounds(partial, proj, 0.01).unwrap()[2], 1.0);
        let behind = AABB::from_min_max(Vec3::new(-1.0, -1.0, 4.0), Vec3::new(1.0, 1.0, 5.0));
        assert!(!f.intersects_aabb(&behind));
        assert!(project_bounds(behind, proj, 0.01).is_none());
    }
    #[test]
    fn test_camera_pose_roundtrip_and_invalid_values() {
        let p = [3.0, 2.0, 7.0];
        let t = [1.0, 1.0, -2.0];
        let (yaw, pitch, d) = camera_pose(p, t).unwrap();
        let actual = Vec3::new(t[0], t[1], t[2])
            + Quat::new_from_yaw_pitch(yaw, pitch).rotate_vec3(Vec3::new(0.0, 0.0, d));
        assert!((actual - Vec3::new(p[0], p[1], p[2])).length() < 0.001);
        assert!(camera_pose(p, p).is_err());
        assert!(camera_pose([f32::NAN, 0.0, 1.0], t).is_err());
    }
}
