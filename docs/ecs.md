# ECS ownership and system authoring

Katla stores components in per-type paged sparse sets (1024-entry pages). Queries select the smallest column and preserve its dense order. Entity IDs contain an index and
its generation. Clearing a world invalidates its live IDs without resetting
generations; exhausting a generation retires the slot. Sparse lookups compare
the complete ID, so a stale handle cannot address a replacement entity.

## Decision: typed parallel access and exclusive World access

A worker never receives `World`, a storage registry, or a resource registry.
`TypedSystem::Params` is the only source of its access metadata. Engine-owned,
sealed parameter implementations derive read/write claims and registration
rejects conflicting claims within a system, including across separate queries.

Before dispatch, the caller thread resolves independently borrowed component
and resource entries. The registries are frozen until every prepared job has
returned. Component entries use independent interior-mutable cells rather than
repeated exclusive borrows of a shared map. Resource parameters require `Sync`
for shared access and `Send` for mutable ownership transfer. Non-send resources
can be used by exclusive systems on the caller thread.

Full-World systems implement `System` and register through the explicitly
exclusive path. The script VM, physics synchronization, and transform hierarchy
use this path. They are barriers in the same schedule as typed systems and are
never transferred to Rayon. This preserves thread affinity without an unsafe
exception to worker access rules.

Manually declared access was rejected because an omitted or incorrect claim
could allow data races. A global World mutex was rejected because it serializes
independent systems. Duplicating World into worker snapshots was rejected because
it changes resource, event, and entity identity semantics and adds merge costs.
Archetype storage is a separate measured decision; see [benchmarks](ecs_benchmarks.md).

## A typed system

```rust
use katla_ecs::{Component, Query, Read, SystemExecutionOrder, SystemParam,
                TypedSystem, World, Write};

#[derive(Component)]
struct Position(f32);
#[derive(Component)]
struct Velocity(f32);
struct Movement;

impl TypedSystem for Movement {
    type Params = Query<(Write<Position>, Read<Velocity>)>;

    fn run(&mut self, mut query: <Self::Params as SystemParam>::Item<'_>, dt: f32) {
        for (_entity, (position, velocity)) in query.iter_mut() {
            position.0 += velocity.0 * dt;
        }
    }
}

let mut world = World::new();
world.spawn((Position(0.0), Velocity(1.0)));
world.register_typed_system(Movement, SystemExecutionOrder::NORMAL);
world.update_parallel(0.016);
```

Compose parameters as tuples. `Res<T>` and `ResMut<T>` require the resource to
exist; `Option<Res<T>>` and `Option<ResMut<T>>` permit absence while preserving
the same scheduling claim. `Local<T>` is persistent, isolated system state.
`Commands` queues structural work. Event readers and writers are typed resource
claims, so dependent readers run after writers. Each reader has its own cursor;
clear the typed event log after consumers finish a retention interval. Sequence
numbers preserve subsequent events, and replacing a log resets readers. Camera and animation systems
provide complete application examples.

Query descriptors support tuples through arity eight, mixed read/write access,
and disjoint `With<T>` / `Without<T>` filters. Query membership and dense offsets are cached until
structural changes invalidate the world epoch. Allocation bases are resolved
for each batch, so vector reallocation, replacement, removal and slot reuse
cannot leave a cached address pointing at old data. Returned component references
borrow their query view; retaining them prevents a second mutable view borrow.

`par_for_each_mut(chunk_size, callback)` partitions unique matched rows and joins
before returning. It never changes the shared storage metadata from chunk
workers. Typed mutable queries conservatively mark matched entities changed
before dispatch, including rows a callback only reads. Direct mutable World
queries mark their entire mutable columns before lazy iteration, including
entities excluded by the join or filter. Selecting an entire column uses a
constant-time dirty flag. Change detection means
“mutable access requested or inserted,” rather than comparison of values.
Multi-component changed queries retain union semantics.

## Scheduling and structural visibility

Execution order values are strict barriers. Equal-order systems preserve
registration order for conflicts. Independent equal-order typed systems may run
concurrently. Sequential and parallel updates consume the same schedule, with
small workloads executed on the calling thread. The default cutoff is 32,768
estimated entity-system visits per batch; `set_parallel_work_threshold` allows
applications to tune it to their work. Query chunk parallelism is explicit, so
use sequential iteration when per-row work is small.

