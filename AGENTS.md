# AGENTS.md

Katla is a Vulkan/Metal render engine in Rust 2024 with ECS architecture.
Read the touched crate's AGENTS.md when it exists. Follow documentation for the
area being changed; [docs/README.md](docs/README.md) is the task-oriented index.

## Read for the task

- Crate boundaries, assets and math: [architecture](docs/architecture.md).
- Rendering: [graphics ownership](docs/graphics_core.md), [graph API](katla_gfx/src/render_graph/API.md); for Metal, [backend contracts](docs/metal_backend.md).
- ECS: [ownership and systems](docs/ecs.md); for storage/performance decisions, [benchmarks](docs/ecs_benchmarks.md).
- Editor UI: [declarative architecture](docs/declarative_ui_design.md) and [visual design](docs/editor_ui_design.md).
- Scripts or physics: [Luau integration](docs/katla_script_architecture.md) or [Rapier decision](docs/physics-engine-adr.md).
- Validation/CI: [CI policy](docs/ci.md), [cross-backend contracts](docs/contract-suite.md), [native Metal evidence](docs/metal4_validation.md).

Keep the relevant document current when its contract changes. Git/GitHub track
publication and delivery history; TODO.md tracks unresolved engineering work.

## Technical rules

- Preserve crate dependency boundaries. Scene/editor composition belongs in the app; the GPU core owns generic resources, frames and compiled execution.
- Replace superseded code and all usages. No parallel legacy implementation, compatibility/deprecation path or default no-op workaround.
- Matrices are column-major: `Mat4(pub [Vec4; 4])`, `m[col][row]`. Never transpose to adapt a backend.
- Rendering changes require native validation of the affected path. Builds and screenshots alone do not prove GPU behavior. Set `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1` before launching Metal; report unavailable hardware explicitly.
- Metal CI uses exactly `macos-26` on Apple Silicon. Never add older macOS jobs or `macos-latest`. Replace the runner and docs/ci.md together when adopting a newer generation.
- Remove unused code or gate it with cfg; no dead-code suppression. Prefer small responsibility modules, simple types, Result/Option and pub(crate) until an external use exists.
- Avoid production unwrap, obvious comments and issue-specific comments. Document public APIs with /// and modules with //!. Tests use the test_ prefix; hot paths use inline.
- Log unrecoverable GPU failures at error, recoverable fallbacks at warn, lifecycle at info and diagnostics at debug.

## Checks and commits

Use cargo check/test/clippy for the affected crates and cargo fmt after edits.
Native compute checks: `cargo test -p katla_gfx --lib render_graph::native_compute_tests -- --nocapture`.
Windowed Metal validation: `MTL_DEBUG_LAYER=1 METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- -s`.

Test before committing. One logical change per commit; use an imperative 50–72
character summary, describe what changed, avoid “Update” and Co-Authored-By.
Continue authorized tasks without confirmation between routine steps.
