# Odin graphics core

`odin/gfx` owns generic resource identities, explicit frame acquisitions,
compiled graphs and authored compute/transfer/graphics packets. Native adapters
in `odin/gfx/metal` and `odin/gfx/vulkan` import no ECS, math, editor or scene
packages. Application composition and material authoring live in `odin/app`.
The remaining migration is tracked in [TODO](../TODO.md#odin-port).

## Ownership

`Resource_Storage(T, Kind)` transfers values on insertion/removal. Handles carry
stationary owner, slot and generation; foreign, removed and stale handles fail
lookup. Generation exhaustion permanently retires a slot. Remove native values
before destroying empty storage. Native submissions retain resolved allocations,
pipelines, descriptor/argument tables and residency independently of public
handles. Removing a handle prevents new lookup while accepted work keeps its
parents alive until exact retirement.

`acquire` returns the actual `Frame_Token` before selecting mutable slot resources.
`write_buffer` requires that acquired token and rejects pending GPU allocations.
Initialized immutable allocations use `create_buffer_with_data`. Submission
consumes the token only after the queue accepts work; preflight rejection leaves
it acquired for repair or `abort`. Recording failures discard its native partial
work. A submitted acquisition cannot abort. The three-slot ring returns `Busy`
until the caller explicitly waits or polls an accepted submission; acquisition
does not introduce an implicit CPU wait.

```
Idle -> Acquired -> Recorded -> Submitted -> Idle
          |           |
          +-- abort --+
```

Completion matches the exact token and submission number. Double retirement,
foreign owners and old acquisition generations fail. Drain accepted native work
and readback copies before destroying stationary owners.

## Declarations and compilation

One `Graph` declares buffers and images. There is no separate image or scene
execution graph. Resource/pass IDs remain stable while declarations are
appended. Buffer accesses name actual byte intervals, mode and usage. Image
accesses name actual mip/layer/aspect intervals and usage. Imported images name
arrival/final states and initialized-content promises separately. Undefined
contents cannot be loaded; presentation requires an explicit final present state.

Compilation validates initialization even for dead declarations, then traces
backward demands from exports and side effects. A later write replaces only its
covered byte, mip, layer and aspect ranges. Fully overwritten producers are
culled; surviving partial producers remain live. Live read/write, write/read and
write/write overlaps produce hazards. Read/read, disjoint ranges and query-only
bindings produce no contents hazard. Query-only bindings still retain resource
identity and native ownership. Exports require complete initialized coverage.

Plans borrow their stationary graph and revision. Declaration changes and attachment content-policy changes invalidate
old plans. `graph_set_packet` validates a replacement against the existing
contract before cloning it. `graph_set_commands` validates commands and new
buffer/image accesses together, commits atomically and increments the revision.
It supports a real transition to a clear-only empty scene without inventing a
mesh or dummy draw.

Shader code, dynamic indexing and imported initialization remain application
contracts. Reflection supplies selected-entry reads/writes, binding classes and
minimum spans, but cannot prove every dynamic address or full write coverage.

## Packets and preflight

`Dispatch` carries explicit workgroups or an exact twelve-byte GPU indirect
command, and logical group/binding resource slots. `Fill_Buffer`, `Copy_Buffer`,
`Copy_Image_Buffer` and `Copy_Buffer_Image` carry ordinary declared transfer
accesses. Fill intervals are four-byte aligned. Buffer/image transfers preserve
explicit row/slice pitches, volume coordinates, compressed block edge rules and
the actual selected aspect width. Zero pitches select tightly packed rows; padded
image-to-buffer copies declare only useful rows as written, leaving padding
uninitialized. `Generate_Mips` reads the selected base mip and initializes the
remaining chain with native filtered transfers; native format capabilities are
required. Partial image uploads use `Read_Write` for their mip/layer
so untouched texels require prior initialization; a full subresource upload can
initialize with `Write`.

Texture descriptors specify positive width, height and depth. Volumes require
one array layer; two-dimensional resources use depth one. Subresource ownership
covers full mip volumes. The formats include sRGB/unorm color, single/two-channel
color, float/integer color, depth/stencil and BC1/BC3 blocks. Compression forbids
volume, storage and attachment roles. Native allocation queries reject unsupported
physical format/usage combinations before publication; supported format names do
not imply hardware support on every device.

A `Render` carries explicit color/depth attachments and shared buffer, image,
sampler and constant bindings. Ordered `Render_Phase` values select pipelines,
constant overrides, viewport/scissor and generated, vertex, indexed or indirect
geometry. Native render areas come from the declared attachments, including
smaller targets and depth-only passes. Attachment extents must agree. Clear
initializes the whole selected subresource; load requires initialized content.
Load/store discard invalidates the selected subresources. The latest write
determines content validity, so an earlier clear cannot satisfy a read or export
after discard. Raster coverage after a discarded load cannot promise full
initialization.

Vertex layouts name shader locations, actual buffer bindings, strides and vertex
or instance stepping. Direct bounds and index/indirect alignment are validated;
indirect command counts and strides remain explicit. GPU-authored index and
indirect values remain caller contracts. Native pipelines advertise supported
draw forms so missing encoding support cannot become successful empty work.

Prepared phases own the effective constants after shared defaults and phase
overrides merge. Their native bytes are immutable for that exact submission.
Shared bindings may be used by different phases; their aggregate actual shader
visibility must match the declared stages. Preparation checks actual native
capacities/usages, reflected binding class/mode/minimum span, alignment, descriptor
limits, texture shape/format, sampler comparison class and dispatch limits. Missing
queries, resources, packets, layouts and unsupported shapes fail before encoding.
Failed preparation owns no partially published packet.

Prepared graphs clone packets, inputs, contracts and hazards. Later packet
replacement does not rewrite an earlier snapshot. Native adapters resolve handles
again before retention and retain each resolved owner until accepted work retires.

## Shader contracts

Prepared shader descriptors carry SPIR-V and MSL targets and independent native
entry names. Naga may translate a valid WGSL entry name to a different MSL symbol;
no target-name fallback is permitted. Logical group/binding identities map to
explicit per-stage Metal indices. Runtime-array sizes use the compiler's reserved
native index and owned immutable size bytes for each submission.

Compute and graphics descriptors include buffers, textures and independent
samplers. Selected-entry access modes and minimum byte spans accompany the
canonical shader metadata; preflight cannot downgrade a shader write to an
authored read. Query-only access is explicit. Graphics state includes color
writes/blending, raster topology/culling, depth testing and writing, stencil,
depth bias and wireframe. Required native operations have explicit implementations
or return a typed failure; they have no default no-op path.

## Physical aliases

Physical identity is distinct from public handle identity. Two graph roles may
share an actual allocation only when their compiled live intervals do not overlap.
A successor cannot promise imported initialized contents; it must initialize its
own data. An export extends its lifetime through the graph end. Preparation emits
`Alias_Handoff` values from the previous role's last pass to the next role's first
pass. Native encoders consume those handoffs as actual memory/queue barriers.
`graph_allocation_plan` groups live transient buffers/images by disjoint compiled
lifetimes, native alignment, memory domain and intersecting memory types.
`graph_allocate` instantiates each group with real native placement storage and
captures every handle for rollback and cleanup. Independent instances provide
actual per-slot storage. Imports remain caller-owned. Public allocation removal
preserves native parents retained by pending submissions. CPU-visible and
GPU-private buffer domains are explicit; private CPU access is rejected, while
initialized private construction uses an owned staging submission.

## Sources, readback and presentation

`graph_texture_source(renderer, submission, resource)` identifies an exact
accepted exported image. An aborted frame publishes no source. A lightweight
source can be queued while its exact export is retained. Its generation captures
the physical allocation content epoch: an accepted write through another graph
or an aliased buffer invalidates an older unqueued source too. Rejected writes
do not advance that epoch. `release_graph_exports` removes one graph's retained
exports before its stationary CPU owner is destroyed. `queue_texture_readback` immediately retains
the selected native source and its independent copy submission. Later graph
replacement, slot reuse, resize or public-handle removal cannot change that queued
copy's source. `poll_texture_readback` never waits and transfers owned bytes once.
`Readback_Data` includes source, region, row/slice pitches and captures its allocator;
release it with `readback_data_destroy`. Cancel/destroy joins accepted copy work
before releasing native parents.

Surface acquisition returns a generation-bound image. Presentation associates it
with the accepted rendering submission. `Present_Outcome` preserves that committed
submission even if presentation requests recreation or fails afterward. Native
surface images, semaphores/drawables and readback copies remain owned through
acceptance, failed presentation and recreation. Application window policy belongs
outside the GPU core.

## Native acceptance

`odin/gfx_conformance` supplies required native function inputs to both adapters.
The compute scenario executes 12,288 integers through three concurrently live
fill/parameter/copy graphs and checks immutable bindings, CPU access exclusion,
ring exhaustion and exact out-of-order retirement. The graphics scenario checks
512 RGBA8 pixels and 512 D32 values through real rasterization and image-to-buffer
copies. A queued image ticket must preserve its earlier pixels across four later
submissions, frame-slot reuse and public resource destruction. Corrupted/stale physical sources and repeated ticket completion fail. The shared
allocation scenario verifies three independent mixed buffer/image placement
groups, 6,144 compute values and 9,216 image pixels, including public allocation
owners removed before retirement and readback completion.

Both Metal 4 and Vulkan through MoltenVK execute these shared scenarios on the
Apple Silicon development host with native validation. Backend-specific scenarios
also exercise mesh/indirect phases, physical aliases and surface replacement.
New contracts require rerunning the affected native scenarios before delivery;
CPU tests and compilation alone do not establish GPU behavior. Linux/Windows
Vulkan typechecks are separate from native hardware evidence.

```sh
python3 scripts/validate_odin.py
python3 scripts/validate_odin.py --native-metal --native-vulkan \
  --vulkan-library /usr/local/lib/libvulkan.dylib \
  --vulkan-icd /usr/local/share/vulkan/icd.d/MoltenVK_icd.json
```

Native Metal sets `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launching.
Native Vulkan enables Khronos synchronization validation and requires a real device,
loader and validation layer. Failure to initialize validation is not a passing
run. Native tests have bounded execution and release Odin/native owned state at
teardown. Full application migration and removal of superseded Rust consumers
remain separate completion requirements.
