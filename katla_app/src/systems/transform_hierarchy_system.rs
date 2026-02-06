use katla_ecs::{EntityId, System, World};
use katla_math::Transform;
use crate::components::{Parent, TransformComponent, TransformDirty, WorldTransform};
use std::collections::{HashMap, HashSet};

/// Updates world-space transforms by propagating parent transforms to children.
///
/// This system traverses the entity hierarchy in topological order (parents before children)
/// and computes the world-space transform for each entity by multiplying the parent's
/// world transform with the entity's local transform.
///
/// **Optimizations**:
/// - Only processes entities marked with `TransformDirty`
/// - Skips entire subtrees when few entities are moving (static scene optimization)
///
/// **Execution Order**: Should run EARLY - before physics, rendering, and any systems
/// that depend on world-space transforms.
///
/// **Performance**: O(D) where D is the number of dirty entities and their descendants.
#[derive(Default)]
pub struct TransformHierarchySystem {
    /// Track if we've run at least once (to initialize all transforms)
    initialized: bool,
}


/// Configuration for static scene optimization.
///
/// For scenes with many static entities, it's much faster to track dirty subtrees
/// and skip them during propagation. If your scene is very dynamic, the cost of
/// tracking can exceed the benefits.
#[derive(Debug)]
pub struct TransformOptimization {
    /// If the percentage of moving objects exceeds this value, skip dirty tracking.
    /// - 0.0 = Never use static optimization (always process all entities)
    /// - 1.0 = Always use static optimization (only process dirty entities)
    /// - 0.3 = Default: Skip optimization if >30% of entities are moving
    pub threshold: f32,
    /// Number of moving entities this frame (for debugging/profiling)
    pub moving_count: usize,
    /// Total number of transform entities (for debugging/profiling)
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

impl TransformHierarchySystem {
    /// Collect all entities with transforms and optionally their dirty state
    fn collect_hierarchy_data(world: &mut World) -> HierarchyData {
        let mut entities = HashSet::new();
        let mut local_transforms = HashMap::new();
        let mut parent_map = HashMap::new();
        let mut dirty_entities = HashSet::new();

        // Query all entities with transforms
        for (entity, transform) in world.query::<&TransformComponent>() {
            entities.insert(entity);
            local_transforms.insert(entity, transform.transform);
        }

        // Collect parent relationships
        for (entity, parent) in world.query::<&Parent>() {
            parent_map.insert(entity, parent.parent);
        }

        // Collect dirty entities (only if initialized)
        for (entity, _) in world.query::<&TransformDirty>() {
            dirty_entities.insert(entity);
        }

        HierarchyData {
            entities,
            local_transforms,
            parent_map,
            dirty_entities,
        }
    }

    /// Perform topological sort of the transform hierarchy.
    ///
    /// Returns entities in order where parents always come before children.
    fn topological_sort(
        entities: &HashSet<EntityId>,
        parent_map: &HashMap<EntityId, EntityId>,
    ) -> Vec<EntityId> {
        let mut sorted = Vec::with_capacity(entities.len());
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        // Build children map for efficient traversal
        let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        for &entity in entities {
            if let Some(&parent) = parent_map.get(&entity) {
                children_map.entry(parent).or_default().push(entity);
            }
        }

        // Visit each unvisited entity
        for &entity in entities {
            if !visited.contains(&entity) {
                Self::visit(
                    entity,
                    &mut sorted,
                    &mut visited,
                    &mut visiting,
                    &children_map,
                );
            }
        }

        sorted
    }

    /// Recursive DFS helper for topological sort.
    fn visit(
        entity: EntityId,
        sorted: &mut Vec<EntityId>,
        visited: &mut HashSet<EntityId>,
        visiting: &mut HashSet<EntityId>,
        children_map: &HashMap<EntityId, Vec<EntityId>>,
    ) {
        // Check for cycles
        if visiting.contains(&entity) {
            eprintln!(
                "Warning: Transform hierarchy cycle detected at entity {:?}. \
                 This entity and its descendants will not be updated.",
                entity
            );
            return;
        }

        // Skip if already processed (from another path in the graph)
        if visited.contains(&entity) {
            return;
        }

        // Mark as being visited AND visited before recursing
        visiting.insert(entity);
        visited.insert(entity);

        // Recursively visit all children
        if let Some(children) = children_map.get(&entity) {
            for &child in children {
                Self::visit(child, sorted, visited, visiting, children_map);
            }
        }

        // All descendants processed, add this entity to sorted list
        visiting.remove(&entity);
        sorted.push(entity);
    }

