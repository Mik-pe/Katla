# Odin Metal backend

`odin/gfx/metal` implements the generic GPU contract directly against Metal 4 on macOS Apple Silicon. It uses Odin's Darwin bindings and explicit Objective-C selectors for Metal 4 APIs absent from those bindings. The application owns scene, model, particle, UI, overlay and picking pipelines. The GPU core owns resources, frame slots, prepared execution and completion; it imports no editor or ECS policy. Both backends consume the same `gfx.Graph`, `Compiled_Graph` and immutable prepared packets. See [graphics ownership](graphics_core.md).

## Capabilities and handles

`renderer_init` requires a default device supporting `MTLGPUFamilyMetal4` before creating its compiler, queue or command allocators. No device returns `No_Device`; a device lacking Metal 4 returns `Unsupported`. There is no alternate command implementation. Metal CI runs on `macos-26`; its label and SDK do not prove that the hosted virtual GPU supports Metal 4.

Fixed texture binding arrays require argument-buffer Tier 2. Selected WGSL reflection owns their exact count, including one-element arrays. Scalar native textures and native argument-buffer pointers have explicit separate metadata. Metal reflection validates the pointer's texture element type, access and eight-byte resource-ID ABI; it does not expose a fixed shader array count. Immutable tables contain exactly the declared IDs and retain selected views and allocations through completion. Buffer and sampler binding arrays are rejected by the current shader adapter.

Supported image formats and D2/D3 shapes are explicit generic descriptors. BC formats require actual device compression support. D24 depth/stencil is unsupported on this Apple backend; D32 depth/stencil remains a distinct format with four-byte depth and one-byte stencil transfers. Unsupported features return typed errors without substituting a format.

Handles carry owner and generation. Removing one immediately invalidates future CPU lookup; accepted commands keep native objects alive. Foreign or stale handles/tokens return `Invalid_Resource`. Invalid bytes, subresources or state return `Invalid_Range`, `Invalid_Graph` or `Invalid_Shader` as appropriate. CPU access to private buffers returns `Unsupported`; CPU access to pending allocations or aliasing heaps returns `Busy`.

## Frames, recording and completion

Keep the renderer stationary on its owner thread. Three native slots each own an `MTL4CommandAllocator`. `acquire` explicitly obtains the next idle token and returns `Busy` until that slot retires. It neither waits for another frame nor implicitly acquires during submission. The caller chooses `poll` or `wait` for an exact accepted submission. Reuse requires terminal feedback and releases that submission's owners before resetting its allocator.

Mutable `write_buffer` requires the exact acquired token and CPU ownership. Immutable `create_buffer_with_data` creates a fresh owner; private buffers use actual GPU staging. `abort` abandons an unsubmitted token. Preflight rejection preserves the acquisition for retry or abort. Recording failure releases unpublished native owners and aborts its token. Neither rejection path publishes exports or initialized-content state.

`submit` preflights the complete graph, owns its reflected bindings and packet bytes, records one Metal 4 command buffer and commits it to one queue. Graphics packets contain ordered phases with pipelines, immutable constants, viewport/scissor and generated, vertex, indexed or indirect draws. Compute supports direct and GPU-produced indirect dispatch. Transfers, fill and mip generation use Metal 4 compute-encoder transfer commands. Resources use untracked hazards; explicit queue-stage visibility and alias visibility barriers order declared producers and consumers.

Each submission retains buffers, textures, placement heaps, samplers, pipeline states, argument tables, inline uploads and its residency set until its own feedback. Allocation groups use actual placement heaps and compiler-proven disjoint lifetimes. Buffer/image alias handoffs invalidate prior contents; exported resources extend liveness and cannot be reused prematurely.

The initialized-content journal commits only after native queue acceptance. Clear/full writes initialize subresources; partial writes preserve previously initialized regions. Discard invalidates contents. Failed recording cannot falsely initialize textures. Accepted writes, uploads and physical alias writes advance the allocation's content epoch.

Terminal feedback copies native error code and description into synchronized completion state. Retirement logs GPU failure and makes subsequent work return `Native_Failure`. Callbacks do not mutate editor state or retire owner-thread resources. Shutdown joins accepted frame, upload and readback work before releasing native parents.

## Uploads, exports and presentation

Texture uploads validate dimensions, aspect, mip/layer, block alignment, row pitch and image pitch. Fresh immutable uploads use independently retained staging and queue completion. Explicit graph copies and mip generation preserve the generic layout contracts, including 3D slices.

`graph_texture_source` names an exact accepted submission and exported image. Its content epoch must still match when queuing readback; overwrites through another graph or alias invalidate that old source. Once queued, the copy independently retains its source and returns owned CPU bytes once, with exact source, region, row pitch and image pitch. Resize, slot reuse, handle removal and `release_graph_exports` cannot replace a queued ticket. Double polling, foreign tickets and stale sources are rejected. Paired color/ID capture and generational entity maps belong to the app; see [UI and committed picking](odin-ui-rendering.md).

The application supplies a main-thread NSView and physical extent. `attach_surface` creates a three-drawable BGRA8 UNORM layer. Acquire a GPU token before `acquire_surface`; each drawable has a distinct `Surface_Frame` generation. The graph consumes that exact image. The queue waits for its drawable before commit; `present_surface` signals and presents only the accepted submission that consumed it. `Present_Outcome` preserves queue acceptance independently of presentation success. Abort an unsubmitted drawable explicitly and detach before destroying its view. Zero-sized surfaces are unavailable; resizing an acquired drawable returns `Busy`.

## Depth, color and shader changes

Matrices remain column-major across backends. Forward depth uses the application's projection, Less/LessEqual and clear 1; reverse depth uses its reverse projection, Greater/GreaterEqual and clear 0. The backend maps comparisons and clears without transposing matrices or inventing camera policy. Independent depth-test/write flags, stencil, depth bias, clipping and wireframe have explicit native state.

Scene lighting and particles compose in linear RGBA16F before one tone/transfer stage. UI decodes encoded viewport/theme values, blends in its own linear RGBA16F target and applies one transfer-only sRGB encode into surface UNORM. It does not tone-map the scene again. Shader reload prepares complete native families and owned future descriptor snapshots before publication; failed candidates retain accepted pipelines. See [shader reload](odin-shader-reload.md).

Validate actual output and lifecycle with `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` set before launch. The [native validation workflow](metal4_validation.md) defines commands and hardware/sanitizer boundaries. Compilation or screenshots alone are not native GPU acceptance.
