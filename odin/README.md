# Katla in Odin

`katla` is the canonical editor executable. Packages follow ownership boundaries;
scene policy belongs to the application and generic GPU execution belongs to gfx.

| Package | Responsibility | Contract |
| --- | --- | --- |
| `katla` | Startup, native platform/window routing and the editor frame loop | [Build](../docs/odin_build.md), [editor](../docs/odin_editor.md) |
| `app`, `app/editor` | Scene documents, reversible authoring, panels and gameplay integration | [Architecture](../docs/architecture.md), [scene format](../docs/scene_format.md) |
| `app/render` | Materials, models, lights, shadows, particles and viewport composition | [Rendering](../docs/render_features_odin.md), [models](../docs/gltf_odin.md) |
| `ecs`, `editor` | Generational entities, storage, registry metadata and owned shared history | [ECS](../docs/ecs_odin.md) |
| `agent`, `agent/mcp`, `mcp_stdio`, `mcp_proxy` | Bounded host/mailbox ownership, tools and live private transport | [Agent](../docs/agent_odin.md) |
| `gfx`, `gfx/metal`, `gfx/vulkan`, `gfx/spirv` | Resources, immutable compiled frames, native execution and reflection | [GPU contracts](../docs/gfx_odin.md) |
| `gfx/shader` | Cached external compiler jobs and validated immutable artifacts | [Shader compilation](../tools/naga_bridge/README.md) |
| `ui` | Retained widgets, Odin flex/grid layout, docking and input ownership | [UI](../docs/ui_odin.md) |
| `resources` | Confined retained directory capabilities and atomic publication | [Resources](resources/README.md) |
| `script` | Direct pinned Luau VM with protected ownership boundaries | [Luau](../docs/katla_script_architecture.md) |
| `physics/box3d` | Native bodies, owned geometry, constraints and spatial queries | [Box3D](../tools/box3d/README.md) |
| `audio` | Clip/stream mixing, cues, spatial audio, DSP and device recovery | [Audio](../docs/audio_odin.md) |
| `math`, `icons` | Column-major geometry/math and editor icon catalogue | [Math](../docs/math_odin.md), [leaf packages](../docs/odin_leaf_ports.md) |
| `deps` | Small explicit C/C++ ABI boundaries, source pins and native ownership | [Build](../docs/odin_build.md) |

Build and run from the repository root:

```sh
python3 scripts/build_katla_odin.py --tests
python3 scripts/run_katla_odin.py --no-build
python3 scripts/validate_odin.py --sanitize
```

Packages use relative imports, compiler `core`, and explicit native dependency
paths. No Rust engine, script/layout runtime or fallback editor is linked.
The isolated Naga command-line compiler is a build tool, invoked only when
shader source/options/compiler freshness requires it. Native GPU fixtures live
in `gfx_native`, `gfx_vulkan_native`, `gfx_conformance` and `examples`;
their actual devices, validation and resource-lifetime checks are distinct from
portable package acceptance.
