# Odin graphics foundation

`odin/gfx` preserves an independent GPU core while the complete renderer
migration continues in [TODO](../TODO.md#odin-port). It imports no ECS, math or
editor packages. See [the agent foundation](agent_odin.md) for scene host calls.

## GPU core


`Resource_Storage(T, Kind)` owns slot memory while transferring resource values
on insertion/removal. A handle includes its stationary storage owner, slot and
generation; zero, foreign, removed and stale-generation handles fail lookup.
Generation exhaustion permanently retires a slot. Native owners must remove and
release every live value before `storage_destroy`, which asserts empty storage.
Do not copy owners or retain handles beyond their owner's lifetime.

`Frames` tracks acquisition generations and accepted submission numbers:

```
Idle -> Acquired -> Recorded -> Submitted -> Idle
          |           |
          +-- abort --+
```

A busy slot requires native completion before acquisition. `frame_submitted`
is called only after the backend accepts a recording; rejection instead aborts.
`frame_completed` checks the exact token and submission. Submitted work cannot
abort, and rejected work creates no submission. Native resources/surfaces remain
the backend's responsibility. Drain work and abort acquisitions before teardown.
Keep each owner at a stable address throughout its lifetime.

`Buffer_Graph` owns buffer descriptions and immutable pass declarations. Each
access carries a real byte range, read/write mode and permitted use. Addition
validates owner identity, bounds without arithmetic overflow, usage, unique names
and nonoverlapping accesses within one pass before changing the graph. Compute
accepts storage or read-only uniforms; transfer accepts source reads and
destination writes. Use one `Read_Write` access for overlapping in-place compute.

Compilation preserves authored order, checks read initialization (including
coverage from multiple adjacent writes), culls passes unrelated to an export or
explicit side effect, and emits all overlapping read/write, write/read and
write/write hazards among live passes. Read/read and disjoint ranges need no
hazard. The plan is conservative: overwritten writers and redundant transitive
hazards can remain. Dead declarations still undergo initialization validation.
Imported buffers promise already initialized bytes; transient buffers require
prior writes. Exported buffers keep their writers live and require complete initialized byte
coverage, since external consumers may read the whole buffer. Graph identity and
resource declarations must outlive a compiled plan.

This first compiler covers buffer compute/transfer dependencies. It does not
allocate native memory, compile shaders, alias storage, execute graphics passes,
track image subresources, reflect binding layouts or provide a complete renderer.
Native command packets and their resource-access validation are the next port
step; no default backend callback pretends to execute unsupported work.

## Native acceptance and checks

`odin/gfx_native` is a macOS arm64 acceptance consumer. It uses direct Objective-C
calls from Odin, with Metal 4 selectors defined by Apple's current SDK; there is
no Rust FFI bridge or legacy command encoder. It compiles two MSL kernels through
`MTL4Compiler`, creates a residency set and immutable buffer argument table, then
consumes the compiled pass order/hazards with separate Metal 4 compute encoders.
The declared graph fills 1,024 integers, transforms them in place, and transfers
them to a shared readback buffer. Consumer barriers use the source/destination
stages and device visibility. Exact commit feedback gates slot/allocator reuse;
errors fail acceptance. Owners remain alive until feedback and readback finish.
The harness binds a fixed workload, so it does not establish generic packet
preflight or shader reflection acceptance.

```sh
odin test odin/gfx -out:target/odin-gfx-tests -vet -strict-style
odin build odin/gfx_native -out:target/odin-gfx-native -vet -strict-style
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 target/odin-gfx-native
python3 scripts/validate_odin.py --native-metal
```

With Odin dev-2026-09:a2fb372b7 on the Apple M5 host with macOS 27 and the current Xcode SDK, eight submissions
verify all 8,192 output values with Metal API validation enabled. Both new CPU
packages pass native debug AddressSanitizer and optimized tests with strict
vet/style checks and test allocator leak tracking; they typecheck for
`linux_amd64`. Agent tests include actual multithreaded limiter admission, JSON
rejection before queue mutation, selection generations and undo. Graphics tests
cover stale/foreign handles, generation exhaustion, exact completion, rejection,
range hazards, dead-pass culling, initialization gaps and whole exported-buffer
initialization. The Rust reference also passes its 14 native compute tests with
Metal API validation and strict all-target check/Clippy. GPU acceptance remains
limited to this Metal 4 workload: Vulkan, windowed presentation, images, shader
reflection, draw output, native replacement/readback lifetime and full application
integration remain pending.

The portable validation script always runs the new CPU tests and actual agent
consumer. Native Metal is explicit via `--native-metal`, requires macOS arm64,
sets both validation environment variables before launch and fails on an absent
or unsupported device. It never counts a CPU test as native acceptance.
