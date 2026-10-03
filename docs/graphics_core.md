# Graphics ownership and composition

`GpuRenderer` owns native resources, hardware capabilities, frame acquisition,
submission, retirement and readback. `RenderGraphBackend` supplies generic graph
allocation, synchronization and command execution. Neither interface requires
fonts, viewport selection, lights, shadows, particle simulation, picking policy,
outlines or postprocessing.

`AnyRenderer` selects the implementation at construction. Application code uses
ordinary buffer, texture, mesh and material handles thereafter. Viewport layout
belongs to the app; render targets use graph declarations or ordinary texture
handles. Pipeline state uses `PipelineDescriptor` and `ImageFormat`, without
separate viewport builders or feature-specific pipeline initialization enums.
Failed initial material compilation publishes no material handle. Vulkan also
releases unpublished pipeline variants and newly created descriptor layouts;
Metal retains temporary pipeline objects locally until successful publication.
Materials keep independent handles, texture sets and reload dependencies. Native
graphics pipelines share immutable state when their resolved descriptors and
expanded shader contents match. File paths do not affect native reuse; dependency
tracking still uses each material's canonical source paths. Weak cache entries do
not retain native pipelines after material and submission owners release them.
Graphics passes carry `PassBindings`: reflected buffers, images, samplers and immutable inline
bytes, plus explicit drawing phases. A phase selects mesh-layout pipelines,
submitted objects, generated vertices or a declared indirect buffer. Pipeline
descriptors specify depth, stencil, blending, color writes and depth bias.
Sampler descriptors define filtering, mip selection, wrapping, comparison and
anisotropy independently of image uploads. Both backends cache native samplers by
descriptor value. Drawing phases can override sampler and constant slots; each
phase starts from the pass's base packet. Explicit image bindings can select an
imported image's mip range, whose first included level becomes shader level zero.

Vertex layout fields carry their shader location and storage format together.
Storage/buffer order is independent of shader location: static PBR uses locations
`0,1,2,3,6`, while skinned PBR retains joints/weights at `4,5` and UV1 at `6`.
`VertexPBR` occupies 56 bytes and `VertexPBRSkinned` 80 bytes. Their convenience
constructors initialize UV1 from UV0; glTF decoding preserves independent UV1
accessors and validates counts/finite values. SoA uploads bind fields in descriptor
order instead of selecting buffers from a skeleton handle. Descriptor validation
rejects duplicate locations and locations beyond 30 before native creation,
matching Metal's [31 vertex attribute entries](https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf).

## Minimal application

An empty application graph selects `GraphOnly` and installs no scene or editor
services:

```rust
use katla_app::prelude::*;

let builder = ApplicationBuilder::new().with_frame_graph(|renderer, _resources| {
    Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer)))
});
```

A custom graph can declare a clear-only target, a shader-generated triangle or
compute commands without preparing Katla's editor preset. Graphics passes
without depth use `.without_depth()`; passes that use depth name a graph-owned
depth attachment explicitly. A void fragment entry is supported for alpha-tested
depth rendering and does not imply a color attachment.

The executable custom-shader example in
[`backend_neutral_api.rs`](../katla_gfx/tests/backend_neutral_api.rs) creates a
material, supplies inline tint bytes, renders generated vertices and checks
the exported pixels through the core readback API. It initializes no scene
subsystem:

```bash
cargo test -p katla_gfx --test backend_neutral_api -- --ignored
```

The test requires Vulkan. The shared native compute and contract fixtures select
the platform backend; they exercise the same ordinary resource and binding
contracts on Metal and Vulkan.

## Editor composition

The editor is an explicit application preset:

```rust
use katla_app::prelude::*;

let builder = ApplicationBuilder::new().with_frame_graph(|renderer, resources| {
    KatlaEditorFrameGraphPreset::build(renderer, resources)
});
```

The builder resolves the graph runtime before loading fonts or initializing
feature resources. `SceneFeatures` owns semantic default textures, animation
uploads, light buffers, particle pools, shader descriptors and scene pass
composition. Its graphics component supplies shadow cascade phases, depth and
object-ID pipelines, selection phases, sky and postprocessing packets.
`EditorFeatures` owns an ordinary atlas texture and its sampled slot, plus
pending picking tickets and their source-frame entity maps. Viewport sizing
belongs to the application. HDR, depth, object-ID, shadow and viewport images
belong to the graph.

Scene model pipelines use `R16G16B16A16Sfloat`, matching the graph's HDR target.
Application `MaterialSurface` contains emissive RGB, normal scale, occlusion
strength and coverage policy. `FrameContext` collects typed values alongside assigned object slots;
the scene geometry packet binds their immutable bytes at group 0, binding 2. This
layout belongs to application shaders, while the core continues to own generic
objects, material texture handles and submission retention.
Shadow shaders and app packets bind skeletal joints in group 2 and cascade data
in group 3. Sky and postprocessing draws generate vertices from vertex index
and declare an empty vertex layout.

