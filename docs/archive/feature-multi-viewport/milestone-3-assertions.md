# Milestone 3: Cleanup & Migration - Behavioral Assertions

## Code Removal Assertions

### VAL-CLEAN-001: Old Viewport Rendering Functions Removed
**Behavioral description:** The renderer must no longer expose functions for direct viewport rendering outside the frame graph system. Functions such as `render_viewport()` and `viewport_texture()` in `VulkanRenderer` must be completely removed from the codebase.

**Pass condition:** Code search for these functions returns zero results in `katla_gfx/src/renderer.rs` and related files.

**Evidence requirements:**
- Grep results showing zero matches for removed function signatures
- Git diff showing deletion of old rendering functions
- Compilation succeeds without errors

---

### VAL-CLEAN-002: No Hybrid Rendering Approaches
**Behavioral description:** The codebase must not contain both old and new viewport rendering code paths coexisting. All viewport rendering must route through the frame graph system exclusively.

**Pass condition:** No functions or modules exist for manual viewport rendering outside `render_graph/`.

**Evidence requirements:**
- Grep for `render_viewport`, `begin_viewport`, `end_viewport` patterns returns zero results
- All viewport rendering code exists only within `render_graph/` directory
- No conditional compilation or feature gates for old vs. new rendering

---

### VAL-CLEAN-003: Viewport Struct Refactored to Configuration-Only
**Behavioral description:** The `Viewport` struct must contain only configuration data (label, extent, clear_color, output_mode) and must not contain rendering state (render_target, storage_manager, draw_list, frame_uniforms).

**Pass condition:** `Viewport` struct definition in `katla_gfx/src/viewport.rs` contains only configuration fields.

**Evidence requirements:**
- Struct definition shows only configuration fields
- Grep for removed fields (`render_target`, `storage_manager`, `draw_list`, `frame_uniforms`) returns zero results in Viewport
- Compilation succeeds with new struct definition

---

### VAL-CLEAN-004: Render State Moved to Frame Graph
**Behavioral description:** All viewport rendering state previously stored in `Viewport` (textures, uniforms, draw lists) must now be managed by the frame graph system through transient resources and pass data.

**Pass condition:** Viewport rendering state exists only in `render_graph/` modules, not in `Viewport` struct.

**Evidence requirements:**
- Grep for `transient_texture`, `pass_data` in `render_graph/` returns results
- Grep for these patterns in `Viewport` returns zero results
- Frame graph pass execution correctly accesses viewport state

---

### VAL-CLEAN-005: No Dead Code Remaining
**Behavioral description:** All unused viewport rendering code, helper functions, and intermediate structures must be removed. No functions should remain unimplemented or commented out.

**Pass condition:** Compiler reports zero dead code warnings; no `TODO` or `unimplemented!()` macros exist in viewport-related code.

**Evidence requirements:**
- Full compilation log shows zero dead code warnings (`cargo clippy -- -W dead_code`)
- Grep for `TODO`, `FIXME`, `unimplemented!()` in viewport-related files returns zero results
- Code coverage analysis shows all viewport functions are used

---

## API Migration Assertions

### VAL-CLEAN-006: Old Viewport Rendering API Removed
**Behavioral description:** Public API functions for direct viewport rendering (`VulkanRenderer::render_viewport`, `VulkanRenderer::viewport_texture`) must not exist.

**Pass condition:** These functions are not callable from external crates; API documentation does not reference them.

**Evidence requirements:**
- `cargo doc --open` does not show removed functions
- Attempting to call these functions results in compilation error
- Public API surface (`pub fn`) in renderer contains only viewport creation/management functions

---

### VAL-CLEAN-007: New Frame Graph API Works Correctly
**Behavioral description:** The new frame graph-based viewport rendering API must successfully create viewport passes, execute rendering, and produce correct output without errors.

**Pass condition:** Integration tests demonstrate successful multi-viewport rendering using frame graph API.

**Evidence requirements:**
- Test output showing successful frame graph creation and execution
- Screenshot comparison showing correct viewport rendering
- No Vulkan validation errors during execution
- Test log: `cargo test test_frame_graph_multi_viewport -- --nocapture`

---

