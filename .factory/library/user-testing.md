# User Testing

Testing surface, required testing skills/tools, and resource cost classification.

## Validation Surface

This mission involves a native Rust library/engine with no web or CLI surface for end users.

**Automated testing (no GPU required):**
- `cargo test --workspace` — primary validation (ECS, GFX, UI, App unit tests)
- `cargo clippy --workspace -- -D warnings` — linting
- `cargo fmt --check` — formatting
- `cargo check --workspace` — type checking
- `cargo test --doc -p katla_ecs` — doctests

**Automated testing (GPU required, only for visual verification):**
- `cargo run -- -s` — single-frame mode (25 frames, requires Vulkan)

**No manual user testing surface** — all validation is through compilation and unit tests.

## Validation Concurrency

Max concurrent validators: 1

This is a single-threaded Rust compilation and test pipeline. No GPU-intensive validation needed. Machine has sufficient resources for cargo test.

## Testing Tools

No special testing tools required (no agent-browser, no tuistory). All validation is through cargo commands.

## Pre-existing Issues

- **Known test failure**: `katla_gfx::primitives::cone::tests::test_cone_vertex_count` — expects 52 vertices, got 36. Being fixed as VAL-GFX-001.
