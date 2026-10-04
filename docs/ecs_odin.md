# Katla ECS in Odin

The shared [Odin tree](../odin/README.md) now also contains the independent
[math port](math_odin.md) and a runnable `odin/examples/movement` consumer that
composes math components with typed ECS systems.

The Odin port lives in `odin/ecs`, with the optional reflection/scene/agent layer
in `odin/editor`. It is a standalone CPU library. Katla's Rust application,
renderers, scripting and physics still use `katla_ecs`; this experiment does not
replace their language or introduce an FFI bridge. Rust remains the comparison
implementation requested for the compilation measurements.

## Dependencies

| Rust dependency | Odin implementation |
| --- | --- |
| `katla_derive`, `syn`, `quote`, `proc-macro2`, `unicode-ident` | Polymorphic component types plus `core:reflect` RTTI; no proc-macro build |
| Rayon and Crossbeam | `core:thread` pool and `core:sync`; prepared, disjoint jobs and joined query chunks |
| Optional `serde_json` and its dependencies | `core:encoding/json` in the separately imported editor package |
| Rust collections/allocation | Odin maps/dynamic arrays and aligned `core:mem` allocations |

There is no ECS dependency on `katla_math` or `katla_gfx`, so neither is needed
for this port. The normal dependency graph in the Rust manifests remains intact.
The standard Odin packages ship with the compiler; no third-party Odin package
or foreign library is required by these ECS packages.

## Contracts and API mapping

| Contract | Odin API |
| --- | --- |
| 32-bit slot + 32-bit generation; stale-ID rejection; exhausted-slot retirement | `create_entity`, `entity_exists`, `destroy_entity`, `clear_entities` |
| One sparse column per component type, 1024-entry pages, dense swap removal | `register_component`, `add_component`, `remove_component` |
| Struct component bundles, up to eight fields | `spawn` |
| Read, write, smallest-column joins and disjoint structural filters, arities 1–8 | `Read(T)`, `Write(T)`, `Query(Row, Filter)`, `With(T)`, `Without(T)` |
| Membership cache invalidated by structural epoch; addresses freshly resolved | Prepared typed queries; `query_row` checks cached membership |
| Direct mutable queries mark entire columns; typed writes mark matching rows; changed-query union | `query_begin` with `direct` / `changed_only`, `component_changed`, `clear_changed` |
| Global resources, required/optional resource parameters, persistent local state | Resource APIs including factory/ownership transfer, `Res`, `Res_Mut`, `Optional_Res`, `Optional_Res_Mut`, `Local`, `initialize_local` |
| Per-reader cursor, retained event sequence, replacement identity | `Events`, `Event_Reader`, `Event_Writer`, clear/replace event APIs |
| Derived access validation; separate component/resource/event namespaces | `register_typed_system` reflects its parameter struct; read/write aliases reject registration |
| Strict order tiers, registration order for conflicts, independent workers, small-work fallback | `world_update`, `parallel_work_threshold`, `set_system_enabled` |
| Exclusive full-World work stays on caller thread | `register_exclusive_system` |
| Per-system FIFO commands applied in registration order after the batch; stale targets ignored | `Commands`, `command_spawn`, `command_insert`, `command_remove`, `command_destroy` |
| Failure joins workers, discards the failed batch's unapplied commands; earlier batches remain | `System_Error` returned by callbacks and `world_update` |
| Lifecycle arrays and change flags clear after successful ticks | `entity_events`, `component_events`, `world_update` |
| Integrity validation and empty-entity cleanup | `validate`, `cleanup_empty_entities` |
| Component field metadata and runtime JSON mutation | `editor_register`, `editor_fields`, `editor_set_field` |
| Spawn/destroy/duplicate, component add/remove, query/list/attributes, typed owned undo commands | `scene_execute`, `Undo_Group`, `undo_group`, `redo_group` |
| Observations, action history, synchronous agent, queued background requests/results and bounded tick work | `Agent_Session`, `agent_run_sync`, `Agent_Harness`, `agent_tick` |

