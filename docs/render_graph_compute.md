# Compute commands and graph buffers

Compute and transfer work uses `PassDesc.commands` on both Vulkan and Metal. A
`ComputeDispatch` selects a `ComputeKernel::Shader(ComputePipelineDesc)` or a
built-in kernel, declares its buffer bindings and byte ranges, and supplies
explicit direct workgroups, a declared indirect buffer, or the built-in scene
workload. Native pipeline objects and command callbacks are absent from pass
metadata. Both adapters compile the exact WGSL source and entry point recorded
by the descriptor.

`ComputePass` builds buffer accesses from WGSL reflection and resolves named graph
buffers. For example, a compute shader can be declared with:

```rust,ignore
let shader = ComputePipelineDesc {
    wgsl: "@group(0) @binding(0) var<storage, read_write> values: array<u32>; \
           @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3u) { \
               values[id.x] += 1u; }".into(),
    entry: "main".into(),
};
let graph = FrameGraphBuilder::new()
    .create_buffer(GraphBufferDesc::new("values", BufferDesc::new(
        256, BufferUsages::STORAGE, BufferMemoryPolicy::DeviceLocal,
    )))
    .export_resource("values")
    .add_pass(ComputePass::new("increment", shader)
        .bind_buffer(0, 0, "values", BufferByteRange::new(0, 256))
        .dispatch([1, 1, 1]))
    .build()?;
```

Initialize contents with a transfer pass before using a read/write shader.
`dispatch_indirect(name, offset)` declares the twelve-byte command read.
`constants(bytes)` writes the reflected uniform binding and requires a buffer
with `UNIFORM | TRANSFER_DESTINATION` capabilities. Inline data must contain a
multiple of four bytes, no more than 65536 bytes, and fit the declared range.

Compilation rejects missing, duplicated, or unknown binding slots, a mismatch
with the shader's read/write declarations, unsupported non-buffer bindings,
commands outside the pass's declared ranges, and unaligned indirect dispatches.
The Vulkan adapter flattens sorted WGSL group/binding pairs into one reflected
push-descriptor set. Metal derives its native buffer indices and runtime-array
bounds from the same source. User byte ranges remain authoritative when native
buffers contain larger allocations. Reflection also rejects ranges smaller than
the WGSL buffer type. Particle storage arrays use their runtime byte ranges and
configured pool capacity, allowing small pools without fixed million-element
allocations.

Call `initialize_compute_pipelines` during graph initialization, before acquiring
a frame. Katla's scene graph does this during application initialization. Native
pipeline registries reuse canonical descriptors; command encoding only binds a
prepared pipeline.

Scene animation, Forward+ light culling and particle simulation execute graph
commands. Animation copies each entity's output range into its declared skeleton
buffer, and graphics passes read those actual buffers. Particle counter rollover,
tile-header clearing and skeleton copies are explicit transfer passes. Storage,
uniform and indirect consumers participate in the same range-aware hazard DAG.
Renderer buffer imports resolve the active frame slot and preserve native slice
offsets for animation params and output, light arrays, particle data, dead lists
and alive lists. CPU frame updates only write the acquired slot; static animation
allocation replacement waits for prior GPU use before freeing old buffers. An absent inactive scene
subsystem may have no physical allocation; arbitrary missing user buffers are
errors.

`render_graph::compute_tests` checks actual direct and indirect output, inline
uniform data, storage ranges, transfer/readback ordering and frame slots on the
platform's native backend. Run it with:

```sh
cargo test -p katla_gfx --lib render_graph::compute_tests -- --nocapture
METAL_DEVICE_WRAPPER_TYPE=1 MTL_DEBUG_LAYER=1 \
  cargo test -p katla_gfx --lib render_graph::compute_tests -- --nocapture
```
