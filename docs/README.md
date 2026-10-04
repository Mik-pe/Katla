# Katla documentation

Read the contracts for the area being changed. Source and pinned dependency
manifests define the current API. Git/GitHub record delivery and CI;
[TODO](../TODO.md) records unresolved engineering work.

| Task | Start here | Related contracts |
| --- | --- | --- |
| Build, run and CI | [Build and launch](odin_build.md), [CI](ci.md) | [Editor completion](odin_editor.md) |
| Package boundaries and ownership | [Architecture](architecture.md) | [Odin packages](../odin/README.md) |
| Scene documents, migration and save/load | [Scene format](scene_format.md) | [Prefabs](prefabs.md), [editor](odin_editor.md) |
| ECS, registry and shared history | [ECS](ecs_odin.md) | [Ownership](ecs.md), [measured storage](ecs_benchmarks.md) |
| Editor UI and docking | [Retained UI](ui_odin.md) | [Native rendering and picking](odin-ui-rendering.md), [visual design](editor_ui_design.md) |
| GPU resources and compiled frames | [gfx](gfx_odin.md) | [Graphics ownership](graphics_core.md), [native contracts](contract-suite.md) |
| Scene rendering, lights and shadows | [Rendering](render_features_odin.md) | [Overlays and depth](editor_overlays_odin.md), [models](gltf_odin.md) |
| Animation and timeline | [Animation](animation_odin.md) | [Scene events](scene-events.md) |
| GPU particles | [Particles](particles_odin.md), [reset ownership](particle_reset_odin.md) | [Scene events](scene-events.md), [rendering](render_features_odin.md) |
| Audio, streams, DSP and mixer | [Audio](audio_odin.md) | [Pinned native dependency](../tools/audio_native/README.md) |
| Luau gameplay | [Runtime contracts](katla_script_architecture.md) | [Scene events](scene-events.md), [editor lifecycle](odin_editor.md) |
| Physics, joints and queries | [Physics ownership](physics-engine-adr.md) | [Box3D native contract](../tools/box3d/README.md) |
| Assets, mesh and prefab authoring | [Prefabs](prefabs.md), [models](gltf_odin.md) | [Confined resources](../odin/resources/README.md), [image thumbnails](thumbnails_odin.md) |
| Agent tools and live room authoring | [Agent ownership](agent_odin.md), [authoring guide](agent-authoring.md) | [MCP viewport sharing](shared-editor-view.md) |
| Shader compilation and refresh | [Compiler/cache](../tools/naga_bridge/README.md) | [Atomic reload](odin-shader-reload.md), [application refresh](render_shader_reload_odin.md) |
| Math and icons | [Math](math_odin.md) | [Leaf packages](odin_leaf_ports.md) |

[Benchmarks](benchmarks/) retain source-hashed historical measurements. The
[archive](archive/) retains completed research and plans, including superseded
Rust designs. Historical evidence does not define the running engine's API.
Update the relevant current contract when its behavior changes; avoid a
parallel delivery/status log.
