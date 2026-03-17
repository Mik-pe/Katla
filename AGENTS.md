# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Katla is a Vulkan-based 3D render engine written in Rust 2024 edition, using ECS (Entity Component System) architecture. The project is structured as a Cargo workspace with multiple crates:

- **katla_math** - Custom math library (vectors, matrices, quaternions) - SIMD planned (see katla_math/PLAN.md)
- **katla_gfx** - Katla high-level graphics API layer
- **katla_app** - Application framework, components, and systems
- **katla_ecs** - Custom Entity Component System framework
- **katla_derive** - Derive macros for the ECS (Component trait)
- **katla_ui** - Immediate mode UI system for debug overlays and in-game HUDs

## Build and Test Commands

```bash
# Build
cargo check                    # Quick typecheck
cargo build                    # Build all workspace crates
cargo build -p katla_ecs       # Build specific package

# Test
cargo test                     # Run all tests
cargo test --workspace         # Explicit workspace tests
cargo test -p katla_ecs        # Test specific package
cargo test test_entity_id_creation  # Run single test
cargo test test_entity        # Run tests matching pattern
cargo test -- --nocapture      # Show stdout

# Lint
cargo clippy                   # Linter
cargo clippy --fix             # Auto-fix
cargo fmt                      # Format

# Run
cargo run                     # Run the application
cargo run -- -s               # Run in limited-frame mode (25 frames) for validation
cargo run -p katla_gfx --example particle_validation  # Headless GPU particle system validation with exit codes (0=success, 1=failure)
```

## Command Line Arguments

- `-s, --single-frame` - Run in limited-frame mode (25 frames) for validation testing. Useful for checking Vulkan validation errors without running indefinitely.

## Working Conventions

- **Task Continuity**: When working with tasks, continue through the task list without asking for confirmation between tasks. If there are pending tasks, proceed to the next one automatically.
- **No Backwards Compatibility**: When introducing new APIs or patterns, don't maintain backwards compatibility or deprecation paths. Just remove the old code and update all usages to the new approach.
- **No Hybrid Implementations**: Avoid having multiple ways of doing the same thing. Similar implementations doing the same work are maintenance burden and bug magnets. When adding new code, look for existing patterns to follow. When replacing old code, remove it entirely—don't leave hybrid states with old and new coexisting.
- **No AI Slop comments**: Avoid adding comments that are obvious or can be inferred from the code itself. Avoid adding comments that have to do with the current issue at hand.

## Git Commit Conventions

### Commit Message Format

Follow a consistent format for commits:

```
Summary line (50-72 chars, imperative mood)

- Optional detailed bullet points
- Each line starts with a hyphen
- Describe WHAT was done, not WHY
- Keep it concise and focused
```

### Commit Guidelines

1. **Test before committing**: Run `cargo test` to ensure all tests pass
2. **Keep commits focused**: One logical change per commit
3. **Write clear summaries**: Use imperative mood ("Add", "Fix", "Refactor")
4. **Include details**: List major files, components, or features added
5. **No Co-Authored-By**: Do not include AI co-authorship tags
6. **Avoid "Update":** Be specific about what was updated

### Commit Workflow

```bash
# Format your changes
cargo fmt

# Check status
git status

# Stage relevant files
git add path/to/files

# Review changes
git diff --staged

# Commit with message
git commit -m "Summary line

- Detail one
- Detail two
- Detail three"
```

## Critical Architecture Rules

### Dependency Restrictions (Enforced Boundaries)

**CRITICAL**: These restrictions maintain clean module boundaries and prevent circular dependencies.

**Rules:**
- **katla_gfx** must NOT depend on: `katla_math`, `katla_ecs`, `katla_app`, `katla_ui`
- **katla_ecs** must NOT depend on: `katla_app`, `katla_gfx`, `katla_math`, `katla_ui`
- **katla_math** must NOT depend on: ANY other crate
- **katla_ui** must NOT depend on: `katla_ecs`, `katla_app`
- **katla_ui** CAN depend on: `katla_math`, `katla_gfx`
- **katla_app** can depend on: `katla_gfx`, `katla_ecs`, `katla_math`, `katla_ui`

## Code Style
  
- **Naming**: `StructName`, `function_name`, `CONSTANT_NAME`, `type_param T`
- **Tests**: Prefix with `test_` (`test_entity_id_creation`)
- **Modules**: Prefer splitting into multiple files/private rust modules over large modules
- **Complexity**: Avoid overly complex types, Result<Rc<Option<RefCell<Option<T>>>>
- **Error Handling**: `Option<T>`, `Result<T, E>`, avoid `unwrap()` in production
- **Documentation**: `///` for public APIs, `//!` for module-level
- **Visibility**: Prefer `pub(crate)` until we know something should be part of the public API. Only promote to `pub` when there's a clear external use case. Keep the public API surface small and intentional.
- **Performance**: Mark hot path functions with `#[inline]`, prefer stack allocation
- **Logging**: Use appropriate log levels (see Logging Guidelines below)

Run `cargo fmt` after applying changes to format the code.

## Logging Guidelines

Use the `log` crate with appropriate levels to balance visibility vs noise:

### Log Levels

| Level | Use For | Examples |
|-------|---------|----------|
| `error!` | Unrecoverable errors, critical failures | GPU device lost, failed to create swapchain |
| `warn!` | Recoverable issues, missing optional data | No normals found, template not found using fallback |
| `info!` | Major lifecycle events, user-visible actions | Window resized, model loaded, hot reload enabled |
| `debug!` | Detailed diagnostic info | Parsed X vertices, shader reloaded, entity spawned |
