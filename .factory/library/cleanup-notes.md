# Cleanup Mission Notes

Notes specific to the repo-wide cleanup mission.

## Build Times

- `cargo check`: ~7 seconds
- `cargo clippy`: ~10 seconds
- `cargo test -p katla_math`: fast (~2s)
- `cargo test -p katla_ecs`: fast (~2s)
- `cargo test -p katla_gfx`: moderate (~30s, GPU tests)
- `cargo test -p katla_app`: moderate (~10s)
- `cargo test -p katla_ui`: moderate (~5s)
- `cargo test -p katla_derive`: fast (~5s)
- `cargo test --workspace`: VERY SLOW (~5min, avoid unless necessary)

**Always use per-crate test commands, not workspace-wide.**

## Dead Code Removal Notes

- When removing dead code from katla_ui, many items are in the `context/` submodule tree. Check both the method definition AND the struct/impl it belongs to.
- The `katla_app/src/lib.rs` blanket `#![allow(dead_code)]` may reveal many more dead_code warnings when removed. Workers should fix all of them, not just the 17 currently visible.
- `katla_math/src/sse/quat.rs` - SSE Quat is LIVE CODE. It's re-exported via `pub use crate::sse::quat::Quat` in `katla_math/src/quat.rs` on x86/x86_64 targets. Do NOT remove the module. The `#[allow(dead_code)]` annotations were already removed in the dead-code-cleanup milestone.

## File Splitting Notes

- When splitting files, use `git mv` to preserve history
- Extract submodules ONE AT A TIME, verifying compilation after each
- The `Frame` struct in render_graph/frame.rs has many private fields accessed by methods. These need `pub(super)` visibility after splitting.
- `particles/mod.rs` already exists as a directory - just add new files alongside existing submodules
- `text/mod.rs` and `context/mod.rs` in katla_ui also already exist as directories
- Struct fields that submodules need access to must be widened to `pub(super)`. This is a consistent pattern across all splits (e.g., renderer widened 3 fields: last_presented_image_index, default_material_handle, pending_readback).
- Helper types used across submodules within the same directory may need `pub(crate)` promotion (not just `pub(super)`) to satisfy privacy boundaries. Example: `EmitterState` in particles/types.rs was promoted from `pub(super)` to `pub(crate)` because dispatch.rs needed access but the borrow checker treats submodules within the same directory as separate privacy boundaries for type visibility.
- Feature descriptions for file splits are aspirational about what stays in mod.rs. The 500-line-per-file constraint may require moving additional types to domain submodules (e.g., vulkan/context moved ValidationMode/ValidationLevel/RenderTexture to their domain submodules to keep mod.rs under 500 lines).

## Comment Cleanup Criteria

**Remove:**
- Comments that restate the code: `// Create buffer` before `Buffer::new()`
- Comments that restate the function name: `// Insert barriers` before `fn insert_barriers()`
- Section dividers between related items: `//----` between two methods of the same type
- Commented-out code blocks (not TODOs)

**Preserve:**
- Vulkan synchronization explanations (barrier.rs has many useful ones)
- Comments explaining WHY, not WHAT
- TODOs with actionable intent
- Public API documentation (`///`)
- Comments explaining non-obvious constraints or invariants
