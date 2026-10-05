# Render graph API

`FrameGraph<B>` owns declared graph resources, compiled liveness and synchronization,
per-frame transient allocations, and execution diagnostics. `B` implements
`RenderGraphBackend`; `AnyFrameGraph` dispatches the same graph operations at runtime.
The renderer owns native resource storage and frame/submission retirement. The
application owns feature composition, shader selection and frame inputs.

An empty graph is valid and emits no scene, UI or postprocessing passes:

```rust
use katla_gfx::{FrameGraphBuilder, VulkanRenderer};

let graph = FrameGraphBuilder::new().build::<VulkanRenderer>()?;
```

The same builder can produce `FrameGraph<MetalRenderer>` on macOS. Graph construction
never initializes an editor. Built-in pass templates declare attachment operations;
they do not install shaders, allocate a font atlas, select objects or create scene
buffers. Supply the pipelines and resource bindings the chosen feature requires.

## Images and attachments

Declare color/depth images with `create_resource(GraphResourceDesc)`. Imported
textures use `import_resource(name, TextureHandle, ImportedImageContract)`. An imported
contract states the arriving image state and any required final state. Undefined
contents cannot be loaded. The acquired backbuffer is an imported graph image;
windowed execution requires its final present state.

Every depth-using pass binds an explicit graph target. A color image, a buffer, the
backbuffer or a missing target produces a structural error before native encoding.
Imported depth texture formats are checked when the backend resolves the image.

```rust
use katla_gfx::render_graph::{
    FrameGraphBuilder, GeometryPass, GraphResourceDesc, GraphResourceType, PassBuilder,
};
use katla_gfx::{VulkanRenderer, texture::ImageFormat};

let graph = FrameGraphBuilder::new()
    .create_resource(GraphResourceDesc {
        name: "depth".into(),
        resource_type: GraphResourceType::DepthAttachment { clear_value: 0.0, sampled: false },
        format: ImageFormat::D32Sfloat,
        width: 640,
        height: 480,
        tracks_swapchain_size: false,
    })
    .add_pass(GeometryPass::new("scene")
        .write_color("backbuffer", ImageFormat::Auto)
        .depth_target("depth"))
    .build::<VulkanRenderer>()?;
```

Choose `GeometryPass::without_depth()` for a depth-free pass. Load/store/clear
operations come from the pass declaration. `ShadowPass::write_depth` binds its named
output as its depth target. Arbitrary scene/HDR/object-ID/viewport textures remain
graph resources, with no renderer-owned fallback.

Pass liveness starts from exported resources and explicit side effects. The
backbuffer is exported by default. Export offscreen results needed after execution,
including object-ID images and readback buffers. Unused passes are culled, including
compute pipeline warmup for those passes. Exporting a resource retains the producer
of every surviving byte range, mip, layer and aspect. A later write replaces only
the range it covers; fully overwritten producers are culled.

## Buffer resources and frame ownership

`create_buffer(GraphBufferDesc)` declares graph-owned per-slot storage.
`import_buffer(name, BufferHandle, BufferDesc)` imports an ordinary application-owned
allocation. Descriptors declare byte capacity, usages and memory policy. Every
consumer supplies typed `BufferAccess` values with access mode, shader stage/transfer
role and exact byte range. Coarse read/write lists cannot replace these declarations.

After acquiring a frame, applications select their own slot's handles with
`rebind_imported_buffer(resource, handle)`. The declared descriptor stays unchanged.
Native resolution checks the actual allocation descriptor before encoding, including
already initialized graphs. `GpuRenderer::write_buffer` takes the acquired token and
checks CPU visibility, ownership and bounds.

`redefine_imported_buffer(resource, handle, desc)` supports application-controlled
capacity changes and invalidates compilation. `remove_imported_buffer(resource)`
requires all command, packet, access and export references to be removed first. IDs
stay reserved, so retiring an import never shifts another resource's ID.

## Compute and drawing inputs

A compute dispatch names a `ComputePipelineDesc` containing the exact WGSL source and
entry point. Reflection supplies its binding interface. `ComputeDispatch` provides
`pipeline: ComputePipelineDesc`, complete bindings and explicit direct workgroups or an indirect command range.
Update workgroups and inline compute bytes through authored command packets.
Frame callbacks submit graphics lists to graphics passes and one composed UI
list to a UI pass. Wrong input kinds are rejected before native encoding.
Callbacks do not override dispatches or push untyped uniform bytes. There are
no built-in buffer roles, named compute kernels or implicit frame-workload
parameters. Zero direct dimensions express an empty workload. Copy/fill commands use
typed transfer accesses; indirect dispatch/draw commands use typed indirect reads.

Graphics inputs use `PassBindings`: layout-selected material pipelines, graph buffer
and sampled image bindings, independent sampler slots, immutable constant blocks and
ordinary draw phases. A phase can draw submissions, selected object indices,
shader-generated vertices or a declared indirect command. Optional viewports have
finite coordinates and positive extents. Stable object-storage indices survive
layout-selected material overrides.

`PassDesc::with_bindings` and `set_pass_bindings(PassId, packet)` preserve the explicit
access contract. Replacement packets validate before mutation and reuse the
compiled execution plan. A rejected replacement leaves the previous packet and plan intact.
Bindings must fit the declared byte/subresource ranges and include
every selected shader stage. Native preflight also checks reflected slot types,
stages and minimum spans. Constants identify their reflected group/binding/stages;
no hidden tonemap, overlay, light or shadow write mutates object storage.

`set_pass_commands(PassId, commands, accesses)` replaces an explicitly authored
compute/transfer workload and its buffer accesses together. Applications obtain
pass handles from their owning graph. Handles survive pass insertion; handles from
another graph are rejected before mutation or native execution. Appending and
inserting passes return typed errors for invalid positions or names and leave
the graph intact on failure. Shader/pipeline preparation occurs before encoding
and uses the compiled live pass order. Services can explicitly prepare an authored
compute descriptor before acquiring a frame when its pass becomes live later.

## Exported image readback

`GpuRenderer::graph_texture_source(resource)` returns the latest committed exported
image generation. An aborted frame does not replace it. The source identifies its
frame slot, acquisition generation and submission. `queue_texture_readback` takes
that exact source and a subresource/region; `poll_texture_readback` returns pending or
one completed typed result without waiting. Native ownership retains the queued
source across graph resize and later slot reuse. Applications interpret pixel data
and map object identifiers to their own entities.

## Diagnostics

Enable execution tracing with `set_execution_trace(true)`. `capture()` joins the
logical/compiled graph, native allocations and observed encoder/submission records.
Capture does not add waits, barriers, compilation or submission work. Compare the
trace against the compiled contract before treating an execution as validated.

See [capture and comparison documentation](../../../docs/render_graph_capture.md)
and [core graphics ownership](../../../docs/graphics_core.md) for complete capture,
minimal-renderer and editor composition examples.
