# Cross-Area Flow Assertions

**Feature:** Multi-Viewport Compositing  
**Date:** March 2026  
**Scope:** Integration scenarios spanning multiple milestones/areas

This document enumerates ALL testable behavioral assertions for cross-area flows that span multiple implementation phases. Each assertion includes a stable ID, behavioral description with clear pass/fail conditions, and evidence requirements.

---

## End-to-End Rendering Flows

### VAL-CROSS-001: Viewport Creation to Display Pipeline

**Behavioral Description:**
When creating multiple viewports, building a frame graph with compositing, and executing the frame, the final output must correctly display all viewports at their specified screen positions without visual artifacts.

**Pass Condition:**
- Viewport textures are created with correct format and dimensions
- Frame graph compiles without errors
- All viewport passes execute in correct order
- Compositing pass samples from all viewport textures
- Final backbuffer contains correctly positioned viewports
- No black screens, garbage data, or visual corruption

**Fail Conditions:**
- Frame graph compilation fails with dependency errors
- Viewport textures have incorrect format/size
- Compositing pass produces black output
- Viewports appear at wrong screen positions
- Visual artifacts (flickering, tearing, corruption)

**Evidence Requirements:**
1. **Frame captures:** Full Vulkan frame capture showing all passes
2. **Frame graph dump:** Execution plan with pass ordering
3. **Screenshot:** Final rendered output with viewport positions marked
4. **Logs:** Frame graph build and execution logs (INFO level)
5. **Validation output:** Vulkan validation layer output (must be clean)

---

### VAL-CROSS-002: Viewport Resize Flow

**Behavioral Description:**
When one or more viewports are resized, the frame graph must rebuild transient textures with new dimensions, re-register textures with descriptor sets, and continue rendering correctly without memory leaks or crashes.

**Pass Condition:**
- Old viewport textures are properly destroyed
- New viewport textures created with correct dimensions
- Compositing descriptor set updated with new texture views
- Frame graph execution succeeds after resize
- Rendered output shows resized viewports at correct positions
- No memory leaks (heap usage stable over multiple resizes)

**Fail Conditions:**
- Crash or panic during resize
- Memory leak detected (heap grows with each resize)
- Descriptor set not updated (stale texture views)
- Viewport stretches or distorts after resize
- Frame graph compilation fails after resize

**Evidence Requirements:**
1. **Memory profile:** Heap usage before/after resize (10 iterations)
2. **Vulkan validation:** Check for descriptor set corruption warnings
3. **Screenshots:** Before and after resize showing correct viewport sizes
4. **Logs:** Frame graph rebuild logs showing texture recreation
5. **Profiler output:** Memory allocator stats over time

---

### VAL-CROSS-003: Runtime Viewport Addition/Removal

**Behavioral Description:**
When viewports are added or removed at runtime (not just at startup), the frame graph must dynamically adapt its pass structure and descriptor set layout without requiring application restart.

**Pass Condition:**
- Adding viewport: Frame graph rebuilds with new pass
- Removing viewport: Frame graph rebuilds without old pass
- Compositing descriptor set updated with new viewport count
- All remaining viewports continue rendering correctly
- No resource leaks from destroyed viewports

**Fail Conditions:**
- Crash when adding/removing viewports
- Frame graph fails to compile after topology change
- Descriptor set has stale texture views (dangling pointers)
- Rendering breaks for remaining viewports
- Memory leak from destroyed viewport resources

**Evidence Requirements:**
1. **Frame graph dumps:** Before and after viewport topology change
2. **Descriptor set inspection:** Verify correct texture bindings
3. **Screenshots:** Showing correct rendering before/after change
4. **Memory profiler:** No leaks after 10 add/remove cycles
5. **Vulkan validation:** No "object destroyed but still referenced" warnings

---

## Frame Graph Integration Flows

### VAL-CROSS-004: Viewport Pass Integration with Existing Passes

**Behavioral Description:**
When viewport rendering passes are added to the frame graph alongside existing passes (geometry, UI, tonemap), all passes must execute in correct dependency order with proper barrier synchronization.

