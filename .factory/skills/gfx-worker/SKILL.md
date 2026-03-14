---
name: gfx-worker
description: Graphics system worker for shaders, textures, render graph passes, and rendering pipeline modifications
---

# GFX Worker

Graphics system implementation worker for the Katla render engine. Handles shaders, textures, render graph passes, and rendering pipeline modifications.

**NOTE:** Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this worker for features that:
- Modify `katla_gfx` crate
- Implement or modify WGSL shaders
- Create render graph passes or templates
- Work with textures, descriptors, or Vulkan resources
- Modify rendering pipeline or frame graph execution
- Implement graphics debugging or validation features

## Work Procedure

### 1. Test-Driven Development (TDD)

**Write tests FIRST:**
```rust
#[test]
fn test_feature_name() {
    // Red: Test fails initially
    let result = feature_api();
    assert!(result.is_ok());
}
```

**Then implement to make tests pass:**
```rust
fn feature_api() -> Result<(), Error> {
    // Green: Implementation makes test pass
    Ok(())
}
```

**Why TDD for graphics:**
- Catches API design issues early
- Documents expected behavior
- Prevents "it works on my machine" issues
- Makes refactoring safer

### 2. Shader Development Pattern

**Step 1: Write WGSL shader**
- Create file in `resources/shaders/feature.wgsl`
- Use WGSL syntax and best practices
- Follow existing shader patterns (see `ui.wgsl`, `tonemapping.wgsl`)

**Step 2: Validate WGSL**
```bash
# Shader compilation happens during build
cargo build --bin katla
```

**Step 3: Test shader output**
```bash
# Visual verification with Vulkan validation
cargo run -- -v -- -s
```

**Common shader issues:**
- WGSL syntax errors (naga will report)
- Descriptor layout mismatch (Vulkan validation will report)
- Incorrect binding numbers (check set/binding match Rust code)

### 3. Frame Graph Pass Development

**Follow existing patterns:**
```rust
// 1. Create pass struct
pub struct FeaturePass {
    name: String,
    reads: Vec<String>,
    writes: Vec<String>,
    material: Option<MaterialHandle>,
}

// 2. Implement PassBuilder trait
impl PassBuilder for FeaturePass {
    fn as_builder(self) -> InternalPassBuilder {
        // Generate pass descriptor with correct dependencies
    }
}

// 3. Add to render_graph/passes/feature.rs
// 4. Export from render_graph/passes/mod.rs
```

**Key principles:**
- Declare resource reads/writes explicitly
- Frame graph handles barriers automatically
- Use transient textures for intermediate results
- Follow naming conventions: `"resource_name"` format

### 4. Visual Verification

**Use validation mode:**
```bash
# 25 frames then exit (good for iterative development)
cargo run -- -s

# With Vulkan validation layers
cargo run -- -v -- -s

# Check for black frames (reads back center pixel)
cargo run -- -s --check-black-frames
```

**What to look for:**
- Black screens (descriptor/shader issue)
- Flickering (synchronization issue)
- Corruption (memory/layout issue)
- Incorrect positioning (rectangle/transform issue)

### 5. Running Tests

**Unit tests:**
```bash
cargo test -p katla_gfx
cargo test test_feature_name
```

**Integration tests:**
```bash
cargo test --workspace
cargo test -p katla_gfx --test integration_test_name
```

**With output:**
```bash
cargo test -- --nocapture
cargo test -- --test-threads=1
```

### 6. Verification Checklist

Before completing a feature:

**Code Quality:**
- [ ] `cargo fmt` run
- [ ] `cargo clippy` shows no warnings
- [ ] Code follows Katla conventions (AGENTS.md)
- [ ] Public APIs have `///` documentation

**Testing:**
- [ ] Unit tests added and passing
- [ ] Integration tests added and passing
- [ ] Visual verification completed (screenshot if applicable)
- [ ] Vulkan validation shows no errors

**Functionality:**
- [ ] Feature works as specified
- [ ] Edge cases handled (errors, limits, etc.)
- [ ] No dead code or unused imports
- [ ] No performance regression (check frame times if relevant)

## Example Handoff

