# User Testing

Testing surface: tools, URLs, setup steps, and known quirks for manual validation.

**What belongs here:** How to test the application manually, testing tools, known issues.

## Application Type

**Native Vulkan Desktop Application** - This is a graphics engine with real-time rendering, NOT a web application or API service. User testing validation requires running the application and verifying visual output.

## Running the Application

```bash
# Standard run (interactive)
cargo run

# Limited-frame mode (25 frames, for validation testing)
cargo run -- -s

# Run tests
cargo test --workspace

# Build and typecheck
cargo build
cargo check
```

## Testing Approach for Visual Assertions

For milestone "rendering-fixes", the assertions require visual verification of:

1. **Opacity/Transparency** (VAL-OPACITY-001, VAL-OPACITY-002)
   - UI panels must be fully opaque when alpha=1.0
   - Text must render with correct colors
   - No background bleeding through solid elements

2. **Positioning/Scissor** (VAL-POS-002, FLOW-002)
   - Scissor rectangles must clip correctly at any DPI scale
   - Content outside clip region must not be visible
   - Content inside clip region must be fully visible

3. **Font Atlas** (VAL-ATLAS-001)
   - White pixel must exist at atlas origin (0, 0)
   - Used for solid color rendering

### Testing Methodology

**Primary Method:** Run application in limited-frame mode and inspect visual output
- Application runs for 25 frames then exits
- Vulkan validation layers check for rendering errors
- Visual inspection of UI elements during execution

**Secondary Method:** Automated unit tests
- Unit tests verify data structures and calculations
- Headless rendering tests validate Vulkan operations
- Pixel inspection tests verify rendering output

## Flow Validator Guidance: Native Desktop UI

### Isolation Strategy
- **No user accounts or authentication** - native desktop app
- **No database** - all state is in-memory
- **No shared network resources** - standalone application
- **Parallel testing**: Safe to run multiple instances with different configurations

### Testing Constraints
- Each test run is independent (no shared state between runs)
- Application uses in-memory ECS and rendering state
- No file I/O conflicts (each run operates independently)
- Vulkan context is created per application instance

### Validation Points
For each assertion, verify:
1. **Visual correctness** - Does it look right?
2. **Vulkan validation** - No validation layer errors
3. **Data correctness** - Vertex/texture data is correct
4. **Consistency** - Behavior is consistent across runs

## Visual Testing Checklist

When running `cargo run -- -s`, verify:

1. **Debug Overlay** (top-left):
   - FPS counter visible
   - Stats panel is opaque (not see-through)
   - Graph renders correctly

2. **Settings Panel** (if visible):
   - Sliders work correctly
   - Checkboxes toggle
   - Buttons respond to hover/click

3. **Text Rendering**:
   - Text is crisp and readable
   - Colors match expected values
   - No artifacts or blurring

4. **Panel Opacity** (VAL-OPACITY-001):
   - Panels with alpha=1.0 are fully opaque
   - No background content visible through solid panels
   - Text is readable and properly colored

5. **Clipping/Scissor** (VAL-POS-002):
   - Content is clipped correctly at panel boundaries
   - No content bleeds outside its container
   - Nested clipping works correctly

## Known Issues

- Scissor coordinates may be incorrect on HiDPI displays (scale_factor != 1.0)
- All UI elements may appear semi-transparent (under investigation)

## Testing Tools

- **Vulkan validation layers**: Enabled by default in debug builds
- **Headless context**: Available via `create_headless_context()` for automated tests
- **Limited-frame mode**: `cargo run -- -s` runs 25 frames for validation
- **Unit tests**: `cargo test --workspace` runs all tests

## Assertion-Specific Testing

### VAL-OPACITY-001: Opaque Panel Rendering
**Test**: Run `cargo run -- -s` and observe panel opacity
**Expected**: Panels with alpha=1.0 are fully opaque, no background visible
**Validation**: Visual inspection + Vulkan validation (no blending errors)

### VAL-OPACITY-002: Text Color Correctness
**Test**: Observe text rendering in debug overlay and panels
**Expected**: Text colors match specified values, alpha blends correctly
**Validation**: Visual inspection + color sampling

### VAL-POS-002: HiDPI Scissor/Clip Coordinates
**Test**: Test with different scale factors (1.0, 1.5, 2.0)
**Expected**: Content clipped correctly at all scales
**Validation**: Visual inspection + unit tests for coordinate scaling

### FLOW-002: Clipped Content Rendering
**Test**: Verify clipping in nested panels and scrollable areas
**Expected**: Content outside clip region not visible, inside fully visible
**Validation**: Visual inspection + draw list inspection

### VAL-ATLAS-001: White Pixel at Origin
**Test**: Verify font atlas initialization
**Expected**: First 2x2 pixels are white (255, 255, 255, 255)
**Validation**: Unit test in katla_ui tests
