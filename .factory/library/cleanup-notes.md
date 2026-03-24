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