```json
{
  "salientSummary": "Implemented CompositingDescriptorSet with fixed texture array bindings (max 8 viewports). Added descriptor set layout (set 2, binding 0), validation for viewport count limits, and update_textures() method. Shader compilation successful, pipeline layout creation works. All tests passing, Vulkan validation clean.",
  "whatWasImplemented": "Created CompositingDescriptorSet struct in katla_gfx/src/render_graph/descriptor_sets/compositing.rs. Descriptor set uses fixed array of 8 sampler2D bindings at set 2, binding 0. Added validation to enforce max 8 viewport limit (returns Error::TooManyViewports). Implemented update_textures() method to replace viewport textures at runtime. Created unit tests for creation, max limit validation, and empty viewport handling. Descriptor set layout matches WGSL shader expectations.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test -p katla_gfx test_compositing_descriptor_set",
        "exitCode": 0,
        "observation": "All 3 tests passed (creation, max limit, empty)"
      },
      {
        "command": "cargo build --bin katla",
        "exitCode": 0,
        "observation": "Shader compilation successful, no WGSL errors"
      },
      {
        "command": "cargo run -- -v -- -s",
        "exitCode": 0,
        "observation": "Vulkan validation clean, no descriptor layout errors"
      }
    ],
    "interactiveChecks": [
      {
        "action": "Created 2-viewport split-screen layout",
        "observed": "Both viewports rendered correctly at expected screen positions, no visual artifacts"
      },
      {
        "action": "Created 4-viewport grid layout",
        "observed": "All 4 viewports rendered in correct quadrants, clean division between viewports"
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "katla_gfx/src/render_graph/descriptor_sets/compositing.rs",
        "cases": [
          {
            "name": "test_compositing_descriptor_set_creation",
            "verifies": "Descriptor set creation with valid viewport textures succeeds"
          },
          {
            "name": "test_compositing_descriptor_set_max_limit",
            "verifies": "Creating with >8 textures returns Error::TooManyViewports"
          },
          {
            "name": "test_compositing_descriptor_set_empty",
            "verifies": "Creating with 0 textures succeeds (edge case)"
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

Return to orchestrator if:

**Blocking Issues:**
- Existing bugs in frame graph or render graph block implementation
- Vulkan SDK issues or missing dependencies
- Cannot create descriptor sets or pipelines due to existing issues
- Frame graph compilation fails for unrelated reasons

**Scope Issues:**
- Need to modify application layer (game/src/main.rs or katla_app)
- Feature requires changes outside katla_gfx crate
- Requirements conflict with existing architecture

**Clarification Needed:**
- Implementation plan is ambiguous or contradictory
- Need architectural decision that affects multiple areas
- Not clear how to integrate with existing systems

**DO NOT return for:**
- Expected implementation challenges (shader debugging, etc.)
- Test failures that can be fixed
- Documentation updates within scope
- Refactoring within katla_gfx
- Performance optimization that can be done iteratively

## Common Pitfalls

### DON'T: Ignore TDD
- Wrong: Write implementation first, then add tests
- Correct: Write failing tests first, then implement

### DON'T: Skip Visual Verification
- Wrong: Assume rendering works without checking
- Correct: Always run `cargo run -- -s` and verify output

### DON'T: Ignore Vulkan Validation
- Wrong: Suppress or ignore validation errors
- Correct: Fix all validation errors before completing feature

### DON'T: Break Conventions
- Wrong: Use different naming or patterns than existing code
- Correct: Follow existing Katla conventions (AGENTS.md)

### DON'T: Skip Documentation
- Wrong: Leave public APIs undocumented
- Correct: Add `///` docs for all public interfaces

## Resources

**Internal:**
- `C:\dev\katla\AGENTS.md` - Coding conventions and guidelines
- Mission AGENTS.md - Mission-specific guidance and boundaries
- `docs/feature-multi-viewport/` - Implementation documentation

**External:**
- WGSL Spec: https://www.w3.org/TR/WGSL/
- Vulkan Spec: https://registry.khronos.org/vulkan/specs/1.3/html/
- Vulkan Guide: https://vkguide.dev/

**Examples:**
- `katla_gfx/src/render_graph/passes/*.rs` - Existing pass templates
- `resources/shaders/*.wgsl` - Existing shader implementations
- `katla_gfx/tests/*.rs` - Integration test examples