### VAL-CLEAN-008: All Existing Viewport Usage Migrated
**Behavioral description:** All existing code that created and rendered viewports using the old API must be updated to use the frame graph approach.

**Pass condition:** Codebase contains zero references to old viewport rendering API; all viewport rendering uses frame graph passes.

**Evidence requirements:**
- Grep for old API usage patterns returns zero results
- All viewport creation sites use `ViewportBuilder` → frame graph pattern
- Integration tests cover all migrated use cases
- Git diff shows all call sites updated

---

### VAL-CLEAN-009: No Breaking Changes to Public Viewport Creation API
**Behavioral description:** Viewport creation via `ViewportBuilder` must remain unchanged and backward compatible. Only the rendering API changes.

**Pass condition:** Existing viewport creation code compiles without modifications; `ViewportBuilder` API surface unchanged.

**Evidence requirements:**
- Viewport creation tests pass without modification
- Public API documentation for `ViewportBuilder` unchanged
- Integration test demonstrating unchanged viewport creation flow

---

### VAL-CLEAN-010: Public API Surface Consistency
**Behavioral description:** All public viewport-related APIs must follow consistent patterns. No hybrid or transitional APIs should be exposed.

**Pass condition:** Public API contains only creation (`ViewportBuilder`) and management (`ViewportManager`) functions; no direct rendering functions.

**Evidence requirements:**
- `cargo doc --open --no-deps` shows clean, consistent API
- No deprecated or transitional APIs documented
- API naming conventions consistent (no `_old`, `_v2` suffixes)

---

## Viewport Manager Assertions

### VAL-CLEAN-011: Viewport Manager Builds Frame Graph Passes
**Behavioral description:** `ViewportManager` must provide a `build_viewport_passes()` method that creates frame graph passes for specified viewports and returns texture resource names.

**Pass condition:** Method exists, accepts viewport handles, returns pass names/textures for compositing.

**Evidence requirements:**
- Function signature: `pub fn build_viewport_passes(&self, viewport_handles: &[ViewportHandle], graph: &mut FrameGraphBuilder) -> Result<Vec<String>, RenderGraphError>`
- Unit test demonstrates successful pass building
- Integration test shows correct texture name returns

---

### VAL-CLEAN-012: Viewport Lifecycle Management Unchanged
**Behavioral description:** Viewport creation (`create_viewport`), lookup (`viewport`), and destruction (`destroy_viewport`) must continue to work as before.

**Pass condition:** Existing viewport lifecycle tests pass; no breaking changes to management API.

**Evidence requirements:**
- All viewport lifecycle unit tests pass
- Integration test shows create → use → destroy flow works
- No memory leaks (use `valgrind` or similar if available)

---

### VAL-CLEAN-013: Viewport Lookup Unchanged
**Behavioral description:** Looking up viewports by handle must return correct viewport configuration and work identically to before.

**Pass condition:** Viewport lookup returns correct data; handle resolution works correctly.

**Evidence requirements:**
- Unit test for viewport lookup passes
- Test verifies correct viewport data returned
- No handle collision or invalidation issues

---

### VAL-CLEAN-014: Viewport Destruction Unchanged
**Behavioral description:** Destroying viewports must correctly clean up resources without leaks or crashes, even after migration to frame graph.

**Pass condition:** Viewport destruction tests pass; no Vulkan validation layer warnings about leaked resources.

**Evidence requirements:**
- Unit test for viewport destruction passes
- Vulkan validation layers report zero object leaks
- Repeated create/destroy cycle does not leak memory

---

## Build & Tests Assertions

### VAL-CLEAN-015: All Tests Pass After Migration
**Behavioral description:** Every existing test in the workspace must pass after migration to frame graph approach.

**Pass condition:** `cargo test --workspace` returns with exit code 0 and zero failed tests.

**Evidence requirements:**
- Full test output log: `cargo test --workspace 2>&1 | tee test-results.log`
- All test suites show `passed` status
- No tests skipped or ignored

---

### VAL-CLEAN-016: No Compilation Errors
**Behavioral description:** The entire workspace must compile without errors after migration.

**Pass condition:** `cargo build --workspace` completes successfully with zero compilation errors.

**Evidence requirements:**
- Full compilation log: `cargo build --workspace 2>&1 | tee build.log`
- Exit code 0
- No `error:` messages in output

