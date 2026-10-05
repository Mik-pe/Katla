# Metal backend

Katla uses the Metal 4 core API through objc2-metal on macOS. The required Apple Silicon CI platform is `macos-26`. Vulkan and Metal consume the same compiled render graph, including compute commands, imported image contracts, subresource synchronization, attachment operations, and transient allocation lifetimes.

`MetalContext` owns one native `MTL4CommandQueue`. Three frame slots each own an `MTL4CommandAllocator`. Acquisition waits only for the slot being reused, receives its exact terminal commit feedback, retires its upload staging, and resets its allocator. Encoding creates a `MTL4CommandBuffer` with that allocator and attaches native residency sets again after begin. Submission does not wait for the previous frame. See [frame ownership](metal4_frame_slots.md).

`MetalExecutionPlan` lowers declared passes into native render and compute encoders. Copy and fill operations use Metal 4 compute command encoder transfer operations. There is no separate native blit encoder or alternative command path. Application-owned scene services and custom WGSL kernels use the same prepared, reflected pipeline registry. The native core initializes no scene or editor pipelines. Graphics passes provide reflected buffers, images, samplers, constants and draw phases through ordinary pass packets. Shader compilation finishes before encoding; graph commands specify binding identities, byte ranges, constants, and direct or indirect workgroup counts.

Metal 4 resources are untracked. `sync.rs` lowers compiled producer and consumer scopes to explicit queue stage visibility barriers, including external texture uploads and terminal imported-image contracts. Alias handoffs additionally use `ResourceAlias` visibility. A tracked native heap descriptor never substitutes for those barriers. `transient_heap.rs` allocates real placement heaps; memoryless attachments require the compiled whole-lifetime eligibility proof.

`render_encoder.rs` and `compute_encoder.rs` populate native `MTL4ArgumentTable` objects from shader reflection. Runtime-array metadata uses the exact declared bound byte length. Missing reflected bindings produce a renderer error before commit. `argument_buffer.rs` publishes immutable bindless resource-ID snapshots, with cached transient frame variants. `residency.rs` retains physical allocations, heap owners, and stable persistent buffer sets. Every submission retains its tables, pipeline states, samplers, residency snapshots, attachment resources, and inline constants until its own completion. See [binding and residency](metal-binding-residency.md).

`pipeline_archive.rs` uses the Metal 4 compiler and native archives with content-addressed source, binding-schema, workgroup, and render-state identities. Material variants are prepared ahead of encoding. Background reloads publish complete replacements; an unsuccessful reload leaves the previous ready variants usable.

`texture_upload.rs` stages validated subresource uploads into private textures, supports arrays and 3D textures, and generates mip chains explicitly. Staging retires after the exact consuming commit feedback; aborted unsubmitted work returns to the pending queue. See [texture upload contracts](metal_texture_uploads.md).

The surface configures three drawables. Native presentation follows queue `waitForDrawable`, commit, queue `signalDrawable`, then drawable `present`. Headless frames use the same command and completion path. Screenshots wait for the latest submitted frame; explicit shutdown and rebuild operations drain outstanding submissions.

`diagnostics.rs` preserves the Metal 4 feedback error domain, code, and description. Native feedback supplies GPU start and end times; `MetalFrameMetrics` exposes CPU commit cost, slot wait, observed GPU duration, and CPU lead. Encoding remains thread-affine; callback blocks capture only synchronized completion data and owning resources retire on the encoding thread.

Validate native changes by launching with `METAL_DEVICE_WRAPPER_TYPE=1` before process startup. GPU output checks, multi-slot stress, aliasing, uploads, and terminal feedback tests complement portable graph tests.

GPU profiling uses Metal 4 counter heaps owned by the three frame slots. Open profiling labels measure the graph submission; results become readable after that submission's exact feedback, with native ticks converted using the Mach timebase. Profiling emits no commands unless labels are open. See [frame ownership](metal4_frame_slots.md).

Initialization requires the default device to support `MTLGPUFamilyMetal4`.
Unsupported devices return a typed capability error before creating a native
queue, compiler or allocator; there is no legacy command fallback. The hosted
macos-26 virtual GPU may lack this capability, as documented in [CI](ci.md).
