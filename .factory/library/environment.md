# Environment

Environment variables, external dependencies, and setup notes.

**What belongs here:** Required env vars, external API keys/services, dependency quirks, platform-specific notes.
**What does NOT belong here:** Service ports/commands (use `.factory/services.yaml`).

---

## Rust Toolchain

- Rust 2024 edition
- Workspace with multiple crates (katla_math, katla_gfx, katla_ecs, katla_ui, katla_app, katla_derive)
- Uses ash for Vulkan bindings
- GPU allocator for Vulkan memory management

## External Dependencies

- **Vulkan SDK**: Required for GPU rendering and validation examples. Not needed for unit tests.
- **No network services, databases, or external APIs**

## Platform Notes

- Windows (win32) development environment
- PowerShell is the default shell (not bash)
- Use `cargo fmt` for formatting, `cargo clippy` for linting
