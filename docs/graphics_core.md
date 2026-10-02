# Graphics ownership and composition

`GpuRenderer` owns native resources, hardware capabilities, frame acquisition,
submission, retirement and readback. `RenderGraphBackend` supplies generic graph
allocation, synchronization and command execution. Neither interface requires
fonts, viewport selection, lights, shadows, particle simulation, picking policy,
outlines or postprocessing.

`AnyRenderer` selects the implementation at construction. Application code uses
ordinary buffer, texture, mesh and material handles thereafter. Graphics passes
carry `PassBindings`: reflected buffers, images, samplers and immutable inline
bytes, plus explicit drawing phases. A phase selects mesh-layout pipelines,
submitted objects, generated vertices or a declared indirect buffer. Pipeline
descriptors specify depth, stencil, blending, color writes and depth bias.

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

Built-ins install normal WGSL compute descriptors and direct or indirect
dispatches. The graph contains no named built-in buffer roles or hidden workload
dispatch policy. Backend constructors do not install these services.

`initialize_compute_pipelines` prepares only live graph commands. Features with
future workloads, such as animation before the first skeleton is loaded, call
`prepare_compute_pipeline` explicitly with the same descriptor they will submit.
Neither preparation operation inserts work into a frame. Encoding requires a
prepared pipeline and reports a missing one without compiling on the frame path.

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

Per-pass packets own inline bytes. Native encoders retain resolved resources,
pipeline variants, descriptor tables and residency until the exact submission
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
