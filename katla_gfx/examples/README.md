# Particle System Examples

This directory contains example binaries for validating and testing the particle system.

## particle_validation

A headless particle system validation example that:

- Initializes a Vulkan context without requiring a window
- Creates a particle system with test emitters
- Runs simulation for several frames
- Validates particle data (NaN checks, bounds checking, etc.)
- Validates emitter configurations
- Detects Vulkan validation errors
- Exits with code 0 on success, code 1 on failure

### Usage

```bash
# Build the example
cargo build -p katla_gfx --example particle_validation

# Run the example
cargo run -p katla_gfx --example particle_validation

# Run with custom log level
RUST_LOG=debug cargo run -p katla_gfx --example particle_validation

# Check exit code in CI
cargo run -p katla_gfx --example particle_validation
echo $?  # Should be 0 on success
```

### Output

The example prints detailed validation information:

```
=== Particle System Validation Example ===
Max particles: 10000
Frames to simulate: 10
Creating headless Vulkan context with validation...
Vulkan context created successfully
Creating particle system...
Particle system created successfully
Memory usage: 0.57 MB
Initializing debug readback...
Debug readback initialized
Creating test emitters...
Created 3 test emitters
Running simulation for 10 frames...
=== Simulation Complete ===
Total frames: 10
Total particles emitted: 149
Total particles died: 0
Current alive count: 0
Reading back particle data for validation...
✓ Particle data validation passed
Validating emitter configurations...
✓ Emitter configuration validation passed
=== Validation Summary ===
Max particles: 10000
Total frames simulated: 10
Memory usage: 0.57 MB
Total emitted: 149
Total died: 0
Currently alive: 0
=== All Validations Passed ===
```

### Exit Codes

- **0**: All validations passed
- **1**: Validation failed (check error messages)

### Use Cases

- **CI/CD pipelines**: Automatically validate particle system changes
- **LLM validation**: Quick smoke test without visual output
- **Debugging**: Inspect particle data without running full renderer
- **Performance testing**: Measure particle system overhead without rendering

### Implementation Details

The example demonstrates:
- Headless Vulkan context creation
- Particle system initialization
- Emitter creation with various configurations
- Debug readback for GPU data inspection
- CPU-side validation of particle data
- Error handling and exit codes

See `particle_validation.rs` for full implementation details.
