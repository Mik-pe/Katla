# User Testing

Testing surface, required tools, and resource classification for validation.

**What belongs here:** Testing surface knowledge, tool requirements, resource constraints.
**What does NOT belong here:** Service commands (use `.factory/services.yaml`).

---

## Validation Surface

### Primary: CLI / Build Tools
- `cargo check --workspace` — typecheck
- `cargo test --workspace` — all unit + integration tests
- `cargo clippy --workspace` — lint
- `cargo fmt -- --check` — format
- `cargo run -- -s` — headless 25-frame GPU validation

### Secondary: GPU Validation Examples
- `cargo run -p katla_gfx --example picking_validation` — GPU picking
- `cargo run -p katla_gfx --example outline_validation` — outline rendering

### Feature Flag Testing
- `cargo check -p katla_app --features editor` — with editor
- `cargo check -p katla_app --no-default-features` — without editor

## Required Tools
- Rust toolchain (cargo, rustc, clippy, rustfmt)
- Vulkan SDK (for headless GPU validation)
- ripgrep (for grep-based evidence collection)

## Validation Concurrency

This is a Rust workspace with no running services. Validation is CPU-bound + GPU-bound:
- `cargo test` parallelism controlled by test runner (default: num_cpus)
- `cargo check/clippy` single-process
- GPU validation runs (`cargo run -- -s`) require exclusive GPU access — **max concurrent: 1**

Resource cost per validator:
- Test runner: ~200-500MB RAM, CPU-bound
- Headless GPU run: ~500MB-1GB RAM + GPU
- Clippy/check: ~500MB-1GB RAM per crate

**Max concurrent validators: 3** (limited by memory on typical dev machine)
