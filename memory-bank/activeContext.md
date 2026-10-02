# Active Context

## Current State

- Remaining GitHub issue implementations #37 and #93 and the requested ECS/gfx cleanup are complete. Architecture and local acceptance are recorded in systemPatterns.md, progress.md and docs/metal4_validation.md. GitHub is the source of truth for publication, issue state and exact-head CI.
- The hosted macos-26 virtual GPU lacks Metal 4. CI reports native acceptance BLOCKED while running current-SDK/portable tests and typed capability rejection. Physical M5 native results are separate.
- The preexisting GPU particle emitter-index reuse hazard remains in TODO.md and needs generation/retirement design before emitter-slot reuse is safe.
