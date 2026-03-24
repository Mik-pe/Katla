---
name: split-worker
description: Splits large Rust source files into well-organized submodules using impl blocks in the Katla project.
---

# Split Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Features involving splitting large files into submodules in the Katla repo. Used for milestones: file-splitting-gfx, file-splitting-ui.

## Required Skills

None.

## Work Procedure

### 1. Read Feature Description

Read the feature description from features.json carefully. It specifies:
- The source file to split
- What stays in mod.rs (struct definition, core methods)
- What goes in each submodule (method names, line ranges)
- Expected submodule names

### 2. Analyze the Source File

Read the full source file. For each method/group being extracted:
- Identify all types, traits, and imports it uses
- Determine what needs to be `pub(crate)` or `pub(super)` vs private
- Note any helper types defined in the same file that the method needs
- Check if the method accesses private fields of the main struct

### 3. Plan the Split

Before making any changes, create a clear plan:
1. Create the directory (e.g., `frame/` from `frame.rs`)
2. Move `frame.rs` to `frame/mod.rs`
3. Create each submodule file
4. Update `mod.rs` to declare submodules and make fields `pub(super)` where needed
5. Update parent module's `mod` declaration if needed (it usually doesn't change since `mod frame;` still works)

### 4. Execute the Split

**Step 1: Create directory and move file**
```bash
mkdir katla_gfx/src/render_graph/frame
git mv katla_gfx/src/render_graph/frame.rs katla_gfx/src/render_graph/frame/mod.rs
```

**Step 2: Add submodule declarations in mod.rs**
Add `mod barriers;`, `mod graphics_pass;`, etc. at the top of mod.rs.

**Step 3: Adjust visibility**
- Struct fields that submodule impl blocks need access to: change from private to `pub(super)`
- Helper types that submodules need: change to `pub(super)`
- Methods being extracted: change from `pub` to `pub(super)` (unless they were already public API - check parent module exports)

**Step 4: Create submodule files**
Each submodule file follows this pattern:
```rust
use super::*;

impl<'a> super::Frame<'a> {
    pub(super) fn insert_barriers(&mut self, ...) -> Result<()> {
        // method body moved here
    }
}
```

Key rules:
- Always `use super::*;` at the top
- Use `impl<'a> super::Frame<'a>` (not just `impl Frame`) to reference the parent struct
- Methods should be `pub(super)` unless they need to be `pub` for the public API
- Move any helper functions/types that only the extracted methods use into the submodule

**Step 5: Update parent module exports**
If the parent module has `pub use frame::Frame;`, this still works because the directory is still called `frame` and mod.rs exports Frame.

**Step 6: Verify after EACH submodule extraction**
```bash
cargo check -p katla_gfx  # or katla_ui
cargo test -p katla_gfx
```

Fix any compilation errors before moving to the next submodule. Do NOT extract all submodules at once - do them one at a time, verifying each.

### 5. Handle Edge Cases

**Private fields**: If a submodule method accesses `self.renderer` (private field), make the field `pub(super)` in mod.rs. This is safe because only submodule files within the same directory can access it.

**Helper types**: If a method uses a private helper struct (e.g., `PassExecutionData`), either move it to the submodule or make it `pub(super)`.

**Imports**: Submodules inherit the parent's imports via `use super::*;`. If the submodule needs additional imports, add them at the top of the submodule file.

**Tests**: If the original file had `#[cfg(test)] mod tests`, keep it in mod.rs. Tests test the public/super API, not internal implementation.

**Existing submodules**: For particles/mod.rs, the directory already exists with submodules. Just add new files alongside existing ones.

### 6. Final Verification

After all submodules are extracted:
```bash
cargo fmt
cargo check -p {crate}
cargo clippy -p {crate}
cargo test -p {crate}
cargo build
```

Also check that downstream crates still compile:
```bash
cargo check -p katla_app  # if you modified katla_gfx
```

### 7. Commit

Commit with a clear message: "Split render_graph/frame.rs into submodules". One commit per file split.

## Example Handoff

```json
{
  "salientSummary": "Split render_graph/frame.rs (2,916 lines) into 8 submodules. Each submodule under 500 lines. All imports resolve, all 432 katla_gfx tests pass.",
  "whatWasImplemented": "Split katla_gfx/src/render_graph/frame.rs into frame/ directory with mod.rs + 8 submodules: barriers.rs (insert_barriers, insert_post_pass_barriers), graphics_pass.rs (execute_graphics_pass, execute_fullscreen_pass), shadow_pass.rs (execute_shadow_pass), depth_prepass.rs (execute_depth_prepass), particle_rendering.rs (render_particles_to_texture), ui_rendering.rs (execute_ui_draw_list + UI buffer/descriptor methods), draw_calls.rs (execute_draw_list, execute_draw_call, bind_descriptor_sets), compositing.rs (execute_compositing_pass, get_or_create_compositing_descriptor_set). Made Frame fields pub(super) where needed. render_graph/mod.rs exports unchanged.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {"command": "cargo check -p katla_gfx", "exitCode": 0, "observation": "Zero errors"},
      {"command": "cargo test -p katla_gfx", "exitCode": 0, "observation": "432 tests passed"},
      {"command": "cargo check -p katla_app", "exitCode": 0, "observation": "Zero errors (downstream crate)"}
    ],
    "interactiveChecks": []
  },
  "tests": {
    "added": []
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- A submodule extraction causes cascading compilation errors that are hard to resolve
- Making a struct field `pub(super)` exposes something that shouldn't be visible to submodules
- The split breaks downstream crate imports in unexpected ways
- A method being extracted has complex dependencies that make the split impractical