**Pass Condition:**
- Topological sort produces valid execution order
- Viewport passes execute before compositing pass
- Compositing pass executes before UI pass
- Barriers inserted correctly between all passes
- All passes read from correct transient textures
- No Hazards (read-before-write, write-before-write)

**Fail Conditions:**
- Dependency cycle detected during compilation
- Barriers missing or in wrong order
- Passes read from textures before they're written
- Visual corruption due to synchronization issues
- Frame graph compilation fails

**Evidence Requirements:**
1. **Execution plan:** Topologically sorted pass order
2. **Barrier log:** All barriers inserted with correct src/dst stages
3. **Vulkan synchronization validation:** No hazard warnings
4. **Frame capture:** Showing all pass boundaries and barriers
5. **Resource state tracking:** Verify layout transitions for all textures

---

### VAL-CROSS-005: Barrier Placement Between Viewport and Compositing

**Behavioral Description:**
When viewport passes write to transient textures and the compositing pass reads from them, barriers must be inserted with correct image layout transitions (COLOR_ATTACHMENT_OPTIMAL → SHADER_READ_ONLY_OPTIMAL).

**Pass Condition:**
- Post-pass barrier after each viewport write
- Layout transition: COLOR_ATTACHMENT_OPTIMAL → SHADER_READ_ONLY_OPTIMAL
- Source stage: COLOR_ATTACHMENT_OUTPUT
- Destination stage: FRAGMENT_SHADER
- Compositing pass can read viewport textures without corruption

**Fail Conditions:**
- Missing barrier after viewport pass
- Incorrect layout transition (causes garbage data or hang)
- Wrong stage masks (can cause GPU hang)
- Compositing pass reads corrupted data
- Vulkan validation errors (Hazards)

**Evidence Requirements:**
1. **Barrier inspection:** Log all barriers with layout transitions
2. **Vulkan validation:** Must show NO synchronization errors
3. **Visual test:** Composited output must be clean (no artifacts)
4. **Frame capture:** Show barrier with correct src/dst access masks
5. **Resource state log:** Track each texture's layout through frame

---

### VAL-CROSS-006: Transient Texture Management Across Frames

**Behavioral Description:**
When the frame graph uses double-buffered transient textures for viewports, each frame-in-flight must have its own texture instances to prevent race conditions between CPU and GPU.

**Pass Condition:**
- FRAMES_IN_FLIGHT (typically 2) sets of viewport textures created
- Each frame uses correct texture instance (frame_idx % FRAMES_IN_FLIGHT)
- No CPU write to texture while GPU is reading from previous frame
- No black screens due to texture reuse
- Proper layout tracking per-frame