Each system has its own FIFO command queue. After a batch releases all borrowed
data, queues apply in registration order, then enqueue order within each queue.
The next batch sees the changes; systems in the same batch see the pre-batch
structure. Spawn allocates its ID when applied, rather than exposing a reserved
entity as live to another worker. Commands targeting stale IDs have no effect.

A panic aborts the tick and propagates to the caller after workers join. Systems
are restored, unapplied batch commands are discarded, and earlier completed
batches are not rolled back. Component mutations completed before the panic are
retained; the tick is not transactional.

## Lifecycle and editor behavior

Direct World operations and applied commands share one lifecycle path. Creation
emits `Spawned`; insertion emits `Added`, including replacement; removal emits
`Removed` only when present. Destruction removes components and invalidates the
entity before emitting `Destroyed`, with one `Removed` per removed component.
The per-frame entity/component event arrays are visible during the tick and clear
after a successful update, as before. Component values are unavailable after
removal. External-resource cleanup payloads and post-update lifecycle event
retention remain separate items in the ECS roadmap.

Editor inspection, scene tools, serialization, scripts and physics continue to
use the same generational World component API. Exclusive systems can perform
structural changes directly because no typed batch overlaps them. Clearing systems
during exclusive execution stops the remaining schedule and invokes shutdown once
at the boundary.

## Implementation boundaries

World owns identity, lifecycle events and the storage registries. Private modules
separate query construction, resource access, system execution and integrity
validation without changing the World API. Typed parameter families separate
resource borrows, local state, structural commands and event logs; shared access
validation and tuple composition remain together. Registration, query alias checks
and scheduling use the same read/write conflict rule: identical types conflict when
either access writes. Component and resource claims remain separate namespaces.

## Unsafe boundaries and verification

The remaining unsafe operations resolve independently claimed storage cells,
borrow preselected component addresses, and construct lifetime-bound views.
The outer World storage cell keeps a filtered iterator's registry pointer valid
while its query borrows disjoint component columns. Removing that cell without
changing query preparation invalidates the pointer under Miri. These operations
do not create multiple exclusive references to World. Sealed descriptors
and parameter implementations keep pointer preparation unavailable to safe game
code. Every mutable row is unique and all references end before structural work.

CPU tests cover held query rows, duplicate claims, filtered access, cache churn,
commands, resource access, ordering, panics, events, change tracking and stale IDs.
Miri exercises the pointer and lifetime boundaries, including retained mutable
filtered rows and dense removal across sparse pages. Unit tests assert bulk
lifecycle and change-tracking results; performance belongs in benchmarks rather
than elapsed-time thresholds. Native app tests cover the
migrated camera/animation behavior and exclusive script thread affinity. See
[CI](ci.md) and [benchmarks](ecs_benchmarks.md) for reproducible validation.

## Migration

Use `TypedSystem` with `Params` for bounded component/resource work and register
it with `with_typed_system` or `register_typed_system`. Use `System` only for
full-World operations and register with `with_exclusive_system` or
`register_exclusive_system`. The old builder registration and manual static/dynamic
access declarations have been removed. Both runtime loops use the same scheduler.

## Application transform hierarchy

`TransformComponent` stores parent-local TRS; `Parent` is the authoritative link.
The app's iterative `resolve_world_transforms` resolves current locals in O(N),
without recursion or dependence on cached component update order. It preserves
exact composed matrices including shear. `TransformHierarchySystem` publishes
those matrices and accumulated rotation/scale as `WorldTransform`; direct local
edits, new entities and reparenting refresh even without dirty markers. Invalid
runtime cycles terminate with warnings and local poses; scene validation rejects
them before loading. Rendering, bounds, lights, audio, particles and physics use
the same resolver, including newly instantiated prefabs before the next ECS tick.
`TransformOptimization` avoids rewriting unchanged cached poses; it does not make
hierarchy traversal O(D). Incremental topology and dirty-root work remain in TODO.

## Odin port experiment

The standalone CPU port and compilation comparison live in `odin/ecs` and
`odin/editor`. See [Odin ECS](ecs_odin.md) for API mapping, dependency replacements,
ownership differences and reproducible measurements. The Rust engine continues
to use this crate.
