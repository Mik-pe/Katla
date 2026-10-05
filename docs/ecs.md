# ECS ownership and system authoring

[odin/ecs](../odin/ecs) is the canonical CPU ECS. It owns generational identities,
paged sparse component columns, resources, event logs, prepared typed queries,
structural commands and scheduling. It imports no graphics, application, script
VM or editor policy. Optional reflection/tool/history support lives in
[odin/editor](../odin/editor); application authority lives in
[odin/app](../odin/app). The complete API and ownership rules are in
[Odin ECS](ecs_odin.md).

## Identity, storage and value ownership

An `Entity_Id` contains a 32-bit slot and 32-bit generation. Lookups compare the
complete identity. Clearing/destroying entities invalidates live generations;
exhausted slots retire permanently. Each component type owns a paged sparse set
with 1024-entry pages and dense swap removal. Queries start from the smallest
participating column and preserve its dense order.

Insertion transfers ownership. Owned strings, arrays and nested allocations need
registered `Value_Ops` destroy/deep-clone callbacks before insertion. The same
rule applies to owned resources, events, deferred values and persistent local
state. Plain numeric components and non-owning handles need no such hooks.
Keep `World` and its worker pool stationary through destruction, and use a
thread-safe allocator when parallel execution is enabled.

## Typed work and exclusive authority

`register_typed_system` derives access claims from the parameter struct. Queries
compose `Read(T)`/`Write(T)` with structural filters; parameters can also contain
required/optional resources, local state, event readers/writers and commands.
Duplicate writes and read/write aliases reject registration. Component,
resource and event claim namespaces remain separate.

Before a batch starts, the caller resolves rows and resource entries and freezes
structural registries. Workers receive only their prepared parameters and owned
system state. They must not capture `World`, registries, caller-thread VM owners
or unrelated mutable state. `register_exclusive_system` is the caller-thread
barrier for full-World operations, scene hierarchy, physics and script authority.

Odin does not enforce Rust lifetimes or `Send`/`Sync`. Read copies can contain
shallow nested pointers, so callers must preserve read-only semantics. Borrowed
rows, resource pointers and event slices expire at the end of preparation or
before structural/log mutation. End direct queries before mutating structure.
All chunk workers join before their preparation scope ends.

## Scheduling, visibility and failure

Execution-order tiers are strict barriers. Conflicting equal-order systems run
in registration order; independent equal-order typed systems may run in parallel.
The default pool threshold is 32,768 estimated entity-system visits; smaller
work runs on the caller. Explicit query chunking is useful only when per-row
work justifies it.

Each system owns a FIFO structural command queue. After borrowed data is
released, queues apply in registration order and then enqueue order. The next
batch sees changes; the same batch observes its initial structure. Spawn IDs
become live when commands apply. Stale command targets are ignored.

A recoverable `System_Error` joins workers and discards the failed batch's
unapplied commands. Earlier completed batches and component writes remain;
`world_update` is not a transaction. An Odin panic terminates rather than
unwinding and restoring execution. Do not describe panic recovery as an API
contract.

Query membership caches follow the structural epoch, but allocation addresses
are freshly resolved. Typed writes mark matched rows changed; direct mutable
queries conservatively mark entire columns. Changed-query combinations use union
semantics. Change tracking means mutable access or insertion, not value comparison.
Per-tick lifecycle/change state clears only after successful updates.

## Application and history boundaries

`Scene_Transform` stores parent-local TRS and `Scene_Parent` identifies its parent.
The application resolver supplies exact column-major world matrices, including
hierarchy-induced shear, to rendering, bounds, lights, audio, particles and
physics. Hierarchy validation/reparenting is application policy, not ECS storage.

The reflected registry must outlive its snapshots and Undo groups. History owns
cloned component values and mandatory apply/destroy/remap callbacks. Restoring a
destroyed entity allocates a fresh generation and remaps registered references
and history; it does not revive stale IDs. UI gestures and agent mutations share
one application action history. Persistent source codecs remain separate from
exact owned Undo snapshots. See [agent/application authority](agent_odin.md) and
[editor ownership](odin_editor.md).

## Author and verify

The runnable [movement example](../odin/examples/movement) composes typed math
components with the scheduler. [Odin ECS](ecs_odin.md) includes a complete system
example and API mapping. Run the independent ownership/scheduler checks with:

```sh
odin test odin/ecs -vet -strict-style
odin test odin/editor -all-packages -vet -strict-style \
  -sanitize:address -define:ODIN_TEST_THREADS=1 \
  -define:ODIN_TEST_FAIL_ON_BAD_MEMORY=true
odin run tools/build -- --tests
```

Tests cover stale IDs, page/dense churn, alias rejection, change tracking,
command order, recoverable failures, actual worker overlap, exclusive thread
identity, events, deep ownership and fresh-reference Undo/Redo. Configured app
checks prove native physics/script consumers. Rendering needs separate native
GPU acceptance. [Benchmark records](ecs_benchmarks.md) describe measured storage
and compilation experiments; historical Rust measurements are not current
engine build commands.
