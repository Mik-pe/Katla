//! Canonical, iterative world-pose resolution for rendering, physics and authoring.
use crate::components::{Parent, TransformComponent, TransformDirty, WorldTransform};
use katla_ecs::{EntityId, System, World};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

/// Resolve current local transforms without depending on system execution order.
/// Each entity is visited once; deep trees do not consume the call stack.
/// Missing transform parents terminate ancestry. Invalid runtime cycles use local
/// poses and emit a warning; scene and prefab validation reject cycles on disk.
pub fn resolve_world_transforms(world: &World) -> HashMap<EntityId, WorldTransform> {
    resolve_with_world_poses(world, HashMap::new())
}

/// Resolve intermediate nodes from explicit world poses returned by physics.
pub(crate) fn resolve_with_world_poses(
    world: &World,
    mut poses: HashMap<EntityId, WorldTransform>,
) -> HashMap<EntityId, WorldTransform> {
    let locals: HashMap<_, _> = world
        .query_ref::<&TransformComponent>()
        .map(|(id, local)| (id, (local, Cell::new(false))))
        .collect();
    let parents: HashMap<_, _> = world
        .query_ref::<&Parent>()
        .map(|(id, parent)| (id, parent.parent))
        .collect();
    poses.reserve(locals.len());
    let mut path = Vec::new();
    for &entity in locals.keys() {
        if poses.contains_key(&entity) {
            continue;
        }
        let mut current = entity;
        let mut cyclic = false;
        while !poses.contains_key(&current) {
            let Some((_, visited)) = locals.get(&current) else {
                break;
            };
            // Completed paths have poses; a visited local without one is active.
            if visited.replace(true) {
                log::warn!("Transform hierarchy cycle at {current:?}; using local poses");
                cyclic = true;
                break;
            }
            path.push(current);
            let Some(parent) = parents.get(&current) else {
                break;
            };
            current = *parent;
        }
        for id in path.drain(..).rev() {
            let Some((component, _)) = locals.get(&id) else {
                continue;
            };
            let local = component.transform;
            let pose =
                if !cyclic && let Some(parent) = parents.get(&id).and_then(|id| poses.get(id)) {
                    let matrix = parent.matrix * local.make_mat4();
                    WorldTransform {
                        matrix,
                        transform: katla_math::Transform {
                            position: matrix.extract_translation(),
                            rotation: parent.transform.rotation * local.rotation,
                            scale: parent.transform.scale * local.scale,
                        },
                    }
                } else {
                    WorldTransform::new(local)
                };
            poses.insert(id, pose);
        }
    }
    poses
}

#[cfg(test)]
#[path = "transform_hierarchy_benchmarks.rs"]
mod benchmarks;

/// Publishes canonical world poses. Changes and reparenting are detected even
/// when callers mutate local components without a `TransformDirty` marker.
#[derive(Default)]
pub struct TransformHierarchySystem {
    _private: (),
}

/// Controls whether unchanged cached components should be rewritten.
/// Pose resolution remains O(N) because local component mutation is unrestricted.
#[derive(Debug)]
pub struct TransformOptimization {
    /// Rewrite all poses when the changed fraction exceeds this threshold.
    pub threshold: f32,
    pub moving_count: usize,
    pub total_count: usize,
}
impl Default for TransformOptimization {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            moving_count: 0,
            total_count: 0,
        }
    }
}

impl System for TransformHierarchySystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        let poses = resolve_world_transforms(world);
        let dirty: HashSet<_> = world
            .query_ref::<&TransformDirty>()
            .map(|(id, _)| id)
            .collect();
        let changed: HashSet<_> = poses
            .iter()
            .filter_map(|(&id, pose)| {
                let old = world.get_component::<WorldTransform>(id)?;
                (old.matrix != pose.matrix
                    || old.transform != pose.transform
                    || dirty.contains(&id))
                .then_some(id)
            })
            .collect();
        let config = world.get_resource_mut_or_insert_with::<TransformOptimization>();
        config.total_count = poses.len();
        config.moving_count = changed.len();
        let rewrite_all =
            config.threshold <= 0.0 || changed.len() as f32 > poses.len() as f32 * config.threshold;
        for (id, pose) in poses {
            if let Some(old) = world.get_component_mut::<WorldTransform>(id) {
                if rewrite_all || changed.contains(&id) {
                    *old = pose;
                }
            } else {
                world.add_component(id, pose);
            }
        }
        let stale: Vec<_> = world
            .query_ref::<&WorldTransform>()
            .filter(|(id, _)| world.get_component::<TransformComponent>(*id).is_none())
            .map(|(id, _)| id)
            .collect();
        for id in stale {
            world.remove_component::<WorldTransform>(id);
        }
        for id in dirty {
            world.remove_component::<TransformDirty>(id);
        }
    }
    fn name(&self) -> &str {
        "TransformHierarchySystem"
    }
}

/// Render bounds for an entire subtree, including an empty prefab root.
#[cfg(feature = "editor")]
pub(crate) fn subtree_render_bounds(world: &World, root: EntityId) -> Option<katla_math::AABB> {
    let poses = resolve_world_transforms(world);
    let mut children = HashMap::<_, Vec<_>>::new();
    for (child, parent) in world.query_ref::<&Parent>() {
        children.entry(parent.parent).or_default().push(child);
    }
    let mut stack = vec![root];
    let mut visited = HashSet::new();
    let mut bounds: Option<katla_math::AABB> = None;
    while let Some(entity) = stack.pop() {
        if !visited.insert(entity) {
            continue;
        }
        if let Some(local) = world
            .get_component::<crate::components::DrawableComponent>(entity)
            .and_then(|d| d.bounds)
            && let Some(pose) = poses.get(&entity)
        {
            let next = local.transform(&pose.matrix);
            bounds = Some(bounds.map_or(next, |bounds| bounds.merge(&next)));
        }
        if let Some(children) = children.get(&entity) {
            stack.extend(children);
        }
    }
    bounds
}
