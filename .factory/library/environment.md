# Environment

Environment variables, external dependencies, and setup notes.

**What belongs here:** Required env vars, external API keys/services, dependency quirks, platform-specific notes.
**What does NOT belong here:** Service ports/commands (use `.factory/services.yaml`).

---

## Build Requirements

- Rust toolchain (edition 2024 for most crates, 2021 for katla_derive)
- Vulkan SDK (needed for katla_gfx compilation)
- No runtime services needed for this cleanup mission

## Platform

- Windows (win32)
- No special environment variables needed
- No external services required

## Dependency Notes

- **winit 0.30**: Text input is exclusively handled through `Ime::Commit` events. The `text` field was removed from `KeyboardInput` struct. Do not process `event.text` from keyboard input — it will cause duplicate text input.