`Spawn_Model` and `Set_Parent` return `Application_Owned`. Loading graphics assets
and defining a transform hierarchy belong to the application; this package does
not pretend those operations succeeded. Scene hierarchy/query operations return
entity lists, as the CPU ECS has no application-owned Parent component. Spatial
queries, transform-aware duplicate offsets, resource-file tools and graphics
asset loading are not ECS implementations in the Rust crate either.

## Authoring

```odin
package main

import ecs "path/to/odin/ecs"

Position :: struct { x: f32 }
Velocity :: struct { x: f32 }
Row :: struct { position: ecs.Write(Position), velocity: ecs.Read(Velocity) }
Params :: struct { query: ecs.Query(Row, ecs.No_Filter) }

movement :: proc(_: ^int, params: ^Params, dt: f32) -> ecs.System_Error {
    for id in ecs.query_entities(&params.query) {
        row, ok := ecs.query_row(&params.query, id)
        assert(ok)
        ecs.write(row.position).x += ecs.read(row.velocity).x * dt
    }
    return .None
}

main :: proc() {
    world: ecs.World
    ecs.world_init(&world, worker_count = 4)
    defer ecs.world_destroy(&world)
    ecs.spawn(&world, struct { p: Position, v: Velocity }{Position{0}, Velocity{1}})
    _, err := ecs.register_typed_system(&world, 0, Params, movement)
    assert(err == .None)
    assert(ecs.world_update(&world, 0.016, parallel = true) == .None)
}
```

A parameter struct composes any number of queries, resource wrappers, local
values, event wrappers and command queues. The library derives their claims;
there is no manual component/resource access list. Use `query_begin` / `query_end`
for caller-thread direct queries. `Read(T)` returns a copy through `read`;
`Write(T)` returns the current prepared pointer through `write`.

Editor field annotations use Odin struct tags, for example:

```odin
Transform :: struct {
    x: f32 `display_name:"X position" min:"-10" max:"10" speed:"0.1"`,
    internal: i32 `inspect:"skip"`,
    color: [4]f32 `inspect:"color"`,
    target: ecs.Entity_Id `inspect:"entity_ref"`,
}
```

Enum variants and nested/array field kinds come from RTTI. Editor registration
owns its default value and metadata. Scene results, undo groups, observations
and sessions have explicit destruction functions. The registry must outlive
its undo groups, because component snapshot names refer to registry metadata.
`Undo_Group` owns a command and its affected `entities` array, with mandatory
apply/destroy/target-remap callbacks. Built-in scene commands own JSON snapshots;
application material commands own a validated numeric batch. All scene snapshots
decode before restoration mutates World. The registry must remain alive until
history is released. Undo of destruction allocates a fresh generation; use the
group's resulting identity rather than reviving an old handle. Agent history
remaps earlier command targets after restoration; references inside arbitrary
serialized payloads still require application-specific ownership.

