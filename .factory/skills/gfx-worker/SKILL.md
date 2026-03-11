---
name: gfx-worker
description: Worker for graphics system changes including shaders, textures, and rendering pipeline modifications
---

# GFX Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this worker type for:
- Shader modifications (WGSL, GLSL)
- Texture system changes (bindless, descriptors)
- Rendering pipeline modifications
- Vulkan API changes
- Graphics performance optimizations
- UI rendering infrastructure changes

## Work Procedure

### 1. Understand the Feature
- Read the feature description and validation contract assertions
- Identify all files that need modification:
  - Shaders: `resources/shaders/`
  - UI types: `katla_ui/src/types.rs`
  - UI drawing: `katla_ui/src/context/drawing.rs`
  - UI draw list: `katla_ui/src/draw_list.rs`
  - Renderer: `katla_gfx/src/renderer/ui_renderer.rs`
  - Bindless: `katla_gfx/src/vulkan/bindless_texture.rs`
  - Render graph: `katla_gfx/src/render_graph/graph.rs`
- Review existing patterns in similar code

### 2. Write Tests First (TDD)
- Write failing tests BEFORE implementation
- Test the specific behavior being changed
- Place tests in appropriate crate:
  - `katla_ui/src/` for UI data structures
  - `katla_gfx/src/` for rendering infrastructure
  - `katla_gfx/tests/` for integration tests

```rust
#[test]
fn test_bindless_texture_registration() {
    // Test that textures can be registered and return valid indices
}
```

### 3. Implement Changes
- Make minimal, focused changes
- Follow existing Vulkan patterns in the codebase
- Maintain architecture boundaries:
  - `katla_gfx` must NOT depend on `katla_math`, `katla_ecs`, `katla_app`, `katla_ui`
- Use `pub(crate)` by default
- Remove old code entirely (no hybrid implementations)

### 4. Build and Test
```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check for Vulkan validation errors
cargo run -- -s
```

### 5. Visual Verification
- Run `cargo run -- -s` for limited-frame testing
- Check console for Vulkan validation errors
- Verify visual output:
  - Text renders correctly
  - Images/textures display properly
  - No texture bleeding
  - Correct colors and opacity

### 6. Format and Lint
```bash
cargo fmt
cargo clippy
```

## Example Handoff

```json
{
  "salientSummary": "Migrated UI shader to use bindless texture array. Updated vertex struct to include texture_index field. Removed push descriptor code path. All 835 tests pass, no Vulkan validation errors.",
  "whatWasImplemented": "Modified resources/shaders/ui/ui.wgsl to use binding_array<texture_2d<f32>, 4096> instead of separate font_atlas and dynamic_texture. Added texture_index: u32 to Vertex struct in katla_ui/src/types.rs. Updated UIRenderer to use BindlessTextureManager. Removed push_descriptor_khr calls from UI render path. Cleaned up BINDLESS_OFFSET hack in graph.rs.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test --workspace",
        "exitCode": 0,
        "observation": "All 835 tests passed"
      },
      {
        "command": "cargo run -- -s",
        "exitCode": 0,
        "observation": "No Vulkan validation errors. UI renders correctly: text crisp, viewport displays 3D scene, buttons interactive."
      },
      {
        "command": "cargo clippy",
        "exitCode": 0,
        "observation": "No warnings"
      }
    ],
    "interactiveChecks": [
      {
        "action": "Visual inspection of text rendering",
        "observed": "All text labels, buttons, and inputs render with correct glyphs. No missing characters."
      },
      {
        "action": "Visual inspection of viewport",
        "observed": "3D scene displays correctly in viewport panel. No texture bleeding."
      },
      {
        "action": "Visual inspection of interactive elements",
        "observed": "Buttons change color on hover/press. Checkboxes toggle correctly. Sliders work."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "katla_gfx/src/vulkan/bindless_texture.rs",
        "cases": [
          {
            "name": "test_bindless_registration",
            "verifies": "Textures can be registered and return valid slot indices"
          },
          {
            "name": "test_bindless_deregistration",
            "verifies": "Slots are freed when textures are deregistered"
          }
        ]
      },
      {
        "file": "katla_ui/src/types.rs",
        "cases": [
          {
            "name": "test_vertex_texture_index",
            "verifies": "Vertex struct includes texture_index field with correct size"
          }
        ]
      }
    ]
  },
  "discoveredIssues": [],
  "fulfillsAssertions": ["VAL-TEXT-001", "VAL-IMG-001", "VAL-CROSS-009"]
}
```

## When to Return to Orchestrator

- Shader changes require new descriptor layouts that don't exist
- Texture system changes reveal deeper architectural issues
- Vulkan validation errors that can't be resolved within feature scope
- Existing bugs block this feature
- Requirements are ambiguous or need clarification
- The change requires breaking public API changes not anticipated

## Key Files Reference

| Area | Files |
|------|-------|
| Shader | `resources/shaders/ui/ui.wgsl` |
| UI Types | `katla_ui/src/types.rs`, `katla_ui/src/draw_list.rs` |
| UI Drawing | `katla_ui/src/context/drawing.rs`, `katla_ui/src/context/mod.rs` |
| UI Renderer | `katla_gfx/src/renderer/ui_renderer.rs`, `katla_gfx/src/renderer.rs` |
| Bindless | `katla_gfx/src/vulkan/bindless_texture.rs` |
| Render Graph | `katla_gfx/src/render_graph/graph.rs` |
