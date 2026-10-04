# Luau scripting architecture

The canonical script runtime is [odin/script](../odin/script), backed directly
by the pinned [Luau dependency](../tools/luau_native/README.md). The dependency
owns the compiler, VM allocator and protected C ABI. The Odin runtime owns
instances, userdata, immutable snapshots, subscriptions, diagnostics, reload
and deferred commands. It imports math and the VM facade, with no application,
ECS, graphics or UI dependency.

[Application adapters](../odin/app/script_native.odin) retain scene authority.
`script_native_init` installs the real stationary runtime; missing or
incompatible libraries return errors. Physics uses the canonical
[Box3D owner](physics-engine-adr.md), and audio, input, animation, particles,
resource capabilities and renderer membership remain app-owned. There is no
Rust script/scene bridge in the canonical initialization path.

## Thread, memory and language boundary

Create, access, reset and destroy a runtime on its owner thread. Full-World
integration runs on the caller thread, outside prepared ECS worker batches.
Destroy outputs and VM references before unloading the dependency.

Entity userdata preserve complete u64 generational IDs without converting them
to floating-point numbers. Native math userdata validate their arguments and
protect their metatables. Per-instance environments inherit sandboxed read-only
globals. Debug, filesystem, package/require and process/environment access are
absent; scripts obtain explicit host capabilities through `world`.

The VM has a 128 MiB native allocation limit. Outer protected calls permit
10 million interrupt safe points and five seconds of wall time; nested host calls
share the budget. Ten consecutive failed ticks disable an instance. Allocation,
metamethod and execution failures return typed diagnostics. A Lua error never
jumps across an active Odin callback: Odin returns and completes its defers
before the native trampoline raises the pending error.

## Tick snapshots and deferred commands

The host supplies immutable entity, local-transform, component-name, input,
event and prior-query snapshots. `on_spawn(entity, world)`,
`on_update(entity, world, dt)` and `on_destroy(entity, world)` run through protected
calls. A retained world proxy expires when its tick or instance ownership ends;
it cannot become an unrestricted live ECS pointer.

Script mutations append an ordered command stream. The app applies completed
commands through its real owners: scene transforms/spawn/removal, particle
activation/bursts, sound, force/impulse/velocity and native spatial queries.
A command failure produces diagnostics; scripts cannot claim successful native
work by emitting an unsupported packet. Query tickets identify owner and index,
and their results arrive in the next host tick.

Event subscriptions are per instance. Emitted payloads remain VM-owned until
released. Trigger and animation events preserve ordering and complete entity
identity; events emitted while delivering callbacks defer to subsequent delivery
rather than recursively reentering a callback. Failed hooks roll back their
queued effects. Console packets and diagnostics have explicit ownership and
destruction APIs.

Runtime entity removal prepares renderer membership and native physics removal
before retiring ECS generations and associated constraints. Stale authored
trigger references during Play produce diagnostics. Stop restores the authored
snapshot with fresh entity/reference mapping. Editing deletion instead uses the
shared authored Undo transaction. The application owns Play/Pause/Resume/Stop
and input focus; scripts do not select editor policy.

## Sources, inspector and replacement

Bare resource names normalize beneath `scripts/` with a `.luau` extension.
Resource/Project sources retain relative identity. Explicit File sources use an
exact parent-directory and basename capability acquired during scene preparation.
Reads/reloads reject symlink children, traversal, invalid UTF-8 and sources above
1 MiB. Failed preparation revokes unpublished capabilities.

`script_native_sync` prepares all source replacements before retiring live
instances. Compilation or publication failure retains accepted environments and
scalar state. `script_reload` forces a real source attempt; inspector APIs expose
and edit owned scalar variables using generation-bound instance handles. Console
logs drain through the app. Code-editor validation compiles actual sandboxed
bytes and cannot replace a live instance on failure.

The language-facing declarations are
[resources/scripts/katla.d.luau](../resources/scripts/katla.d.luau).
[Actual scripts](../resources/scripts) and the
[direct runtime acceptance](../odin/app/script_native_test.odin) demonstrate useful
application behavior, including trigger-driven particle commands.

## Build and acceptance

Use [the canonical build](odin_build.md) for complete dependency setup:

```sh
python3 scripts/build_katla_odin.py --tests
python3 scripts/validate_odin_luau.py
```

The script validator builds pinned VM, Box3D and audio dependencies, then runs
actual language/runtime and application consumers in normal and ASan modes.
The dependency builder also supports isolated outputs:

```sh
python3 scripts/build_odin_luau.py --sanitize --output /absolute/path/to/libkatla_luau_asan
```

Choose the host's `.dylib`, `.so` or `.dll` filename. Tests require the matching
explicit `LUAU_LIBRARY`, and application tests also require matching native
physics/audio artifacts. Odin and native ASan must use the same LLVM major.
The convenience validator currently runs on Darwin/Linux; a Windows build or
cross-target typecheck does not establish native Windows execution.

Required observations include real Luau execution, exact IDs, proxy expiry,
event ordering, hook rollback, budget/OOM recovery, rejected reload retaining
state, native ray/velocity/force/audio commands and Play/Stop fresh restoration.
GPU particle output is proved separately by
[particle acceptance](particles_odin.md). Generic command construction alone is
not application or native-device acceptance.
