//! Application-aware queries for room context beyond the current frustum.
use crate::application::Application;
use crate::components::{DrawableComponent, EditorHidden, NameComponent, Parent};
use katla_ecs::scene_tool::SceneOp;
use katla_math::Vec3;
use serde_json::json;

pub(super) fn execute(app: &Application, op: &SceneOp) -> Option<String> {
    let SceneOp::QueryEntities {
        component_filter,
        name_filter,
        position,
        radius,
        limit,
    } = op
    else {
        return None;
    };
    let entry = match component_filter.as_deref() {
        Some(name) => match app.editor.component_registry.get(name) {
            Some(entry) => Some(entry),
            None => return Some(format!("Error: component '{name}' is not registered")),
        },
        None => None,
    };
    if position.iter().flatten().any(|v| !v.is_finite())
        || radius.is_some_and(|r| !r.is_finite() || r < 0.0)
        || radius.is_some() != position.is_some()
    {
        return Some(
            "Error: spatial queries require a finite position and nonnegative radius together"
                .into(),
        );
    }
    let origin = position.map(|p| Vec3::new(p[0], p[1], p[2]));
    let mut entities: Vec<_> = app.world.entity_ids().collect();
    entities.sort_by_key(|id| id.id());
    let poses = crate::systems::resolve_world_transforms(&app.world);
    let mut rows = Vec::new();
    for id in entities {
        if app.world.get_component::<EditorHidden>(id).is_some()
            || entry.is_some_and(|e| !(e.has_component)(&app.world, id))
        {
            continue;
        }
        let name = app
            .world
            .get_component::<NameComponent>(id)
            .map(|c| c.name.as_str());
        if name_filter.as_ref().is_some_and(|filter| {
            !name.is_some_and(|name| name.to_lowercase().contains(&filter.to_lowercase()))
        }) {
            continue;
        }
        let transform = poses.get(&id);
        let bounds = transform.and_then(|t| {
            app.world
                .get_component::<DrawableComponent>(id)
                .and_then(|d| d.bounds)
                .map(|b| b.transform(&t.matrix))
        });
        if let (Some(origin), Some(radius)) = (origin, *radius) {
            let Some(transform) = transform else {
                continue;
            };
            let distance = if let Some(bounds) = bounds {
                let min = bounds.min();
                let max = bounds.max();
                let closest = Vec3::new(
                    origin.x().clamp(min.x(), max.x()),
                    origin.y().clamp(min.y(), max.y()),
                    origin.z().clamp(min.z(), max.z()),
                );
                (closest - origin).length()
            } else {
                (transform.transform.position - origin).length()
            };
            if distance > radius {
                continue;
            }
        }
        rows.push(json!({ "entity_id": id.id().to_string(), "name": name,
            "position": transform.map(|t| t.transform.position.to_array()),
            "bounds": bounds.map(|b| json!({"center":b.center.to_array(),"extent":b.extent.to_array()})),
            "parent_id": app.world.get_component::<Parent>(id).map(|p|p.parent.id().to_string()),
            "components": app.editor.component_registry.type_names().into_iter().filter(|name| app.editor.component_registry.get(name).is_some_and(|e|(e.has_component)(&app.world,id))).collect::<Vec<_>>()
        }));
    }
    let total = rows.len();
    let limit = limit.unwrap_or(64).clamp(1, 256);
    rows.truncate(limit);
    Some(json!({"success":true,"data":{"entities":rows,"total":total,"truncated":total>limit,"spatial_contract":"Distance to render bounds if available, otherwise transform origin. No frustum or room-membership restriction."}}).to_string())
}
