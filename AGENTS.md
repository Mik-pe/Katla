# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

When making changes to a workspace crate, read the crate's own AGENTS.md (e.g. `katla_gfx/AGENTS.md`) if it exists, as it may contain crate-specific rules and conventions.

## Memory Bank

`memory-bank/` is how you remember across sessions. You are stateless — these files are your state.

**Every session:**
1. Read all files in `memory-bank/` before making changes
2. Update `activeContext.md` and `progress.md` when you finish work

**Principles:**
- Write what you'd need re-explained if you started fresh — architecture decisions, conventions, gotchas, what's in-flight
- Never leave stale entries. If code was removed, decisions reversed, or bugs fixed — delete the old reference
- Keep `activeContext.md` lean: only what's in-flight right now
- `systemPatterns.md` is the architecture bible — update it when crate structure or conventions change
- Don't put code snippets or implementation details in memory bank — the code is the source of truth
- When in doubt, update. Stale docs are worse than no docs

## Project Overview

Katla is a Vulkan/Metal 3D render engine in Rust using ECS architecture. See `memory-bank/systemPatterns.md` for the full architecture description.

## Build and Test Commands

```bash
cargo check                    # Quick typecheck
cargo build                    # Build all workspace crates
cargo build -p katla_ecs       # Build specific package

# Test
cargo test                     # Run all tests
cargo test --workspace         # Explicit workspace tests
cargo test -p katla_ecs        # Test specific package
cargo test test_entity_id_creation  # Run single test
cargo test -- --nocapture      # Show stdout

# Lint
cargo clippy                   # Linter
cargo clippy --fix             # Auto-fix
cargo fmt                      # Format

# Run
cargo run                     # Run the application
cargo run -- -s               # Run in limited-frame mode (100 frames)
METAL_DEVICE_WRAPPER_TYPE=1 cargo run -- -s  # Metal validation (macOS)
cargo run -p katla_gfx --example particle_validation  # Headless GPU validation
```

## Command Line Arguments

- `-s, --single-frame` — Run in limited-frame mode (100 frames)
- `-v, --gpu-validation` — Enable GPU-assisted validation (Vulkan only)

## Metal Validation

`METAL_DEVICE_WRAPPER_TYPE=1` must be set before process launch — `std::env::set_var()` is too late.

## CI Runner Policy

- Primary Metal CI uses the explicit `macos-26` Apple Silicon runner.
- Compatibility CI uses the explicit `macos-15` Apple Silicon runner.
- Do not use `macos-latest`; it is a mutable alias and does not currently mean macOS 26.
- Do not add `macos-14` to required CI unless the supported-platform policy is deliberately changed.
- When a new macOS runner becomes generally available, move the two-generation matrix forward intentionally and update `docs/ci.md` in the same change.

## Working Conventions

- **Task Continuity**: Continue through the task list without asking for confirmation between tasks.
- **No Backwards Compatibility**: Don't maintain backwards compatibility or deprecation paths. Remove old code and update all usages.
- **No Hybrid Implementations**: Don't have multiple ways of doing the same thing. Remove the old approach entirely.
- **No AI Slop Comments**: Don't add comments that state the obvious. Don't add comments about the current issue.

## Git Commit Conventions

```
Summary line (50-72 chars, imperative mood)

- Optional detail bullets
- Describe WHAT was done, not WHY
```

Rules: test before committing, one logical change per commit, imperative mood, no `Update` (be specific), no Co-Authored-By.

## Matrix Conventions

Column-major only. `Mat4(pub [Vec4; 4])`. `m[col][row]`. Do NOT transpose. This matches Vulkan/GLSL.

## Code Style

- No `#[allow(dead_code)]` — remove unused code or use `#[cfg(...)]`
- `StructName`, `function_name`, `CONSTANT_NAME`
- Tests prefixed with `test_`
- Prefer multiple files over large modules
- Avoid complex nested types (`Result<Rc<Option<RefCell<Option<T>>>>`)
- `Option<T>` / `Result<T, E>`, avoid `unwrap()` in production (fine in `#[test]`)
- `///` for public APIs, `//!` for module-level
- `pub(crate)` until there's a clear external use case
- `#[inline]` on hot paths
- Run `cargo fmt` after changes

## Logging

| Level | Use For |
|-------|---------|
| `error!` | Unrecoverable: GPU device lost, swapchain creation failed |
| `warn!` | Recoverable: missing optional data, using fallback |
| `info!` | Lifecycle: window resized, model loaded, hot reload |
| `debug!` | Diagnostic: parsed X vertices, shader reloaded, entity spawned |
