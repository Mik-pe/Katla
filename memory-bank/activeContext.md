# Active Context

## Current Work

- **Session 2026-09-27: `rmcp` security-advisory remediation.** GitHub reported three unique RMCP advisories against locked version 1.7.0; six Dependabot alerts duplicate them across `Cargo.lock` and `katla_agent/Cargo.toml`.
- `katla_agent` now requires `rmcp` 2.1 or newer. The lockfile resolves `rmcp` and `rmcp-macros` to 2.2.0, above the patched versions. `cargo check -p katla_agent --features mcp-server --all-targets --locked` passes.
- The earlier issue #31 buffer-resource slice was pushed to `main` as `22d80d90`; issue #31 remains open for built-in animation/light-culling/particle buffer declarations and transient buffer aliasing.