Owner-thread execution can supply an `Application_Executor` to scene sessions and
mailbox ticks. The host mailbox stores only owned operations/results and no
application executor state. Unsupported application calls return
`Application_Owned` when the owner supplies no executor. See the
[application authoring contract](agent_odin.md#material-requests-and-the-application-owner).

## Ownership and language differences

Keep World at a stable address from initialization through destruction. Pool
storage also stays stationary. The allocator supplied to a parallel World must
be thread-safe. Shutdown joins all pool threads before releasing allocations.

Component/resource insertion transfers ownership. Values containing owned
strings, arrays or other heap allocations need `Value_Ops` destroy and deep-copy
hooks. Register component hooks before insertion, supply matching hooks for
owned deferred values, register hooks for owned event logs before reader/writer
registration, and initialize owned Local state with its hooks. System
state can release its owned allocations in the shutdown callback. Plain numeric
components and non-owning handles need no ownership hooks.

Odin does not provide Rust lifetimes, immutable references, sealed traits or
`Send`/`Sync` checks. The port preserves scheduler/lifecycle behavior but does
not promise Rust's static safety guarantees. Caller code must use the public
operations and respect these rules:

- Keep read parameters and borrowed event slices read-only, including their
  nested pointer/container values. Ordinary Odin value copies are shallow.
- Do not retain query rows, resource pointers or event slices after their
  preparation scope or a structural/log mutation. End a direct query before
  manipulating its World.
- Do not capture World/registry pointers, caller-thread-only state or shared
  mutable state in typed system callbacks. Use exclusive systems for such work.
- Do not access implementation fields or construct arbitrary live IDs. Structural
  operations assert if invoked while a query batch is frozen.
- Treat invalid descriptors and allocation/exhaustion failures as fatal
  assertions. Recoverable tick failures return `System_Error`; an Odin panic
  terminates rather than unwinding and restoring the tick as Rust does.

## Validation and compilation measurements

```sh
mkdir -p target
odin test odin/ecs -out:target/odin-ecs-tests -vet -strict-style
odin test odin/editor -all-packages -out:target/odin-editor-tests -vet -strict-style
odin test odin/editor -all-packages -out:target/odin-ecs-asan \
  -sanitize:address -debug -vet -strict-style
python3 scripts/measure_ecs_compile.py --samples 5
```

Tests cover stale IDs, retired slots, sparse-page churn, lifecycle replacement,
filters/change tracking, cached-address churn, resource aliases/local ownership,
command order and visibility, recoverable failure, event cursors, real worker
overlap across multiple frames, conflicting/disabled systems, chunk joins,
exclusive thread affinity/shutdown, editor JSON/ownership/undo and actual agent
thread submission. Test memory tracking verifies ownership; AddressSanitizer
checks native address accesses. No rendering path changes, so these receipts
are CPU acceptance rather than native GPU evidence.

The build harness instantiates every query arity, spawns 2048 eight-component
entities, runs a typed movement system, destroys/reuses entities and validates
the resulting World. The editor variant additionally mutates a reflected field
and undoes it. Every timed build runs both resulting binaries outside the timed
interval and requires identical checksums, including after application edits.
A standalone Rust consumer avoids pulling Criterion or engine crates into
normal build measurements and retains the production dev/release profiles.

Measurements alternate build order, use five raw samples, isolated targets,
offline dependency sources, explicit Cargo/rustc binaries and disabled compiler
wrappers. `clean` means clean compiler artifacts, with warm filesystem and
registry-source caches. `dependencies_warm` keeps Rust dependencies/macros while
recompiling the ECS and consumer; Odin rebuilds the whole program. `library_edit`
changes the default work threshold. `application_edit` changes a numeric step.
`no_change` compares Cargo freshness with Odin's whole-program rebuild. Clean
`check` measurements exclude ECS/consumer code generation and linking, and use
the dev profile. Rust still builds the procedural-macro host dependency.

Rust uses LLVM 23 while this Odin toolchain uses LLVM 22. Release flags preserve
Rust's thin LTO/one codegen unit and Odin's whole-module speed profile; they are
practical build configurations, not identical optimization pipelines. Different
static safety guarantees, generic expansion and RTTI strategies also affect
compiler work. These observations measure this port and consumer on this host;
they do not establish language-wide or full-engine compilation ratios. The host
is shared, without CPU/frequency/cache pinning; raw ranges remain part of the
published evidence.

Raw observations and all source/toolchain hashes are recorded in
[CSV](benchmarks/ecs-odin-compile.csv) and
[environment and summary](benchmarks/ecs-odin-compile.json).

## Recorded results

Captured on 2026-10-03 on Apple M5 (10 logical CPUs, 24 GiB), macOS 27,
Rust 1.99.0 / LLVM 23.1.1 and Odin dev-2026-09:a2fb372b7 / LLVM 22.1.8.
All 220 compiler commands succeeded. The 100 paired executable checks matched
across languages; the remaining 20 commands were typechecks. All project source
hashes and the raw CSV hash were verified after capture. The JSON base commit
is the pre-port revision; source hashes identify the measured port independently
of its eventual commit.

Each cell is median [minimum–maximum] wall-clock seconds across five samples.
The ratio is Rust median divided by Odin median.

| Feature | Profile | Scenario | Rust seconds | Odin seconds | Rust / Odin |
| --- | --- | --- | --- | --- | --- |
| core | dev | clean | 2.700 [2.638–3.037] | 0.326 [0.320–0.335] | 8.29× |
| core | dev | dependencies_warm | 1.018 [0.990–1.027] | 0.326 [0.325–0.351] | 3.12× |
| core | dev | library_edit | 0.487 [0.474–0.492] | 0.346 [0.325–0.354] | 1.41× |
| core | dev | application_edit | 0.282 [0.280–0.293] | 0.336 [0.329–0.353] | 0.84× |
| core | dev | no_change | 0.135 [0.134–0.149] | 0.335 [0.326–0.344] | 0.40× |
| core | dev | check_clean | 2.240 [1.947–2.378] | 0.065 [0.061–0.066] | 34.33× |
| core | release | clean | 4.850 [4.238–4.968] | 1.634 [1.597–1.707] | 2.97× |
| core | release | dependencies_warm | 2.558 [2.496–2.627] | 1.412 [1.385–1.427] | 1.81× |
| core | release | library_edit | 2.547 [2.482–2.650] | 1.427 [1.414–1.577] | 1.79× |
| core | release | application_edit | 2.040 [1.963–2.462] | 1.414 [1.388–1.496] | 1.44× |
| core | release | no_change | 0.137 [0.136–0.139] | 1.401 [1.387–1.437] | 0.10× |
| editor | dev | clean | 3.528 [3.472–3.544] | 0.407 [0.397–0.413] | 8.67× |
| editor | dev | dependencies_warm | 1.365 [1.341–1.397] | 0.392 [0.384–0.403] | 3.48× |
| editor | dev | library_edit | 0.823 [0.638–0.996] | 0.552 [0.477–0.600] | 1.49× |
| editor | dev | application_edit | 0.491 [0.430–0.721] | 0.501 [0.436–0.568] | 0.98× |
| editor | dev | no_change | 0.191 [0.184–0.202] | 0.486 [0.465–0.524] | 0.39× |
| editor | dev | check_clean | 2.867 [2.810–3.188] | 0.077 [0.071–0.078] | 37.32× |
| editor | release | clean | 6.925 [5.841–7.128] | 2.384 [2.289–2.557] | 2.91× |
| editor | release | dependencies_warm | 3.030 [3.003–3.547] | 2.114 [2.074–2.289] | 1.43× |
| editor | release | library_edit | 3.036 [3.010–3.085] | 2.060 [2.050–2.098] | 1.47× |
| editor | release | application_edit | 2.021 [2.002–2.110] | 2.069 [2.055–2.099] | 0.98× |
| editor | release | no_change | 0.139 [0.136–0.142] | 2.086 [2.061–2.463] | 0.07× |

Clean dev builds favored Odin by 8.3× for the core and 8.7× with editor.
Clean release builds favored Odin by approximately 3×. The dependency-warm
case reduced the Rust difference substantially. Small dev consumer edits favored
Rust for core and were close for editor; release editor consumer edits were
also close. With no changes, Cargo's cached result was faster in every profile.
The raw spread, especially during shared-host activity, limits finer conclusions.

The final port passed 25 core tests and nine editor tests, 34 combined, with
strict Odin vet/style checks, test allocation tracking, AddressSanitizer and
optimized compilation. The Rust reference passed 239 library tests, two entity
lifecycle integration tests, 18 passing doctests (one additional ignored),
all-feature check, strict all-target Clippy and formatting. The standalone
Rust consumer passed strict Clippy and formatting.