    /// Calculate world transform for an entity by traversing its ancestry.
    fn calculate_world_transform(
        world: &World,
        entity: EntityId,
        local_transform: &Transform,
        parent_map: &HashMap<EntityId, EntityId>,
    ) -> Transform {
        let mut world_transform = *local_transform;
        let mut current_entity = entity;

        // Traverse up the hierarchy
        while let Some(&parent_id) = parent_map.get(&current_entity) {
            if let Some(parent_world) = world.get_component::<WorldTransform>(parent_id) {
                // Parent already has world transform (topological order guarantees this)
                world_transform = parent_world.transform * world_transform;
                break;
            } else if let Some(parent_local) = world.get_component::<TransformComponent>(parent_id)
            {
                // Parent not processed yet (shouldn't happen with topological sort)
                // Fall back to local transform
                world_transform = parent_local.transform * world_transform;
                current_entity = parent_id;
            } else {
                // Parent has no transform component
                break;
            }
        }

        world_transform
    }

    /// Mark all descendants of dirty entities as needing update
    fn mark_dirty_subtree(
        entity: EntityId,
        dirty_set: &mut HashSet<EntityId>,
        children_map: &HashMap<EntityId, Vec<EntityId>>,
    ) {
        dirty_set.insert(entity);

        if let Some(children) = children_map.get(&entity) {
            for &child in children {
                Self::mark_dirty_subtree(child, dirty_set, children_map);
            }
        }
    }

    /// Clear dirty flags from all entities
    fn clear_dirty_flags(world: &mut World) {
        // Collect entities with dirty flags
        let dirty_entities: Vec<EntityId> = world
            .query::<&TransformDirty>()
            .map(|(entity, _)| entity)
            .collect();

        // Remove the component from each entity
        for entity in dirty_entities {
            world.remove_component::<TransformDirty>(entity);
        }
    }
}

impl System for TransformHierarchySystem {
    fn update(&mut self, world: &mut World, _delta_time: f32) {
        // Collect all hierarchy data first (before borrowing for resource)
        let data = Self::collect_hierarchy_data(world);

        if data.entities.is_empty() {
            return;
        }

        // Get or create optimization config resource
        let optimization = if world.contains_resource::<TransformOptimization>() {
            world.get_resource_mut::<TransformOptimization>().unwrap()
        } else {
            // First run - register the resource
            world.insert_resource(TransformOptimization::default());
            world.get_resource_mut::<TransformOptimization>().unwrap()
        };

        // First frame: initialize all transforms
        if !self.initialized {
            self.initialized = true;
            optimization.total_count = data.entities.len();

            // Sort and process all entities
            let sorted_entities = Self::topological_sort(&data.entities, &data.parent_map);

            for entity in sorted_entities {
                let local_transform = match data.local_transforms.get(&entity) {
                    Some(&transform) => transform,
                    None => continue,
                };

                let world_transform =
                    Self::calculate_world_transform(world, entity, &local_transform, &data.parent_map);

                if let Some(existing) = world.get_component_mut::<WorldTransform>(entity) {
                    existing.transform = world_transform;
                } else {
                    world.add_component(entity, WorldTransform::new(world_transform));
                }
            }

            // Clear any dirty flags that were set
            Self::clear_dirty_flags(world);
            return;
        }

        optimization.total_count = data.entities.len();
        optimization.moving_count = data.dirty_entities.len();

        // Check if we should use static optimization
        let use_static_optimization = optimization.threshold > 0.0
            && optimization.threshold < 1.0
            && (optimization.moving_count as f32 / optimization.total_count as f32)
                <= optimization.threshold;

        let entities_to_process = if use_static_optimization && !data.dirty_entities.is_empty() {
            // Build children map for subtree traversal
            let mut children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
            for &entity in &data.entities {
                if let Some(&parent) = data.parent_map.get(&entity) {
                    children_map.entry(parent).or_default().push(entity);
                }
            }

            // Mark all descendants of dirty entities
            let mut dirty_subtree = HashSet::new();
            for &entity in &data.dirty_entities {
                Self::mark_dirty_subtree(entity, &mut dirty_subtree, &children_map);
            }

            dirty_subtree
        } else if data.dirty_entities.is_empty() && optimization.threshold > 0.0 {
            // No entities changed, skip all processing
            Self::clear_dirty_flags(world);
            return;
        } else {
            // Either threshold disabled or too much movement, process all entities
            data.entities.clone()
        };

        // Sort entities topologically
        let sorted_entities = Self::topological_sort(&entities_to_process, &data.parent_map);

        // Update world transforms
        for entity in sorted_entities {
            let local_transform = match data.local_transforms.get(&entity) {
                Some(&transform) => transform,
                None => continue,
            };

            let world_transform =
                Self::calculate_world_transform(world, entity, &local_transform, &data.parent_map);

            if let Some(existing) = world.get_component_mut::<WorldTransform>(entity) {
                existing.transform = world_transform;
            } else {
                world.add_component(entity, WorldTransform::new(world_transform));
            }
        }

        // Clear dirty flags after propagation
        Self::clear_dirty_flags(world);
    }

    fn name(&self) -> &str {
        "TransformHierarchySystem"
    }
}

/// Hierarchy data collected from the world
struct HierarchyData {
    entities: HashSet<EntityId>,
    local_transforms: HashMap<EntityId, Transform>,
    parent_map: HashMap<EntityId, EntityId>,
    dirty_entities: HashSet<EntityId>,
}