**Fail Conditions:**
- Single texture instance shared across frames (race condition)
- Black screen or flickering due to concurrent access
- Layout corruption (one frame's layout affects another)
- Texture being written while GPU still reading previous frame

**Evidence Requirements:**
1. **Texture count log:** Verify FRAMES_IN_FLIGHT instances per viewport
2. **Frame indexing log:** Show correct texture instance selected per frame
3. **Vulkan validation:** Check for "texture in use" warnings
4. **Visual inspection:** 100+ frames rendered without flickering
5. **Layout tracking log:** Show per-frame layout state is independent

---

### VAL-CROSS-007: Double-Buffering Race Condition Prevention

**Behavioral Description:**
When using double-buffered transient textures, ensure that CPU-side operations (descriptor set updates, texture writes) never overlap with GPU-side operations (rendering, sampling) for the same texture instance.

**Pass Condition:**
- Fence synchronization prevents CPU/GPU overlap
- CPU waits for frame N to complete before reusing its textures for frame N+FRAMES_IN_FLIGHT
- No "device lost" errors due to memory corruption
- No visual artifacts from corrupted texture data

**Fail Conditions:**
- CPU writes to texture while GPU is still using it
- "Device lost" error due to memory corruption
- Random visual corruption appearing intermittently
- Fence synchronization not working or not checked

**Evidence Requirements:**
1. **Fence wait log:** Verify CPU waits for correct frame
2. **Synchronization validation:** Vulkan validation shows no hazards
3. **Stress test:** Run 10,000 frames, check for corruption
4. **Memory sanitizer:** Run with ASAN/VALGRIND to detect memory corruption
5. **Frame capture:** Show fence synchronization working correctly

---

## Bindless Coexistence Flows

### VAL-CROSS-008: Compositing Descriptor Set Coexistence with Bindless Set

**Behavioral Description:**
When the compositing pass uses its dedicated descriptor set (Set 2) alongside the existing bindless descriptor set (Set 1), both sets must be bound correctly without conflicts or overwriting each other.

**Pass Condition:**
- Set 0: Per-frame data (existing)
- Set 1: Bindless textures (existing)
- Set 2: Compositing viewport textures (new)
- Pipeline layout includes all 3 sets
- All sets bound correctly during compositing pass
- Shader can access both bindless and compositing textures

**Fail Conditions:**
- Pipeline layout doesn't include Set 2
- Set 1 (bindless) not bound during compositing
- Set 2 overwrites Set 1 bindings
- Shader can't access bindless textures during compositing
- Validation error: "descriptor set not bound"

**Evidence Requirements:**
1. **Pipeline layout inspection:** Verify all 3 sets in layout
2. **Bind trace:** Show all 3 sets bound during compositing
3. **Shader disassembly:** Verify Set 1 and Set 2 both accessible
4. **Vulkan validation:** No "descriptor set not bound" errors
5. **Rendering test:** Compositing pass works while bindless still functional

---

### VAL-CROSS-009: Other Frame Graph Passes Still Work Correctly

**Behavioral Description:**
When compositing descriptor set is added, all existing frame graph passes (geometry, shadow, tonemap, UI) must continue working correctly without being affected by the new descriptor set.

**Pass Condition:**
- Geometry pass renders correctly (no broken materials)
- Shadow pass renders depth correctly
- Tonemap pass samples HDR output correctly
- UI pass samples from bindless textures correctly
- All passes use their correct descriptor sets
- No regressions in existing functionality

**Fail Conditions:**
- Geometry pass produces broken output
- Materials not rendering correctly (bindless broken)
- UI can't sample from bindless textures
- Tonemap pass not working
- Any existing functionality regresses

**Evidence Requirements:**
1. **Full frame capture:** Show all passes still executing
2. **Screenshots:** Before/after compositing integration (must be identical)
3. **Descriptor set inspection:** Verify each pass uses correct sets
4. **Bindless texture test:** Verify UI can still sample font atlas
5. **Regression test log:** All existing tests still passing

---

### VAL-CROSS-010: UI Can Still Sample from Bindless Textures

**Behavioral Description:**
When the compositing pass is active, the UI pass must still be able to sample from bindless textures (font atlas, other UI textures) without interference from the compositing descriptor set.

**Pass Condition:**
- UI pass binds Set 0 and Set 1 (bindless)
- UI shader can sample from font atlas texture
- Text renders correctly
- Other UI elements that sample bindless textures work
- No "texture not bound" errors

**Fail Conditions:**
- UI can't sample from font atlas
- Text not rendering or garbled
- "Texture not bound" validation errors
- Compositing descriptor set blocks bindless access

**Evidence Requirements:**
1. **UI render test:** Render text and verify it's legible
2. **Bindless trace:** Verify UI shader samples from bindless set
3. **Screenshot:** UI overlay on top of composited viewports
4. **Vulkan validation:** No descriptor set errors
5. **Font atlas access log:** Confirm font atlas is in bindless set

---

## Error Recovery Flows

### VAL-CROSS-011: Descriptor Set Creation Failure Handling

**Behavioral Description:**
When the compositing descriptor set creation fails (e.g., out of descriptor pool memory), the system must return a clear error instead of crashing or proceeding with invalid state.

**Pass Condition:**
- Descriptor set creation returns `Result::Err` with descriptive error
- Error message indicates root cause (e.g., "descriptor pool exhausted")
- Application can handle error gracefully (show message, exit cleanly)
- No undefined behavior or GPU crash
- No memory corruption

**Fail Conditions:**
- Panic or crash when descriptor set creation fails
- Error message is vague or unhelpful
- Application proceeds with null/invalid descriptor set
- GPU crash or device lost
- Memory corruption

**Evidence Requirements:**
1. **Error injection test:** Force descriptor pool exhaustion, verify error handling
2. **Error message log:** Verify error is descriptive and actionable
3. **Graceful exit test:** Application exits cleanly without crash
4. **Vulkan validation:** Check for "invalid descriptor set" warnings
5. **Memory sanitizer:** No corruption after failed creation

---

### VAL-CROSS-012: Frame Graph Compilation Failure Clear Error

**Behavioral Description:**
When the frame graph compilation fails (e.g., dependency cycle, missing resource), the error message must clearly indicate which passes/resources are problematic and how to fix it.

**Pass Condition:**
- Compilation returns `Result::Err` with specific error type
- Error message includes: pass names, resource names, dependency chain
- Error message suggests fix (e.g., "viewport_0 not declared as read/write")
- No stack trace or cryptic internal errors
- Developer can quickly identify and fix the issue

**Fail Conditions:**
- Generic error like "compilation failed"
- No indication of which pass/resource is problematic
- Stack trace dumped to user
- Internal implementation details leaked
- Developer needs to read frame graph code to debug

**Evidence Requirements:**
1. **Error message examples:** Capture various failure modes
2. **Usability test:** Developer can diagnose issue from error alone
3. **Error type coverage:** Different errors for different failure modes
4. **Fix validation:** Suggested fix actually resolves the issue
5. **Documentation:** Error messages documented with examples

---

### VAL-CROSS-013: Viewport Texture Validation Catch

**Behavioral Description:**
When a viewport texture has invalid parameters (e.g., format not supported for sampling, dimensions too large/small), the validation layer or API must catch this before GPU execution and provide a clear error.

**Pass Condition:**
- Texture creation validates format (must be sampleable)
- Dimensions validated (within device limits)
- Validation layer warns if format incompatible with compositing
- API returns error before GPU execution
- Clear error message with valid format suggestions

**Fail Conditions:**
- Invalid texture created successfully
- GPU crash or hang when sampling invalid texture
- Validation layer doesn't catch the issue
- Vague error like "texture creation failed"
- No indication of what parameter is invalid

**Evidence Requirements:**
1. **Validation test:** Create texture with invalid format, verify error
2. **Vulkan validation output:** Check for format compatibility warnings
3. **Error message log:** Verify error indicates which parameter is invalid
4. **Format compatibility table:** Document which formats work
5. **Device limits check:** Verify dimensions within limits

---

## Real-World Scenario Flows

### VAL-CROSS-014: Split-Screen Game (2 Viewports, Different Cameras)

**Behavioral Description:**
When implementing a split-screen game with 2 viewports showing different perspectives of the same scene, each viewport must render from its own camera and the composited output must show both views side-by-side without artifacts.

**Pass Condition:**
- 2 viewport passes, each with different camera transform
- Viewport 0 renders from player 1's perspective
- Viewport 1 renders from player 2's perspective
- Compositing pass positions them side-by-side (e.g., left/right)
- Both viewports show correct scene from respective cameras
- No visual artifacts at viewport boundary

**Fail Conditions:**
- Both viewports show same camera (camera state not isolated)
- Viewports overlap or have gaps between them
- Artifacts at boundary (tearing, misalignment)
- One viewport not rendering
- Cameras affect each other

**Evidence Requirements:**
1. **Screenshot:** Split-screen output with viewport boundary marked
2. **Camera transform log:** Verify each viewport uses different camera
3. **Frame graph dump:** Show 2 separate viewport passes
4. **Viewport rectangles:** Log showing correct side-by-side positioning
5. **Visual inspection:** Both perspectives correct (e.g., different positions in scene)

---

### VAL-CROSS-015: Editor Layout (3 Viewports, Same Scene)

**Behavioral Description:**
When implementing a 3D editor with 3 viewports (perspective, top, side) showing the same scene, all three viewports must render simultaneously and the compositing pass must position them in a grid layout (e.g., 2 top, 1 bottom).

**Pass Condition:**
- 3 viewport passes: perspective, top orthographic, side orthographic
- All 3 render the same scene geometry
- Each viewport has different camera transform and projection
- Compositing pass positions them in 2x2 grid (one slot empty)
- All viewports show correct scene from respective views
- Performance acceptable (3x rendering cost)

**Fail Conditions:**
- Viewports show wrong camera/projection
- Grid layout incorrect (overlapping, misaligned)
- Performance degradation (3x slower than expected)
- One or more viewports not rendering
- Scene state not shared correctly between viewports

**Evidence Requirements:**
1. **Screenshot:** 3-viewport editor layout with labels
2. **Camera/projection log:** Verify each viewport has correct transform
3. **Performance profile:** Frame time with 3 viewports (should be ~3x single viewport)
4. **Frame graph dump:** Show 3 viewport passes + compositing
5. **Viewport rectangles:** Log showing correct grid positioning

---

### VAL-CROSS-016: Picture-in-Picture Debug Overlay

**Behavioral Description:**
When implementing a picture-in-picture debug overlay (e.g., small viewport showing rear view while main viewport shows forward view), the small viewport must be composited on top of the main viewport with transparency support.

**Pass Condition:**
- 2 viewport passes: main (fullscreen), PiP (small corner)
- Compositing pass layers them: main first, PiP on top
- PiP viewport supports transparency (alpha channel)
- PiP viewport correctly positioned in corner (e.g., top-right)
- Main viewport visible behind transparent PiP areas
- Visual quality maintained (no scaling artifacts)

**Fail Conditions:**
- PiP viewport not visible (covered by main or not rendered)
- PiP viewport not transparent (opaque black box)
- PiP viewport positioned incorrectly (centered, off-screen)
- Main viewport not visible through PiP transparency
- Scaling artifacts (blurry, pixelated)

**Evidence Requirements:**
1. **Screenshot:** PiP overlay on main viewport with transparency visible
2. **Alpha blending test:** Verify PiP has alpha channel and compositing respects it
3. **Viewport rectangles:** Log showing PiP positioned in corner
4. **Compositing order:** Verify main rendered first, PiP second
5. **Visual quality:** Check scaling quality (nearest vs linear interpolation)

---

## Summary

**Total Assertions:** 16 cross-area flow assertions

**Coverage:**
- End-to-End Rendering: 3 assertions (VAL-CROSS-001 to VAL-CROSS-003)
- Frame Graph Integration: 4 assertions (VAL-CROSS-004 to VAL-CROSS-007)
- Bindless Coexistence: 3 assertions (VAL-CROSS-008 to VAL-CROSS-010)
- Error Recovery: 3 assertions (VAL-CROSS-011 to VAL-CROSS-013)
- Real-World Scenarios: 3 assertions (VAL-CROSS-014 to VAL-CROSS-016)

**Integration Points Tested:**
1. Viewport system ↔ Frame graph
2. Compositing pass ↔ Existing passes (geometry, UI, tonemap)
3. Compositing descriptor set ↔ Bindless descriptor set
4. Descriptor set management ↔ Error handling
5. Double-buffering ↔ Synchronization
6. Viewport resize ↔ Frame graph rebuild
7. Runtime topology changes ↔ Dynamic adaptation

**Evidence Types:**
- Frame captures (Vulkan, RenderDoc)
- Screenshots (visual verification)
- Logs (frame graph, barriers, resources)
- Validation output (Vulkan validation layers)
- Performance profiles (timing, memory)
- Stress tests (long-running stability)

These assertions comprehensively validate that the multi-viewport compositing feature integrates correctly with all existing systems and handles real-world usage scenarios.
