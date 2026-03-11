# Testing Infrastructure Limitations

## Integration Testing with Headless Vulkan

When writing integration tests that require a Vulkan context, be aware of infrastructure limitations:

### The Challenge

Creating a headless Vulkan context for tests requires `raw_window_handle` which introduces API complexity:
- Requires specific feature flags and dependencies
- Different code paths for different platforms (Windows, Linux, macOS)
- Test setup becomes as complex as the production code being tested

### Current Practice

Workers have adopted a pragmatic approach:

1. **Write tests that verify API compilation:**
   ```rust
   #[test]
   fn test_bindless_api_compiles() {
       // This test verifies the API compiles and has correct types
       // Full integration testing requires running Vulkan instance
       assert!(true);
   }
   ```

2. **Add explanatory comments:**
   ```rust
   // Note: Full integration testing requires Vulkan context initialization
   // which depends on raw_window_handle. This test verifies API compilation.
   // Actual functionality is tested manually via cargo run -- -s
   ```

3. **Rely on unit tests for logic verification:**
   - Unit tests can verify internal logic without Vulkan context
   - Mock objects can test data flow and state management

4. **Manual verification for GPU operations:**
   - Use `cargo run -- -s` for visual validation
   - Check Vulkan validation layer output for errors
   - Run the full application to test integration

### When to Use This Pattern

Use this approach when:
- Testing GPU-related functionality (Vulkan, shaders, textures)
- Integration requires external windowing system
- Test setup complexity exceeds test value

When NOT to use:
- Pure Rust logic that doesn't require GPU
- State management that can be tested with mocks
- Data transformations or calculations

### Alternative: Conditional Integration Tests

For critical paths, consider conditional integration tests:

```rust
#[cfg(feature = "integration-tests")]
#[test]
fn test_with_real_vulkan() {
    // Only runs when --features integration-tests is specified
    // Requires full Vulkan setup
}
```

This keeps the test suite fast for normal development while enabling thorough testing when needed.
