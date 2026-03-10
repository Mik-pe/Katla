---
name: ui-rendering-worker
description: Worker for UI rendering fixes and test coverage improvements
---

# UI Rendering Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this worker type for:
- Fixing UI transparency/opacity issues
- Fixing UI positioning and coordinate handling
- Adding tests for UI rendering correctness
- Improving font atlas handling
- Any UI rendering quality improvements

## Work Procedure

### 1. Understand the Problem
- Read the feature description carefully
- Identify the specific files involved
- Review existing code patterns in the area
- Check the validation contract assertions being addressed

### 2. Write Failing Tests First (TDD)
- Before any implementation, write a test that demonstrates the issue
- The test MUST fail before you implement the fix
- Tests should be specific and target the exact behavior
- Place tests in the appropriate module:
  - `katla_ui/src/` for UI crate tests
  - `katla_gfx/src/` for graphics tests
  - `katla_app/src/` for application-level tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_description() {
        // Arrange: Set up test data
        // Act: Execute the behavior
        // Assert: Verify the expected outcome
        assert!(false, "Test fails until fix is implemented");
    }
}
```

### 3. Implement the Fix
- Make minimal changes to fix the specific issue
- Follow existing code patterns and conventions
- Maintain dependency boundaries (katla_ui must NOT depend on katla_ecs or katla_app)
- Use `pub(crate)` by default, only `pub` when necessary

### 4. Verify the Fix
- Run tests: `cargo test --workspace`
- Run lint: `cargo clippy`
- Run format: `cargo fmt`
- Manual visual check if applicable: `cargo run -- -s`

### 5. Document Changes
- Update any relevant comments (avoid AI slop)
- Ensure code is self-documenting through good naming

## Example Handoff

```json
{
  "salientSummary": "Fixed transparency issue by correcting font atlas texture format from SRGB to UNORM. Added test verifying white pixel sampling returns correct alpha.",
  "whatWasImplemented": "Changed TextureDescriptor::rgba8_srgb() to rgba8_unorm() in create_ui_font_atlas(). Added test_font_atlas_white_pixel() test in katla_gfx/src/renderer.rs verifying atlas[0,0] = [255,255,255,255].",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test --workspace",
        "exitCode": 0,
        "observation": "All 585 tests passed, including new font atlas test"
      },
      {
        "command": "cargo clippy",
        "exitCode": 0,
        "observation": "No warnings"
      },
      {
        "command": "cargo run -- -s",
        "exitCode": 0,
        "observation": "UI panels appear fully opaque, text is crisp"
      }
    ],
    "interactiveChecks": [
      {
        "action": "Visual inspection of debug overlay",
        "observed": "Panel backgrounds are solid (not see-through), buttons have distinct states"
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "katla_gfx/src/renderer.rs",
        "cases": [
          {
            "name": "test_font_atlas_white_pixel",
            "verifies": "VAL-ATLAS-001: White pixel at origin is [255,255,255,255]"
          }
        ]
      }
    ]
  },
  "discoveredIssues": [],
  "fulfillsAssertions": ["VAL-ATLAS-001", "VAL-OPACITY-001"]
}
```

## When to Return to Orchestrator

- Feature depends on an API or data structure that doesn't exist yet
- Requirements are ambiguous or contradictory
- Existing bugs block this feature
- The fix requires architectural changes beyond the feature scope
- You've discovered a deeper issue that needs separate tracking
