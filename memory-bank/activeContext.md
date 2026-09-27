# Active Context

## Current Work

- **Session 2026-09-27: issue #31 buffer-resource integration on fresh `main`.** Updated local `main` to `origin/main` at `6de43dd9`; preserved the previous head on `codex/main-before-origin-refresh-20260927`. Current changes are uncommitted.
- Added graph-owned transient buffers and renderer-owned imported buffers for Vulkan and Metal, typed buffer access validation, frame lookup, Vulkan range barriers, and Metal tracked-resource sync records. `cargo fmt --all`, `cargo check --all-targets`, and `git diff --check` pass; tests were not run.
- **Issue #31 remains open:** built-in animation, light-culling, and particle passes still need real buffer declarations; transient buffer aliasing is not implemented. Do not describe the issue as complete.