Built-ins install normal WGSL compute descriptors and direct or indirect
dispatches. The graph contains no named built-in buffer roles or hidden workload
dispatch policy. Backend constructors do not install these services.

`initialize_compute_pipelines` prepares only live graph commands. Features with
future workloads, such as animation before the first skeleton is loaded, call
`prepare_compute_pipeline` explicitly with the same descriptor they will submit.
Neither preparation operation inserts work into a frame. Encoding requires a
prepared pipeline and reports a missing one without compiling on the frame path.

Pass handles are opaque identities owned by one graph. Inserting a pass changes
execution order without redirecting saved handles. Mutation rejects foreign
handles, and frame submission validates them before native encoding. Pass
insertion returns errors for invalid positions and duplicate or empty names
without changing the graph or its compiled plan.

## Frame and resource ownership

Acquire a `FrameToken`, select the application's resources for that reusable
slot, prepare bindings and draw storage, execute the graph, then consume the
token with `present` or `abort`. A CPU-visible buffer handle denotes one actual
allocation. Applications create separate mutable buffers for each in-flight
slot and rebind the graph's stable resource ID to the selected handle. Native
writers validate the token, bounds and pending GPU ownership before touching
memory. Immutable initialized resources use `create_buffer_with_data`.

Acquisition tokens have process-unique identities, so a token from another
renderer cannot address a matching slot. `present` returns an outer error only
when no GPU submission was accepted. Every `PresentOutcome` means the submission
was committed: applications advance their associated CPU state before inspecting
its surface result. That result distinguishes successful presentation, required
surface recreation and a fatal error after submission.

Per-pass packets own inline bytes. Replacing graphics inputs validates the
existing access contract before mutation and retains the compiled graph plan.
Invalid replacements preserve the previous packet. Compute dispatch dimensions
and constants belong to authored commands, with no callback override or unused
uniform-byte channel. Frame inputs validate against the pass type: compute and
transfer passes reject draw lists, and UI passes receive one composed UI list.
The same validation runs before native encoding on both backends. Native
encoders retain resolved resources, pipeline variants, descriptor tables and
residency until the exact submission
retires. Rebinding a graph resource for a later frame does not rewrite an
earlier frame's descriptors.

`graph_texture_source` identifies an exact committed exported image, including
resource, frame slot, generation and submission. `queue_texture_readback`
retains that source until its typed ticket completes. `poll_texture_readback`
does not wait. Aborted frames do not publish exports; resize and slot reuse do
not replace a queued ticket's source. The graphics core returns pixels; the
application maps object IDs to entities. Completed buffer readback similarly
returns owned bytes only after the relevant GPU work has retired.

Device drain waits pending image readback copies as well as frame submissions.
Vulkan latches completed buffer owners before a reusable native fence is reset;
an unrelated later submission cannot make an already completed result pending.
Editor clicks queue against their source-frame snapshot immediately. Older
completed tickets are consumed, while only the latest click can change selection.

See [the graph API](../katla_gfx/src/render_graph/API.md) for resource/access
declarations and [capture diagnostics](render_graph_capture.md) for passive
inspection of compiled contracts and actual native emissions.

## Acquisition and prepared draws

Acquiring a busy slot waits for its exact prior submission. Unavailable surfaces
produce no token; OutOfDate requires recreation. Only accepted presentation
advances the slot. Reacquiring an open frame abandons it on the same slot with a
new identity. FrameToken is Copy: dropping a copy has no lifecycle meaning.
A failed render poisons the frame; present rejects partial work until abort.

Draw submissions use one Rc<DrawList> per prepared list and borrow it across
passes. Preserve global object slots and submit order; do not rebuild merged
lists. Metal uploads each unique list once per frame while initializing every
referenced slot. Bindless descriptor slots are GPU addressing, not CPU resource
identity: resource handles always validate index and generation.

Vulkan headless rendering owns two offscreen targets. Windowed resize uses
physical pixel dimensions clamped to surface limits and replaces synchronization
objects with the swapchain. Release the surface before native window teardown.
Output recreation preserves the next reusable frame slot and monotonic submitted
frame count, so app-owned frame resources continue from their committed source.
Frame fences reset immediately before submission. Each slot waits only for a
successfully accepted submission, so rejection leaves an unsubmitted fence
reusable without allocating a replacement. Windowed aborts and rejected
submissions retain the acquired surface image and its semaphore until a
successful submission consumes them. Scene attachment extents remain
independent of output extents. Depth-only passes derive render area, viewport
and scissor from their explicit depth target. Vulkan retains the combined
depth/stencil attachment view and owns a separate depth-only view for sampled
bindless descriptors, including replacement after resize. Core object storage
begins at byte zero; Vulkan
timestamp profiling is unsupported. Native Metal profiling follows its
[frame-slot contract](metal4_frame_slots.md).

## Vulkan device and submission contract

Device selection checks Vulkan 1.3, dynamic rendering, synchronization2,
maintenance4, buffer addresses, anisotropy and the requested bindless descriptor
features before ranking suitable GPUs. The required-feature declarations also
build the logical-device request. Headless devices require push descriptors;
windowed devices additionally require swapchain support and a presenting
combined graphics/compute queue. A dedicated transfer-only queue does not need
presentation support. Maintenance4 is enabled as a core feature.

