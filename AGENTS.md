# AGENTS.md - Katla Repository Guide

This guide is for agentic coding agents working on the Katla Vulkan-based render engine project.

## Project Overview

Katla is a Rust-based Vulkan render engine using ECS (Entity Component System) architecture. The project is structured as a Cargo workspace with multiple crates:
- `katla_math` - Math library (vectors, matrices, quaternions)
- `katla_vulkan` - Vulkan rendering layer
- `katla_app` - Application framework and components
- `katla_ecs` - Entity Component System framework
- `katla_derive` - Derive macros for the ECS

## Build/Test Commands

### Build Commands
```bash
cargo build                    # Build all workspace crates
cargo build --release          # Build in release mode
cargo check                    # Quick typecheck without building
cargo build -p katla_ecs       # Build specific package
```

### Test Commands
```bash
cargo test                     # Run all tests in workspace
cargo test --workspace         # Run tests for entire workspace
cargo test --package katla_ecs # Run tests for specific package
cargo test test_entity_id_creation  # Run single test by name
cargo test test_entity        # Run tests matching pattern
cargo test -- --nocapture      # Run tests with stdout output
cargo test --release           # Run tests in release mode
```

### Lint Commands
```bash
cargo clippy                   # Run Clippy linter
cargo clippy --fix             # Auto-fix Clippy warnings
cargo fmt                      # Format code
cargo fmt --check              # Check formatting without modifying
```

## Code Style Guidelines

### Imports
- Group imports logically: std lib, external crates, crate modules
- Use `use crate::...` for internal module imports
- Prefer explicit imports over glob imports
```rust
use std::collections::HashMap;
use katla_math::Vec3;
use crate::components::Component;
use crate::entity::EntityId;
```

### Formatting
- Use `cargo fmt` for consistent formatting
- Place `#[derive(...)]` attributes on separate lines for complex derives
- Use `#[inline]` for performance-critical functions
- Keep lines under 100 characters where practical
- Use 4-space indentation (Rust standard)

### Types and Generics
- Use `Option<T>` for nullable values
- Use `Result<T, E>` for error handling
- Prefer `dyn Trait` for trait objects
- Use generic type parameters: T, K, V for types; U, V for types, E for errors
```rust
pub struct SparseSet<K, V> where K: Hash + Eq { ... }
fn get_mut(&mut self, entity_id: EntityId) -> Option<&mut T>
```

### Naming Conventions
- **Structs/Enums/Traits**: PascalCase (`World`, `Component`, `System`)
- **Functions/Methods**: snake_case (`create_entity`, `update`)
- **Constants**: SCREAMING_SNAKE_CASE (`FIRST`, `NORMAL`, `X_AXIS`)
- **Fields/Variables**: snake_case (`next_entity_id`, `update_count`)
- **Type Parameters**: Single uppercase letters (T, K, V)
- **Tests**: Prefix with `test_` (`test_entity_id_creation`)

### Error Handling
- Use `Option<T>` for operations that may not succeed
- Use `Result<T, E>` for operations that can fail with errors
- Use `panic!()` only for truly unrecoverable states
- Provide clear error messages in panics
- Avoid `unwrap()` and `expect()` in production code

### Documentation
- Document public APIs with `///` doc comments
- Use `//!` for module-level documentation
- Include examples in doc comments
- Document type parameters and lifetime parameters
- Add `# Examples` and `# Performance` sections where relevant

### Testing
- Write unit tests in `#[cfg(test)] mod tests` sections
- Place test functions in same file as code being tested
- Use descriptive test names following `test_what_is_being_tested` pattern
- Test both success and failure cases where applicable
- Use `assert_eq!`, `assert_ne!`, `assert!` macros

### Architecture Patterns
- **CRITICAL**: `katla_vulkan` crate must NOT export ash types - create wrapper types instead
- Prevent leaking ash types across crates to maintain clear boundaries
- ECS: Components are pure data with `#[derive(Component)]`
- Systems implement `System` trait with `update()` method
- Components stored in sparse sets for O(1) lookups
- World manages entities, components, and systems
- Use re-exports in lib.rs for public API convenience

### Crate Dependency Restrictions
- **CRITICAL**: `katla_vulkan` must NOT depend on: `katla_math`, `katla_ecs`, `katla_app`
- **CRITICAL**: `katla_ecs` must NOT depend on: `katla_app`, `katla_vulkan`, `katla_math`
- **CRITICAL**: `katla_math` must NOT depend on: `katla_app`, `katla_vulkan`, `katla_ecs`
- These restrictions enforce separation of concerns and maintain clean module boundaries

### Dependencies
- Check existing codebase before adding new dependencies
- Prefer workspace crates for shared functionality
- Use existing libraries: ash, winit, image, gltf, itertools
- Ensure compatibility with Rust 2021 edition

### Performance Considerations
- ECS prioritizes cache locality with contiguous storage
- Use sparse sets for O(1) lookups
- Mark hot path functions with `#[inline]`
- Prefer stack allocation over heap where possible
- Profile with `cargo test --release` for benchmarks

### Common Patterns
- Derive `Debug`, `Clone`, `Copy` for simple types
- Use `Default` trait for initial values
- Implement `Display` for user-facing types
- Use builder pattern for complex initialization (e.g., `ApplicationBuilder`)
- Use `pub(crate)` for internal public APIs
