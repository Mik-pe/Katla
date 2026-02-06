# Transform Hierarchy System - Research & Design Notes

## Overview

This document summarizes research on transform hierarchy systems in modern ECS game engines, focusing on optimal implementations for propagating local-space transforms to world-space.

## Research Sources

- [Game engine from scratch #3 - ECS and transform hierarchy](https://arjonagelhout.nl/writings/2024-01-20_game_engine_03/) - Custom ECS implementation guide
- [Unity ECS Transform Concepts](https://docs.unity3d.com/Packages/com.unity.entities@1.0/manual/transforms-concepts.html) - Official Unity ECS transform documentation
- [Spotlight Team: Optimizing the Hierarchy](https://unity.com/blog/engine-platform/best-practices-from-the-spotlight-team-optimizing-the-hierarchy) - Unity team recommendations
- [Bevy Transform Systems Source Code](https://raw.githubusercontent.com/bevyengine/bevy/main/crates/bevy_transform/src/systems.rs) - Full implementation reference
- [This Week in Bevy: Parallel Transform Propagation](https://thisweekinbevy.com/issue/2025-03-03-upgrade-to-rust-2024-and-parallel-transform-propagation) - Rust 2024 parallelization
- [Bevy Transform Hierarchy Stress Test](https://github.com/bevyengine/bevy/blob/main/examples/stress_tests/transform_hierarchy.rs) - Performance testing

## Key Insights

### Bevy's Three-Stage Pipeline (Current State of the Art)

Bevy 0.15+ (2024-2025) uses a sophisticated three-stage pipeline:

```rust
// 1. Update entities without parents (roots)
sync_simple_transforms(world)

// 2. Propagate "dirty" bits up the hierarchy
mark_dirty_trees(world)

// 3. Propagate transforms through the hierarchy
propagate_parent_transforms(world)
```

### 1. Static Scene Optimization

**Concept**: Track which subtrees haven't changed and skip them entirely.

**Implementation**:
- Uses a `TransformTreeChanged` component as a "dirty bit"
- Propagates dirty bits upward during `mark_dirty_trees`
- `propagate_parent_transforms` skips entire subtrees if `!tree.is_changed()`
- Configurable threshold (default: 30% - if more than 30% of entities move, disable optimization)

**Impact**: Massive performance win for static scenes with few moving objects.

```rust
pub struct StaticTransformOptimizations {
    threshold: f32,  // Default: 0.3 (30%)
    enabled: bool,
}
```

### 2. Parallel Propagation (2-5x faster)

**Concept**: Use a work queue with batched tasks for parallel hierarchy traversal.

**Implementation Details**:
- **Work Queue**: `std::sync::mpsc` channel with batched tasks (CHUNK_SIZE = 512)
- **Workers**: Task pool threads consume parent entities, push children to queue
- **Traversal**: Depth-first with task sharing - continues locally while pushing branches to queue
- **Safety**: Assertions to detect cycles before they cause mutable aliasing

**Pseudocode**:
```rust
fn propagate_parallel(world: &mut World) {
    // Seed queue with root entities
    for root in roots {
        queue.push(root);
    }

    // Spawn workers
    thread_pool.scope(|s| {
        for _ in 0..num_threads {
            s.spawn(|| worker(queue, world));
        }
    });
}

fn worker(queue: &WorkQueue, world: &mut World) {
    while let Some(parent) = queue.pop() {
        // Propagate to children
        for child in parent.children {
            child.world_transform = parent.world_transform * child.local_transform;
            queue.push(child);  // Add to queue for parallel processing
        }
    }
}
```

**Performance**: 2-5x faster than serial for deep hierarchies (10+ levels).

### 3. Changed Tracking

**Concept**: Only propagate transforms that actually changed.

**Implementation**:
- Uses `Changed<Transform>` and `Added<Transform>` query filters
- Early exit: `if !transform.is_changed() && !global_transform.is_added() { continue; }`
- Entities without parents get updated immediately (no hierarchy overhead)

**Impact**: Significant CPU savings for static/semi-static scenes.

## Katla's Current Implementation

### Architecture

```rust
// Components (pure data)
pub struct TransformComponent {
    pub transform: Transform,  // Local-space
}

pub struct WorldTransform {
    pub transform: Transform,  // World-space (cached)
}

pub struct Parent {
    pub parent: EntityId,
}

pub struct Children {
    pub children: Vec<EntityId>,
}

// System
pub struct TransformHierarchySystem;

impl System for TransformHierarchySystem {
    fn update(&mut self, world: &mut World, _dt: f32) {
        // 1. Collect all transform entities
        let entities = collect_transform_entities(world);

        // 2. Topological sort (parents before children)
        let sorted = topological_sort(&entities);

        // 3. Update world transforms in order
        for entity in sorted {
            let world_transform = calculate_world_transform(world, entity);
            world.add_component(entity, WorldTransform::new(world_transform));
        }
    }
}
```

### Design Decisions

| Aspect | Choice | Rationale |
|--------|--------|-----------|
| **Caching** | `WorldTransform` component | Avoid O(depth) lookups during rendering/physics |
| **Traversal** | Topological sort | Clean single-pass, easy to understand |
| **Cycle detection** | Log warning + skip | Prevents infinite loops without panicking |
| **Mutability** | Components are pure data | Follows ECS best practices |
| **Parallelism** | None (yet) | Keep simple now, optimize later when needed |

### Performance Characteristics

- **Time Complexity**: O(N + E) where N = entities, E = parent-child edges
- **Space Complexity**: O(N) for HashMap storage during traversal
- **Cache Behavior**: Good - single linear pass through sorted entities

## Upgrade Path: Simple → Parallel

The good news: **Upgrading is easy because the public API stays the same**.

### What Stays The Same

```rust
// These components never change
pub struct TransformComponent { pub transform: Transform }
pub struct WorldTransform { pub transform: Transform }
```

All other systems (rendering, physics, etc.) just query `WorldTransform` - they don't care how it's computed.

### What Changes

Only the internal implementation of `TransformHierarchySystem::update()`:

```rust
// Simple version (current)
impl System for TransformHierarchySystem {
    fn update(&mut self, world: &mut World, _dt: f32) {
        // Topological sort, single-threaded
    }
}

// Parallel version (future)
impl System for TransformHierarchySystem {
    fn update(&mut self, world: &mut World, _dt: f32) {
        // Work queue, rayon tasks, etc.
    }
}
```

## Implementation Recommendations

### Phase 1: Current (Simple Topological Sort)

**Status**: ✅ Complete

**Features**:
- Topological sort for correct ordering
- Cycle detection with warnings
- O(N) single-pass update

**Limitations**:
- Updates all entities every frame
- No dirty tracking
- Single-threaded

### Phase 2: Add Changed Tracking (Low Hanging Fruit)

**Effort**: Low (1-2 hours)
**Impact**: High (30-50% CPU savings for typical scenes)

**Implementation**:
1. Add `Changed<T>` query filter to katla_ecs
2. Skip entities with unchanged local transforms
3. Propagate dirty bits upward (mark all ancestors as dirty)

```rust
for (entity, transform) in world.query::<(Changed<TransformComponent>)>() {
    // Mark this entity and all ancestors as dirty
    mark_dirty_tree(world, entity);
}
```

### Phase 3: Static Optimization (Medium Effort, High Impact)

**Effort**: Medium (4-6 hours)
**Impact**: Very High for static scenes (90%+ savings)

**Implementation**:
1. Add `TransformTreeChanged` component (dirty bit)
2. Track percentage of moving entities
3. Skip entire subtrees if dirty bit is not set

### Phase 4: Parallel Propagation (High Effort, Medium Impact)

**Effort**: High (1-2 days)
**Impact**: 2-5x for deep hierarchies

**Implementation**:
1. Add work queue (`std::sync::mpsc` or crossbeam)
2. Parallel depth-first traversal
3. Batched task distribution (CHUNK_SIZE = 512)
4. Cycle detection assertions

**Prerequisites**:
- `rayon` or `bevy_tasks` for thread pool
- Careful unsafe code for concurrent mutable access
- Comprehensive testing for race conditions

### When to Upgrade?

| Trigger | Metric | Action |
|---------|--------|--------|
| Profiling shows transform propagation bottleneck | >5% frame time | Add changed tracking (Phase 2) |
| Static scenes with few moving objects | <30% moving entities | Add static optimization (Phase 3) |
| Deep hierarchies (10+ levels) | Profile shows scaling issues | Parallelize (Phase 4) |
| Thousands of entities | >1000 transforms | Consider all phases |

## Alternative Approaches Considered

### Calculate on Demand (Rejected)

**Idea**: Traverse hierarchy upward when rendering, instead of caching.

**Pros**:
- Less memory (no `WorldTransform` component)
- Always up-to-date (no cache invalidation)
- Simpler code

**Cons**:
- O(depth) cost per query
- Re-calculates if queried multiple times per frame (rendering, physics, culling)
- Can't parallelize easily

**Verdict**: Caching is better for performance-critical engines.

### Deferred Updates (Rejected)

**Idea**: Batch transform updates and run them once per frame at the end.

**Pros**:
- Can coalesce multiple changes
- Batch processing friendly

**Cons**:
- Systems see stale transforms during update
- Confusing ordering dependencies
- Hard to reason about

**Verdict**: Predictability is more important than minor batching gains.

## Lessons from Industry

### Unity DOTS Recommendations

- Group ~50 GameObjects per root for optimal performance
- Avoid deep hierarchies (>10 levels)
- Use static optimization for scenes with <30% moving objects
- Profile before optimizing

### Bevy's Experience

- Parallelization was worth the complexity (2-5x improvement)
- Static optimization has high ROI for typical games
- Changed tracking is essential for good performance
- Cycle detection is necessary for safe parallel traversal

## References

### Code
- [Bevy Transform Systems](https://raw.githubusercontent.com/bevyengine/bevy/main/crates/bevy_transform/src/systems.rs)
- [Bevy Stress Test Example](https://github.com/bevyengine/bevy/blob/main/examples/stress_tests/transform_hierarchy.rs)

### Articles
- [Arjon Nagelhout: Game Engine from Scratch #3](https://arjonagelhout.nl/writings/2024-01-20_game_engine_03/)
- [Unity Spotlight Team: Optimizing Hierarchy](https://unity.com/blog/engine-platform/best-practices-from-the-spotlight-team-optimizing-the-hierarchy)
- [This Week in Bevy: Parallel Propagation](https://thisweekinbevy.com/issue/2025-03-03-upgrade-to-rust-2024-and-parallel-transform-propagation)

### Documentation
- [Unity ECS Transform Concepts](https://docs.unity3d.com/Packages/com.unity.entities@1.0/manual/transforms-concepts.html)
- [Bevy Cheat Book: Transforms](https://bevy-cheatbook.github.io/fundamentals/transforms.html)
- [Bevy Hierarchy Crate](https://docs.rs/bevy_hierarchy)

### Videos
- [Demystifying Transforms in Unity ECS](https://www.youtube.com/watch?v=NGLVVI2HAo4)
- [Architecting Bevy with Alice Cecile](https://www.youtube.com/watch?v=PND2Wpy6U-E)

---

**Last Updated**: 2025-02-06
**Engine Version**: Katla 0.1.0
**Related Components**: `TransformComponent`, `WorldTransform`, `Parent`, `Children`
**Implementation**: `katla_app/src/systems/transform_hierarchy_system.rs`