All Vulkan queue submissions use one fallible synchronization2 entry point.
Each semaphore wait carries its execution stage; binary signals cover all
commands. Small submissions keep their command and semaphore descriptors inline.
Mesh uploads batch copy-to-vertex/index barriers in one dependency operation.
Rejected submissions release their unsubmitted command buffers, fences and
staging allocations. Submitted one-time command buffers and optional staging
allocations remain owned until fence completion or an idle device drain, even
if a CPU wait fails. Asset image uploads also submit on the graphics queue
without a CPU wait. Their barriers cover the allocated mip chain; filtered blits
check native format support before allocation. Full base updates regenerate
authored mips. Image views span all levels. If an image owner disappears before
upload completion, its allocation moves to the last referencing upload fence;
native retirement contains no context ownership cycle. A successful device-wide
idle wait drains these uploads. Unsampled asset images receive no bindless slot
and arrive in GENERAL layout, while sampled images arrive shader-read-only.

Vulkan resets a retired slot's command buffer before recycling its descriptor
and upload storage. This also discards partial recording and dynamic
rendering state from a rejected frame. Graphics render areas and color pipeline
variants resolve from the declared native attachments, including depth-only
passes and targets smaller than the output. Incompatible attachment extents
return a typed error before beginning rendering.

Graphics descriptor sets allocate from reusable pools owned by each frame slot.
Pool budgets account for both sets and each reflected descriptor type, growing
only when existing capacity is exhausted. Slot acquisition resets used pools
after its exact fence completes and its command buffer resets; resize reuses
the pools after device retirement. Individual sets are never freed or rewritten
while an earlier submission can use them. Pools retain their native device and
release together at renderer teardown. Native allocation failures propagate
without consuming the tracked descriptor budget; retrying an exhausted fresh
pool returns an error instead of growing indefinitely.

One native frame-resource owner retains descriptor pools, upload blocks and
sampled image views. Immutable inline constants and UI uniforms, instances,
vertices and indices use aligned, disjoint ranges in reusable CPU-visible
blocks. Growing storage preserves earlier ranges, and slot retirement resets
offsets without reallocating the blocks. Writes flush through the ordinary
graph-buffer path. Every UI pass allocates fresh descriptors and data ranges;
later passes cannot overwrite earlier geometry or screen-size uniforms. UI
draw commands upload each geometry source once per pass and bind its offsets.
There is no separate UI descriptor cache or scratch-resource manager.

Graphics encoding reuses one resource packet across a pass's drawing phases.
Each phase starts with the pass constants before applying its own overrides;
encoding never clones the complete phase list for individual draws. The native
allocation-growth regression measures actual rendering after both slots warm
up and requires approximately linear growth as phase counts increase.

Explicit pass bindings take precedence over implicit draw bindings on both
backends; duplicate explicit slots remain invalid. Skinned Vulkan draws resolve
the shader's skeleton storage slot at group 2 or 3. Sampled depth/stencil
transients retain a separate depth-only view, shared across texture clones;
the combined attachment view is never installed in sampled descriptors.

Native Vulkan command buffers own their allocations and release them once on
drop. The allocation retains its command pool, which retains its device,
instance and loader. Frame slots share an allocation through explicit Rc owners;
command-buffer wrappers themselves cannot be cloned. A retained unsubmitted
buffer may outlive its context without invalidating those native parents.
Queued work retains its owners until completion, and foreign-device commands
are rejected before recording completion or queue submission.

Frame command buffers are allocated in one batch for the in-flight slot count,
independent of the number of swapchain images. Allocation failures propagate as
typed errors. Windowed resize reuses the completed frame allocations; headless
replacement and teardown drop their old owners. There is no manual pool-return
API or unused transfer command pool. Device alignment limits are cached at
initialization rather than queried for each graphics binding.

Instance ownership also covers the presentation surface, validation messenger
and callback storage. Startup failures release initialized parents, and the
callback pointer borrows retained storage instead of leaking a raw Arc owner.
The messenger is destroyed before its storage and instance are released.

Output creation allocates command buffers before native targets. Image-view
failures release completed views and allocations; swapchains retain their
native device and destroy themselves on rollback. Dropping a frame context
waits for device retirement before releasing its commands and targets.

Scene surface raster variants belong to the application. The core exposes live
material descriptors and texture bindings as read-only resource metadata; stale
handles return `None`. App-owned variants preserve texture bindings, share the
native compilation cache, and retire when their source material is removed.
`DrawList::sort_for_view` groups opaque draws first, then orders transparent draws
by unquantized view depth while preserving slots and equal-depth submission order.
The frame builder separates transparent instances and opposite transform
handedness into independent draws. MASK coverage shares a fragment helper across
color, depth, shadow and picking. The editor preset owns a separate `picking_depth`
attachment so selecting translucent surfaces does not modify scene depth.
