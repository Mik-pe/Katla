# Active Context

## Current Work

- The `rmcp` security remediation was pushed to `main` as `04757698`. `katla_agent` requires `rmcp` 2.1 or newer; the lockfile resolves `rmcp` and `rmcp-macros` to 2.2.0. The MCP-server feature build passes, and GitHub now reports no open Dependabot alerts.
- Issue #31 remains open after buffer-resource slice `22d80d90`; built-in animation/light-culling/particle buffer declarations and transient buffer aliasing remain.