---

### VAL-CLEAN-017: No Compilation Warnings
**Behavioral description:** The entire workspace must compile without warnings after migration.

**Pass condition:** `cargo build --workspace` produces zero compiler warnings.

**Evidence requirements:**
- Clippy output: `cargo clippy --workspace -- -D warnings`
- Zero warnings reported
- No dead code, unused variables, or deprecated feature warnings

---

### VAL-CLEAN-018: Documentation Updated
**Behavioral description:** All documentation references to old viewport rendering API must be updated to reflect frame graph approach.

**Pass condition:** No documentation mentions old rendering functions; examples use new API.

**Evidence requirements:**
- Grep for removed function names in `.md` files returns zero results
- Code examples in documentation compile and run
- API documentation (`cargo doc`) describes new approach only

---

## Regression Prevention Assertions

### VAL-CLEAN-019: Single-Viewport Rendering Works
**Behavioral description:** Existing single-viewport rendering (the default use case) must continue to work correctly with no visual regressions.

**Pass condition:** Single-viewport test produces identical output to pre-migration baseline.

**Evidence requirements:**
- Screenshot comparison test shows zero pixel difference
- Performance metrics (frame time) within 5% of baseline
- No visual artifacts or rendering errors

---

### VAL-CLEAN-020: No Performance Regression
**Behavioral description:** Frame graph-based viewport rendering must not significantly degrade performance compared to direct rendering.

**Pass condition:** Frame time within 10% of baseline for equivalent workload.

**Evidence requirements:**
- Benchmark results: `cargo bench --bench viewport_rendering`
- Frame time graph showing no significant increase
- Profile data showing no hotspots introduced

---

### VAL-CLEAN-021: Vulkan Validation Layers Pass Clean
**Behavioral description:** All rendering operations must pass Vulkan validation layer checks without errors or warnings.

**Pass condition:** Validation layers report zero errors or warnings during multi-viewport rendering.

**Evidence requirements:**
- Validation layer output log: `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation cargo run -- -s`
- Zero `ERROR` or `WARNING` messages from validation
- No synchronization or memory access violations reported

---

### VAL-CLEAN-022: No Memory Leaks
**Behavioral description:** Frame graph and viewport systems must correctly manage Vulkan memory and handle lifecycles without leaks.

**Pass condition:** Extended run (1000+ frames) shows stable memory usage; validation reports zero leaked objects.

**Evidence requirements:**
- Memory usage profile over time
- Validation layer report at exit shows zero leaked objects
- No increasing memory trend during long run

---

### VAL-CLEAN-023: No Synchronization Issues
**Behavioral description:** Frame graph must correctly insert barriers between viewport passes and compositing to prevent data races.

**Pass condition:** Vulkan validation reports zero synchronization issues; rendering is race-free.

**Evidence requirements:**
- Validation layer output shows zero synchronization warnings
- Frame graph barrier insertion logs show correct barrier placement
- Stress test with rapid viewport creation/destruction passes

---

### VAL-CLEAN-024: Backward Compatibility for Viewport Configuration
**Behavioral description:** Viewport configuration (size, clear color, output mode) must work identically to before.

**Pass condition:** Viewport configuration tests pass; visual output matches expectations.

**Evidence requirements:**
- Unit tests for all viewport configuration options pass
- Integration test demonstrates all viewport modes work
- No behavioral changes in viewport creation or properties

---

## Summary

**Total Assertions:** 24
- **Code Removal:** 5 assertions (VAL-CLEAN-001 to VAL-CLEAN-005)
- **API Migration:** 5 assertions (VAL-CLEAN-006 to VAL-CLEAN-010)
- **Viewport Manager:** 4 assertions (VAL-CLEAN-011 to VAL-CLEAN-014)
- **Build & Tests:** 4 assertions (VAL-CLEAN-015 to VAL-CLEAN-018)
- **Regression Prevention:** 6 assertions (VAL-CLEAN-019 to VAL-CLEAN-024)

**Evidence Collection Strategy:**
1. Automated tests for unit assertions
2. Compilation logs for build assertions
3. Screenshot comparison for visual assertions
4. Vulkan validation output for correctness assertions
5. Performance benchmarks for regression assertions
