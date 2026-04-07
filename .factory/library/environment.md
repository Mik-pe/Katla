# Environment

Environment variables, external dependencies, and setup notes.

**What belongs here:** Required env vars, external dependencies, platform-specific notes.
**What does NOT belong here:** Service ports/commands (use `.factory/services.yaml`).

---

## Build Requirements

- Rust 2024 edition (toolchain from rust-toolchain.toml)
- Vulkan SDK (for GPU validation layers)
- Windows 10+ (primary development platform)

## External Dependencies

- GPU with Vulkan support (required for rendering and headless validation)
- No databases, external APIs, or network services

## Platform Notes

- Windows is the primary development platform
- Commands use PowerShell syntax
- `cargo run -- -s` requires a Vulkan-capable GPU (headless frame validation)
