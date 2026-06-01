# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

When making changes to a workspace crate, read the crate's own AGENTS.md (e.g. `katla_gfx/AGENTS.md`) if it exists, as it may contain crate-specific rules and conventions.

## Memory Bank

This project uses a **memory bank** in `memory-bank/` to persist context across sessions. The architecture and patterns live in `memory-bank/systemPatterns.md` — read it before making architectural changes.

### Your responsibilities as an agent:

1. **Read `memory-bank/activeContext.md`** at the start of each session to understand what's in progress.
2. **Read `memory-bank/progress.md`** to see what's been done and what's planned.
3. **Update `memory-bank/activeContext.md`** when you start or finish a task. Remove entries when work is complete.
4. **Update `memory-bank/progress.md`** when you complete work. Remove stale completed entries.
5. **Update `memory-bank/systemPatterns.md`** when you change the architecture, add new crates, change dependency boundaries, or establish new conventions. This is the single source of truth — if it's wrong, fix it.
6. **Update `memory-bank/techContext.md`** when adding/removing dependencies or changing build commands.
7. **Never leave stale entries.** If you refactored away a component, deleted a file, or reversed a decision, remove the old references from the memory bank. Stale docs are worse than no docs.

### Memory bank file guide:

| File | Stability | What goes here |
|------|-----------|----------------|
| `projectbrief.md` | Rarely changes | What the project is and why it exists |
| `systemPatterns.md` | Changes slowly | Architecture, conventions, dependency rules, crate responsibilities |
| `techContext.md` | Changes slowly | Dependencies, build commands, tooling |
| `activeContext.md` | Changes often | What's being worked on right now, recent decisions, open questions |
| `progress.md` | Changes often | What's done, in progress, and upcoming |

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
