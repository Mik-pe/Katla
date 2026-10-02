# Katla documentation

Read the contracts for the area being changed. Source and manifests define the
current API/dependencies; Git/GitHub record commits, issues and CI. Unresolved
engineering work belongs in [TODO](../TODO.md).

## Choose by task

| Task | Start here | Read when relevant |
| --- | --- | --- |
| Crate boundaries, assets or math | [Architecture](architecture.md) | [Scene format](../katla_app/src/scene/README.md) |
| ECS systems, queries or lifecycle | [ECS ownership](ecs.md) | [Measured storage/scheduling decisions](ecs_benchmarks.md) |
| Animation playback, fades or agent control | [Transition contract](animation-transitions.md) | [Graphics ownership](graphics_core.md), [CI policy](ci.md) |
| GPU API or scene/editor rendering | [Graphics ownership](graphics_core.md) | [Graph API](../katla_gfx/src/render_graph/API.md), [contracts](contract-suite.md) |
| Metal implementation | [Metal backend](metal_backend.md) | [Frame slots](metal4_frame_slots.md), [bindings/residency](metal-binding-residency.md), [pipeline cache](metal_pipeline_cache.md), [texture uploads](metal_texture_uploads.md) |
| Graph dependencies or allocation | [Synchronization](render_graph_synchronization.md) | [Compute](render_graph_compute.md), [transient storage](transient_storage.md) |
| Native/compiled trace mismatch | [Capture and comparison](render_graph_capture.md) | [Vulkan/Metal mapping](vulkan_to_metal_mapping.md) |
| Editor UI behavior | [Declarative UI](declarative_ui_design.md) | [Visual design and inspiration](editor_ui_design.md) |
| External agent sharing the editor viewport | [Shared editor view over MCP](shared-editor-view.md) | [Graphics ownership](graphics_core.md) |
| Scripts | [Luau runtime contracts](katla_script_architecture.md#runtime-contracts) | [ECS ownership](ecs.md) |
| Physics | [Rapier decision and runtime contract](physics-engine-adr.md) | [Character controller design](character-controller-design.md) |
| Build, CI or native acceptance | [CI policy and commands](ci.md) | [Cross-backend contracts](contract-suite.md), [Metal evidence/provenance](metal4_validation.md) |

## Design and evidence

[Backend-neutral graph design](backend_agnostic_render_graph.md) provides design
context alongside the current graph API. [Benchmarks](benchmarks/) retain
source-hashed raw observations; validation documents state their scope and
hardware limitations. Update the relevant contract when its behavior changes.
Avoid a parallel delivery/status log.

[Archive](archive/) contains completed plans and research for historical context;
it is not the current operating contract.
