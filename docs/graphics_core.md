# Graphics ownership and composition

The canonical graphics implementation is [odin/gfx](../odin/gfx). Its native
adapters are [Metal](../odin/gfx/metal) and [Vulkan](../odin/gfx/vulkan). They own
resource identities, physical allocations, frame acquisition, compiled execution,
submission, retirement, surfaces and retained readback. They import no ECS,
editor, UI, math or scene package.

Application composition belongs in [odin/app/render](../odin/app/render).
The editor's [GPU owner](../odin/app/editor/gpu_owner.odin) composes its cameras,
models, lights, shadows, particles, overlays, picking and retained UI on one
shared graph and one accepted submission. Window, camera, selection and document
policy stay in the application. A renderer constructor installs no scene preset.

For the complete generic API and native requirements, use
[Odin graphics contracts](gfx_odin.md). For material/compiler ownership, use
[shader compilation and replacement](gfx_shader_odin.md). Application details
are in [render features](render_features_odin.md), [models](gltf_odin.md),
[particles](particles_odin.md), [editor overlays](editor_overlays_odin.md) and
[UI rendering](odin-ui-rendering.md).

## Frames and native owners

`acquire` returns the exact `Frame_Token` before the app writes mutable resources.
A CPU-visible buffer is one actual allocation; the app creates a separate buffer
for each reusable frame slot. `write_buffer` validates the acquisition, bounds
and pending GPU ownership. Initialized immutable buffers use
`create_buffer_with_data`; GPU-private buffers use staging and reject direct CPU
access.

`submit` consumes a token only after native queue acceptance. Preflight rejection
leaves it acquired for repair or `abort`; recording failure discards partial
native work. An accepted acquisition cannot abort. Busy slots require explicit
retirement. Completion matches the exact submission and token, including owner
and generation.

Removing a public handle prevents new lookup. Accepted native work independently
retains its allocations, pipelines, immutable descriptor tables and native
parents until completion. Drain accepted submissions and readback copies before
tearing down stationary owners. An app must publish its associated CPU state
once submission is accepted, including when later presentation fails.

## One graph and explicit packets

`Graph` declares buffers and images using stable owner-bound IDs. Accesses name
actual byte intervals or mip/layer/aspect ranges, modes and usages. Imported
images declare arrival/final state separately from initialized content. Compilation
validates accesses and initialization, culls overwritten producers and retains
live hazards. Physical aliasing requires disjoint compiled lifetimes and explicit
native handoffs.

A `Render` names color/depth attachments and shared resource bindings. Ordered
`Render_Phase` packets select pipelines, constants, viewport/scissor and direct,
indexed or indirect draws. Compute and transfer work use ordinary `Dispatch`,
fill, copy and mip-generation packets. There are no hidden feature workloads.
Pipeline state specifies blending, color writes, raster, depth and stencil
behavior. Native capabilities reject unsupported operations before publication.

`graph_set_packet` replaces commands within an existing access contract.
`graph_set_commands` validates commands and replacement accesses together.
`graph_replace_image` retains an imported image's logical identity while a
transaction replaces its descriptor and affected commands. Compile the complete
candidate before recording; failed app staging restores previous declarations,
packets and revision. Prepared recordings already own immutable descriptors,
inputs, packets and hazards.

WGSL is canonical. The isolated offline compiler and cache produce owned native
artifacts and selected-entry reflection; native encoding never compiles shaders.
Reflection supplies actual binding modes, minimum spans and target-specific
indices. Preflight validates real native resources against that interface.
Dynamic shader addresses and full write coverage remain authored contracts.

## Exports, capture and surfaces

`graph_texture_source` names an exact accepted exported image and its physical
content epoch. Rejected or aborted work publishes no source. An accepted write
through another graph or physical alias invalidates an older unqueued source.
`release_graph_exports` releases one graph's retained exports before that CPU
owner disappears.

`queue_texture_readback` immediately retains the native source and an independent
copy submission. Later slot reuse, resize, replacement and public-handle removal
cannot change that queued copy. `poll_texture_readback` transfers owned bytes
once without waiting; release them with `readback_data_destroy`. Picking maps
pixel IDs through the app's source-frame entity snapshot.

Surface acquisition identifies a generation-bound image. Presentation associates
it with the accepted rendering submission. `Present_Outcome` preserves acceptance
when recreation or failure occurs afterward. Native surface objects and their
synchronization remain owned through retirement; window policy belongs to the app.

## Validation

Use [the cross-backend contract suite](contract-suite.md) for required real-device
checks and [the canonical build](odin_build.md) for dependency setup. Compilation,
CPU tests and screenshots alone do not prove GPU behavior. Metal validation must
be enabled before launch; Vulkan acceptance requires working Khronos validation.
Report unavailable hardware as blocked, and keep typechecks separate from native
execution evidence.
