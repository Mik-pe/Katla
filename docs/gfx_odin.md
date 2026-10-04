# Odin graphics core

`odin/gfx` is an independent GPU core with executable buffer compute/transfer
packets. Native adapters live in `odin/gfx/metal` and `odin/gfx/vulkan`; they
import no ECS, math or editor packages. Complete application migration continues
in [TODO](../TODO.md#odin-port). See [the agent foundation](agent_odin.md) for
scene host calls. The superseded fixed Metal acceptance executor has been
replaced by a consumer of the reusable native adapter.

## GPU core

`Resource_Storage(T, Kind)` owns slot memory while transferring values on
insertion/removal. A handle includes its stationary storage owner, slot and
generation; zero, foreign, removed and stale-generation handles fail lookup.
Generation exhaustion permanently retires a slot. Remove and release every live
value before `storage_destroy`, which asserts empty storage. Do not copy owners
or retain handles beyond their owner's lifetime; keep owners at stable addresses.

`Frames` tracks acquisition generations and accepted submission numbers:

```
Idle -> Acquired -> Recorded -> Submitted -> Idle
          |           |
          +-- abort --+
```

`frame_submitted` publishes only accepted native work; rejection aborts its
acquisition. `frame_completed` requires the exact token and submission. Submitted
work cannot abort. Drain native work before teardown. Native adapters own three
recording slots in a ring. A busy next slot returns `Busy`; submitting does not
implicitly wait or retire another frame. `wait` or successful `poll` retires an
exact submission once. Reusing its retired identity returns `Invalid_Resource`.

`Buffer_Graph` owns descriptions, immutable pass declarations and replaceable
packets. Every declared access has an actual byte range, read/write mode and
permitted use. Declaration validates identity, overflow-safe bounds, usage,
unique pass names and nonoverlapping accesses within a pass. Compute supports
storage and read-only uniforms; transfer supports source reads and destination
writes. Use one `Read_Write` access for overlapping in-place compute.

Compilation preserves authored order, checks read initialization, culls passes
unrelated to an export or explicit side effect, and emits overlapping read/write,
write/read and write/write hazards between live passes. Read/read and disjoint
ranges need no hazard. The conservative plan can retain overwritten writers and
redundant transitive hazards. Dead declarations still undergo initialization
validation. Imports promise initialized bytes; transients need previous writes.
Exports require complete initialized coverage. A plan borrows its stationary graph;
adding a declaration invalidates the plan through its revision.

## Executable packets and preflight

`graph_set_packet` owns a cloned `Dispatch` or `Copy_Buffer` for one pass. Dispatch
sizes are workgroup counts; local size belongs to the pipeline. Every packet use
must match a declaration, and every declaration must have an exact packet use.
This prevents an authored full-buffer write from claiming initialization when a
packet binds only a smaller range. Packet replacement failure preserves previous
work. More operations use more ordinary passes, rather than implicit commands.

`graph_prepare` freezes live packets, mappings and hazards after mandatory native
queries. It checks actual allocation capacities/usages, stale and foreign handles,
unique graph roles, native shader slots/classes, minimum spans, offset alignment,
maximum descriptor ranges and dispatch limits. Mapping two graph roles to one
physical allocation is explicitly rejected; storage aliasing is still pending.
Missing callbacks, resources, packets or unsupported reflection reject execution
before acquiring a native frame. Replacing a packet after preparation leaves the
snapshot intact. Changing declarations requires recompilation.

Shader code and declared accesses remain application contracts: preflight cannot
prove a shader's dynamic indexing, branch behavior or actual memory operations.
Callers must declare every actual shader access and dispatch within its bound
ranges. Import initialization also remains the caller's responsibility.

## Native adapters

Both stationary adapters require calls from their owning thread. They implement
buffer/pipeline creation and destruction, CPU byte access,
preflight queries, submission, exact polling/waiting and teardown. Each accepted
slot retains its native buffers, pipeline layouts and immutable binding state.
Removing a public handle immediately prevents new lookups while pending work
retains its native owner. CPU reads/writes return `Busy` until all consumers of
that allocation retire, even if a different submission has already completed.
There is no hidden CPU wait between submissions.

Metal requires Metal 4 before creating a queue or compiler. It uses direct
Objective-C calls for APIs absent from Odin's bundled bindings, compiles MSL with
`MTL4Compiler`, and reflects used buffer bindings. Each dispatch owns an immutable
argument table; each recording owns a residency snapshot and command allocator.
Separate Metal 4 compute encoders consume graph order and source/destination
queue stages with device visibility. Conservative queue barriers also cover
persistent allocations used by earlier graphs. Commit feedback copies signed
native errors and diagnostic text; callback synchronization finishes before slot
cleanup can free its state. Terminal GPU errors block later submissions.

Vulkan requires 1.3 synchronization2 and maintenance4. Its loader, instance entry
points and device table remain with the renderer. On macOS it enables portability
enumeration/subset for MoltenVK. Buffer allocation requires coherent host-visible
memory and returns `Unsupported` when unavailable. Each frame owns its descriptor
pool, immutable descriptor sets, command pool and exact fence. Synchronization2
consumes compiled range hazards, conservatively orders earlier persistent queue
uses, and makes writes visible to host reads. A nonterminal wait/drain failure
preserves pending native owners; device loss permits terminal cleanup.

`Compute_Desc` supplies prepared MSL/SPIR-V targets, entry, local size and buffer
slot classes. Vulkan reflects a single compute entry, fixed local sizes and
set-zero uniform/storage layouts, including explicit scalar/vector/matrix/array
strides and struct member offsets. Textures, push constants, other sets, multiple
entries and unsupported type forms fail explicitly. The checked reflection parser
is a layout reader, not a complete SPIR-V validator; use compiler-produced modules.
A shared WGSL compiler and asynchronous shader replacement remain pending.

## Native acceptance and checks

`odin/gfx_native` and `odin/gfx_vulkan_native` use the same test-only scenario in
`odin/gfx_conformance`. Required backend function inputs exercise actual generic
packets, rather than a separate fixed workload executor. The scenario fills
1,024 integers, transforms them with a reflected uniform and copies to readback.
Three differently configured graphs share one persistent scratch allocation and
submit concurrently without CPU waits. Four rounds verify all 12,288 results per
backend, immutable binding state, ring exhaustion, out-of-order retirement,
polling, double-retirement rejection and CPU access exclusion. The final three
submissions retain removed scratch/pipeline identities through completion.
Invalid slots, uniform spans and alignment reject without creating a submission;
invalid shader slot metadata rejects pipeline publication.

```sh
python3 scripts/validate_odin.py
python3 scripts/validate_odin.py --native-metal
python3 scripts/validate_odin.py --native-vulkan
```

An explicit loader and ICD can be selected without hardcoding host paths in the
script, for example on the currently validated macOS installation:

```sh
python3 scripts/validate_odin.py --native-metal --native-vulkan \
  --vulkan-library /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json
```

Vulkan acceptance needs `glslc`, a Vulkan loader, an actual device and the Khronos
validation layer. It enables synchronization validation and fails if validation
cannot initialize or produces error messages. Native Metal requires macOS arm64,
sets `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launch, and fails on
missing/unsupported hardware. Both native runs have bounded execution. CPU tests
never count as native acceptance.

On the Apple M5 host with macOS 27, Odin dev-2026-09:a2fb372b7 and the current
Xcode SDK, both the Metal 4 adapter and Vulkan through MoltenVK 1.4.1 execute this
scenario with native validation enabled and no validation errors. This establishes
compute/transfer output and tested buffer/pipeline lifetimes on this host. The new CPU suites also pass optimized and AddressSanitizer runs; both native
consumers pass AddressSanitizer with empty Odin allocation trackers at teardown.
The Vulkan consumer typechecks for Linux and Windows. This does
not establish a native Linux/Windows Vulkan run, images, draws, windowed
presentation, asynchronous readback, surface replacement or the full application.
