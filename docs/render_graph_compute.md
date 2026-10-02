# Compute commands and graph buffers

Compute and transfer work uses `PassDesc.commands` on both Vulkan and Metal.
`ComputeDispatch.pipeline` is a `ComputePipelineDesc` with the exact WGSL source
and entry point. Each dispatch declares its buffer bindings and byte ranges, plus
explicit direct workgroups or a declared indirect command buffer. Both backends
use the same reflected interface and validate native allocation bounds.

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
    .build::<VulkanRenderer>()?;
```

Initialize contents with a transfer pass before using a read/write shader.
`dispatch_indirect(name, offset)` declares the twelve-byte command read.
`constants(bytes)` writes the reflected uniform binding and requires a buffer
with `UNIFORM | TRANSFER_DESTINATION` capabilities. Inline data must contain a
multiple of four bytes, no more than 65536 bytes, and fit the declared range.

Compilation rejects missing, duplicated, or unknown binding slots, a mismatch
with the shader's read/write declarations, unsupported non-buffer bindings,
commands outside the pass's declared ranges, and unaligned indirect dispatches.
The native adapters derive their binding layouts and runtime-array bounds from
the same source. User byte ranges remain authoritative when native buffers
contain larger allocations. Reflection also rejects ranges smaller than
the WGSL buffer type. Particle storage arrays use their runtime byte ranges and
configured pool capacity, allowing small pools without fixed million-element
allocations.

Call `initialize_compute_pipelines` before acquiring a frame. It prepares only
compiled live passes. A service whose workload becomes live later can prepare its
own descriptor explicitly with `AnyFrameGraph::prepare_compute_pipeline` before
acquisition. Command encoding binds prepared pipelines and never compiles one.

Application services own scene animation, Forward+ light culling and particle
simulation. They import ordinary per-slot buffers and author dispatches, clearing,
rollover and skeleton copies as graph commands. Graphics consumers declare the
same actual resources and ranges. No renderer feature role or frame-workload
callback resolves a hidden allocation or dispatch size.

After acquisition, the application rebinds imports to its selected slot and uploads
CPU data through the frame token. Imported descriptors remain fixed until the
application explicitly redefines capacity. Exported buffers retain every producer
of the surviving byte ranges; a later partial write replaces only its own range.

`render_graph::compute_tests` checks actual direct and indirect output, inline
uniform data, storage ranges, transfer/readback ordering and frame slots on the
platform's native backend. Run it with:

```sh
cargo test -p katla_gfx --lib render_graph::compute_tests -- --nocapture
METAL_DEVICE_WRAPPER_TYPE=1 MTL_DEBUG_LAYER=1 \
  cargo test -p katla_gfx --lib render_graph::compute_tests -- --nocapture
```

`render_graph::native_compute_tests` covers ordinary imported shader workloads for
animation and small particle pools. These native fixtures exercise the same
contracts used by application services.

## Animation sampling

The application animation player owns playback time, looping and clip transitions.
GPU pose evaluation clamps that time to the clip and each channel's keyframe
interval; it does not wrap time a second time or subtract an epsilon from the
last sample. STEP includes the final keyframe, and single-keyframe channels stay
constant.

CPU imports and GPU sampling share glTF's CUBICSPLINE layout:
`[in-tangent, value, out-tangent]` for each keyframe. Hermite tangents scale by the
interval duration, and cubic quaternion samples normalize after interpolation.
The native animation fixtures validate translation, rotation, scale, endpoint
clamping and single-keyframe channels across queued frame slots:

```sh
MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 \
  cargo test -p katla_gfx --lib render_graph::native_compute_tests::animation \
  --locked -- --nocapture --test-threads=1
```
