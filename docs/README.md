# Katla Documentation

## Memory Bank (`../memory-bank/`)

The primary source of project knowledge for AI agents. Read and maintain these files:

| File | What |
|------|------|
| `projectbrief.md` | What Katla is and why |
| `systemPatterns.md` | Architecture, conventions, crate responsibilities |
| `techContext.md` | Dependencies, build commands |
| `activeContext.md` | What's being worked on right now |
| `progress.md` | What's done, in progress, upcoming |

## Reference Docs

Active design documents and API references:

| File | Description |
|------|-------------|
| `vulkan_to_metal_mapping.md` | Current Vulkan and Metal 4 command/resource mapping |
| `metal_backend.md` | Native Metal 4 backend architecture reference |
| `metal4_validation.md` | Native acceptance and measured before/after frame/startup behavior |
| `metal4_frame_slots.md` | Frame ownership, completion and command submission |
| `metal-binding-residency.md` | Reflected argument tables and residency ownership |
| `metal_pipeline_cache.md` | Native pipeline archives, warmup and shader reload |
| `metal_texture_uploads.md` | Private texture storage and staged subresource uploads |
| `transient_storage.md` | Compiled lifetime allocation, aliasing and tile storage |
| `render_graph_compute.md` | Neutral graph compute commands and built-in dependencies |
| `render_graph_synchronization.md` | Compiled resource hazards and backend translation |
| `archive/metal_backend_implementation.md` | Superseded pre-implementation Metal plan (historical) |
| `backend_agnostic_render_graph.md` | Backend-agnostic render graph design |
| `render_graph_capture.md` | Capturing, diffing, and blessing render-graph diagnostics |
| `declarative_ui_design.md` | Declarative UI system architecture |
| `katla_script_architecture.md` | Luau scripting system design |
| `ecs.md` | ECS ownership, typed systems, commands, lifecycle and migration |
| `ecs_benchmarks.md` | Reproducible sparse/archetype and scheduling measurements |
| `physics-engine-adr.md` | ADR: Why Rapier3D was chosen |
| `character-controller-design.md` | ECS-facing character controller architecture and implementation plan |

## Archive (`archive/`)

Completed work logs, migration plans, and one-off research. Archived for historical reference — not actively maintained.
