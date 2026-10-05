# AGENTS.md

Katla is a Vulkan/Metal render engine in Odin with ECS architecture.
Read the touched package's AGENTS.md when it exists. Follow documentation for the
area being changed; [docs/README.md](docs/README.md) is the task-oriented index.

## Read for the task

- Package boundaries, assets and math: [architecture](docs/architecture.md).
- Rendering: [graphics ownership](docs/graphics_core.md), [graph contracts](docs/gfx_odin.md); for Metal, [backend contracts](docs/metal_backend.md).
- ECS: [ownership and systems](docs/ecs_odin.md); for storage/performance decisions, [benchmarks](docs/ecs_benchmarks.md).
- Editor UI: [retained UI architecture](docs/ui_odin.md) and [visual design](docs/editor_ui_design.md).
- Live scene authoring, room building and material editing: [agent guide](docs/agent-authoring.md).
- Scripts or physics: [Luau integration](docs/katla_script_architecture.md) or [Box3D ownership](tools/box3d/README.md).
- Validation/CI: [CI policy](docs/ci.md), [cross-backend contracts](docs/contract-suite.md), [native Metal evidence](docs/metal4_validation.md).

Keep the relevant document current when its contract changes. Git/GitHub track
publication and delivery history; TODO.md tracks unresolved engineering work.

## Technical rules

- Preserve package dependency boundaries. Scene/editor composition belongs in the app; the GPU core owns generic resources, frames and compiled execution.
- Replace superseded code and all usages. No parallel legacy implementation, compatibility/deprecation path or default no-op workaround.
- Matrices are column-major: `Mat4 = [4]Vec4`, `m[col][row]`. Never transpose to adapt a backend.
- Rendering changes require native validation of the affected path. Builds and screenshots alone do not prove GPU behavior. Set `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launching Metal; report unavailable hardware explicitly.
- Metal CI uses exactly `macos-26` on Apple Silicon. Never add older macOS jobs or `macos-latest`. Replace the runner and docs/ci.md together when adopting a newer generation.
- Remove unused code or gate it with compile-time conditions; no dead-code suppression. Prefer small responsibility modules, simple types, typed errors and optional values until an external use exists.
- Avoid unchecked production failures, obvious comments and issue-specific comments. Document public APIs with /// and modules with //!. Tests use the test_ prefix; hot paths use inline.
- Log unrecoverable GPU failures at error, recoverable fallbacks at warn, lifecycle at info and diagnostics at debug.

## Checks and commits

Use strict Odin checks and tests for the affected packages. Build configured
native dependencies and run the canonical suite with
`odin run tools/build -- --tests`; add `--sanitize` for combined
C/C++/Odin address checks with a matching LLVM compiler.
GPU contracts: `odin run tools/build -- validate gpu --native-metal`.
Windowed Metal validation: `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 odin run tools/build -- run -- --frames 100`.
Cargo checks apply only to the isolated offline compiler under `tools/naga_bridge`.

Test before committing. One logical change per commit; use an imperative 50–72
character summary, describe what changed, avoid “Update” and Co-Authored-By.
Continue authorized tasks without confirmation between routine steps.
