# Environment

Environment variables, external dependencies, and setup notes for the Katla engine.

**What belongs here:** Required env vars, external dependencies, platform-specific notes.
**What does NOT belong here:** Service ports/commands (use `.factory/services.yaml`).

## Rust Version

- Rust 2024 edition
- Uses Cargo workspace with multiple crates

## Vulkan

- Requires Vulkan SDK installed
- Validation layers available for debugging
- Headless context available for testing without display

## Platform Notes

- Windows: Primary development platform
- Uses winit for window management

## Build Dependencies

No external dependencies beyond Rust toolchain and Vulkan SDK.
