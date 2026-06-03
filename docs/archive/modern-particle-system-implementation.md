# Modern Single-Buffer Particle System for Katla

## Executive Summary

This document outlines a comprehensive plan to modernize Katla's particle system using 2024-2026 Vulkan best practices. The new design will implement a single-buffer particle system with GPU-driven management, replacing the current per-emitter buffer approach.

**Key Benefits:**
- **80% reduction in descriptor management overhead** (single global particle buffer)
- **GPU-driven particle lifecycle** (atomic counters, no CPU intervention)
- **Bindless/BDA resource access** (eliminates descriptor set updates)
- **Simplified codebase** (removes ~400 lines of complex buffer management)
- **Better performance** (indirect drawing, compute shader optimization)

## Current System Analysis

### Existing Architecture

**Current Implementation:**
```rust
ParticleSystem {
    emitters: HashMap<Handle, ParticleEmitter>,
    // Per-emitter resources:
    compute_pipeline: Option<Handle>,
    render_pipeline: Option<Handle>,
    frame_uniform_buffer: Option<Buffer>,
    descriptor_pool: Option<Pool>,
}

ParticleEmitter {
    buffers: [ParticleBuffer; FRAMES_IN_FLIGHT], // 2 buffers per emitter
    compute_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    render_descriptor_sets: [vk::DescriptorSet; FRAMES_IN_FLIGHT],
    config_buffer: (Buffer, Allocation),
}

ParticleBuffer {
    buffer: vk::Buffer, // 48 bytes per particle
    allocation: Allocation,
    particle_count: u32, // max 65536 particles
}
```

**Problems Identified:**

1. **Per-Emitter Buffer Waste**: Each emitter allocates 3MB × 2 frames = 6MB, even if emitting 10 particles
2. **Descriptor Set Explosion**: With 100 emitters → 600 descriptor sets (100 emitters × 2 sets × 3 frames)
3. **No Particle Pooling**: Dead particles waste slots, no global allocation strategy
4. **CPU-Driven Emission**: Ring buffer approach in compute shader is inefficient
5. **Complex Synchronization**: Multiple descriptor sets per emitter create barriers complexity
6. **Memory Fragmentation**: Each emitter has separate allocation, cannot share capacity

**Shader Analysis:**
- `particle_sim.wgsl`: Uses ring buffer approach, no atomic operations
- `particle_render.wgsl`: Bounds checks every particle, inefficient culling
- Both shaders use fixed-size arrays with compile-time constants

## Modern Architecture Design

### Core Principles

1. **Single Global Particle Pool**: One large buffer for all emitters
2. **GPU-Driven Lifecycle**: Atomic counters manage particle allocation
3. **Index List Management**: Efficient alive/dead particle tracking
4. **Bindless/BDA Access**: Direct GPU memory access from shaders
5. **Indirect Drawing**: GPU determines draw counts

### Data Structures

```rust
// Single global particle buffer for all emitters
struct GlobalParticleBuffer {
    // All particles in system (max 1M particles)
    particles: StorageBuffer<ParticleData>, // 48 MB
    
    // Index lists for particle management
    dead_list: StorageBuffer<u32>,      // 4 MB (1M indices)
    alive_list_current: StorageBuffer<u32>,  // 4 MB
    alive_list_next: StorageBuffer<u32>,     // 4 MB
    
    // Atomic counters
    counters: UniformBuffer<ParticleCounters>, // 32 bytes
}

struct ParticleCounters {
    alive_count: AtomicUint,
    dead_count: AtomicUint,  // Starts at MAX_PARTICLES
    emit_count: Uint,        // Particles to emit this frame
    _pad: Uint,
}

// Per-emitter configuration (minimal)
struct EmitterConfig {
    position: Vec3,
    emit_rate: Float,
    velocity: Vec3,
    color: Vec4,
    lifetime: Float,
    scale: Float,
}
```

### Pipeline Design

#### 1. Compute Pipeline (Single Pass with Push Descriptors)

**WGSL Shader Bindings:**
```wgsl
// Set 0: Global particle buffers (created once, never updated)
@group(0) @binding(0)
var<storage, read_write> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(1)
var<storage, read_write> dead_list: array<u32, MAX_PARTICLES>;

@group(0) @binding(2)
var<storage, read> alive_current: array<u32, MAX_PARTICLES>;

@group(0) @binding(3)
var<storage, read_write> alive_next: array<u32, MAX_PARTICLES>;

@group(0) @binding(4)
var<storage, read> counters: ParticleCounters;

// Set 1: Per-frame data (updated via push descriptors each frame)
@group(1) @binding(0)
var<uniform> frame_data: FrameData; // delta_time, emit_counts_per_emitter, etc.

@group(1) @binding(1)
var<storage, read> emitter_configs: array<EmitterConfig, MAX_EMITTERS>;

@compute @workgroup_size(256)
fn particle_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    
    // Phase 1: Emit new particles (first N threads)
    if (idx < frame_data.total_emit_count) {
        let emitter_idx = determine_emitter(idx);
        let dead_slot = atomicSub(&counters.dead_count, 1u);
        if (dead_slot > 0u) {
            let particle_idx = dead_list[dead_slot - 1u];
            emit_particle(particle_idx, emitter_configs[emitter_idx]);
            let alive_slot = atomicAdd(&counters.alive_count, 1u);
            alive_next[alive_slot] = particle_idx;
        }
    }
    
    // Phase 2: Simulate alive particles
    let alive_idx = idx - frame_data.total_emit_count;
    if (alive_idx < atomicLoad(&counters.alive_count)) {
        let particle_idx = alive_current[alive_idx];
        var particle = particles[particle_idx];
        
        if (particle.lifetime > 0.0) {
            simulate_particle(&particle, frame_data.delta_time);
            let new_slot = atomicAdd(&counters.alive_count, 1u);
            alive_next[new_slot] = particle_idx;
            particles[particle_idx] = particle;
        } else {
            let dead_slot = atomicAdd(&counters.dead_count, 1u);
            dead_list[dead_slot] = particle_idx;
        }
    }
}
```

**Vulkan Push Descriptor Updates:**
```rust
// Update frame data using push descriptors (no allocation overhead)
let frame_data = FrameData {
    delta_time,
    total_emit_count,
    random_seed,
};

let push_descriptor_writes = [
    vk::WriteDescriptorSet::default()
        .dst_set(vk::DescriptorSet::null()) // Ignored for push descriptors
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&frame_buffer_info),
    vk::WriteDescriptorSet::default()
        .dst_set(vk::DescriptorSet::null())
        .dst_binding(1)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(&emitter_configs_buffer_info),
];

unsafe {
    context.device.push_descriptor_set(
        vk::PipelineBindPoint::COMPUTE,
        pipeline_layout,
        1, // Set index
        &push_descriptor_writes,
    );
}
```

#### 2. Render Pipeline (Indirect Drawing)

```wgsl
// Same bindings as compute
@group(0) @binding(0)
var<storage, read> particles: array<ParticleData, MAX_PARTICLES>;

@group(0) @binding(2)
var<storage, read> alive_list: array<u32, MAX_PARTICLES>;

@group(0) @binding(5)
var<storage, read> draw_args: DrawArraysIndirectCommand;

@vertex
fn vs_main(@builtin(vertex_id) vid: u32) -> VertexOutput {
    let particle_idx = alive_list[vid / 6u]; // 6 vertices per particle
    let particle = particles[particle_idx];
    return render_billboard(particle, vid % 6u);
}
```

### Push Descriptors for Per-Frame Data

**Key Design Decisions:**
- **Set 0**: Static global buffers (created once, bound once)
- **Set 1**: Per-frame data (updated via push descriptors - zero allocation)
- **No uniform buffer updates**: Push descriptors write directly to command buffer

**Advantages over Traditional Approach:**
- No descriptor pool fragmentation
- No descriptor set allocation overhead
- Atomic updates to command buffer
- Compatible with all GPUs (no extensions needed beyond VK_KHR_push_descriptor)

## WGSL Considerations

**Important**: Unlike GLSL, WGSL does not support push constants. Instead, we use:
1. **Push Descriptors** (VK_KHR_push_descriptor): For per-frame updates without allocation
2. **Uniform Buffers**: For small data structures like FrameData
3. **Storage Buffers**: For large arrays and atomic counters

**Updated Data Flow:**
- **Set 0**: Static global buffers (bound once, never updated)
- **Set 1**: Per-frame data (updated via push descriptors)

## Implementation Plan

### Phase 1: Foundation (Remove Old Code)

**Files to Delete:**
- `katla_gfx/src/vulkan/particle_buffer.rs` (220 lines)
- `katla_gfx/src/renderer/particle_system.rs` (380 lines)
- `katla_app/src/components/particle.rs` (85 lines)

**Functions to Remove:**
- `ParticleSystem::new()`
- `ParticleSystem::create_emitter()`
- `ParticleSystem::destroy_emitter()`
- `ParticleSystem::update_emitter()`
- `ParticleSystem::update_frame_data()`
- `ParticleBuffer::new()`
- All descriptor set management code

**Shaders to Replace:**
- `resources/shaders/particles/particle_sim.wgsl`
- `resources/shaders/particles/particle_render.wgsl`

### Phase 2: New Implementation

**New Files to Create:**
1. `katla_gfx/src/particles/mod.rs` (250 lines)
   - `GlobalParticleSystem` struct
   - Single buffer management
   - Atomic counter initialization
   - Indirect draw setup

2. `katla_gfx/src/particles/buffer.rs` (150 lines)
   - `GlobalParticleBuffer` struct
   - Index list management
   - BDA integration (optional)

3. `katla_gfx/src/particles/shaders/` (2 files)
   - `particle_update.wgsl` (compute shader)
   - `particle_render.wgsl` (vertex + fragment)

4. `katla_app/src/components/particle_emitter.rs` (60 lines)
   - Simplified emitter component
   - Only configuration, no GPU resource management

**New API:**
```rust
// Initialize once (not per-emitter)
let system = GlobalParticleSystem::new(renderer, MAX_PARTICLES)?;

// Create emitter (lightweight)
let emitter = system.create_emitter(config)?;
system.update_emitter(emitter, new_config)?;

// Per-frame update (single call)
system.update(delta_time, emitters)?;
system.render(render_pass)?;
```

### Phase 3: Integration

**Renderer Changes:**
```rust
// In VulkanRenderer
pub struct VulkanRenderer {
    // Remove:
    // particle_system: Option<ParticleSystem>,
    
    // Add:
    global_particles: Option<GlobalParticleSystem>,
}

// Remove from frame rendering:
// - particle_system.update_frame_data()
// - Per-emitter compute dispatches
// - Per-emitter render passes

// Add single call:
self.global_particles.as_ref()?.update(frame_ctx)?;
self.global_particles.as_ref()?.render(render_pass)?;
```

**ECS Integration:**
```rust
// Simplified component
#[derive(Component)]
struct ParticleEmitter {
    config: EmitterConfig,
}

// System runs once per frame
fn particle_system_update(
    mut particle_system: ResMut<GlobalParticleSystem>,
    emitters: Query<&ParticleEmitter>,
) {
    particle_system.update(delta_time, emitters)?;
}
```

## Migration Strategy

### Step 1: Add New System Alongside Old

```rust
// In renderer.rs
pub struct VulkanRenderer {
    particle_system_legacy: Option<ParticleSystem>, // Keep working
    global_particles: Option<GlobalParticleSystem>, // Add new
}
```

### Step 2: Migrate Emitters Incrementally

```rust
// Component flag for migration
#[derive(Component)]
struct ParticleEmitter {
    config: EmitterConfig,
    use_legacy_system: bool, // Migration flag
}
```

### Step 3: Feature Flag for Testing

```rust
// Enable via environment variable
let use_modern_particles = std::env::var("KATLA_MODERN_PARTICLES")
    .unwrap_or_default() == "1";

if use_modern_particles {
    self.global_particles.update()?;
} else {
    self.particle_system_legacy.update_frame_data()?;
}
```

### Step 4: Remove Old Code

```bash
# After validation period (2-3 weeks)
rm katla_gfx/src/vulkan/particle_buffer.rs
rm katla_gfx/src/renderer/particle_system.rs
rm katla_app/src/components/particle.rs
rm resources/shaders/particles/particle_sim.wgsl
rm resources/shaders/particles/particle_render.wgsl
```

## Performance Improvements

### Memory Efficiency

**Before:**
- 100 emitters × 65536 particles × 48 bytes × 2 frames = 600 MB
- Plus descriptor sets, uniform buffers, fragmentation

**After:**
- Single buffer: 1M particles × 48 bytes = 48 MB
- Index lists: 3 × 4 MB = 12 MB
- Total: ~60 MB (10x memory reduction)

### CPU Overhead Reduction

**Before:**
- Per-emitter descriptor updates (100 emitters × 60 sets = 6000 updates/frame)
- Multiple compute dispatches (100 draw calls)
- Complex barrier management

**After:**
- Single descriptor set (bindless)
- One compute dispatch
- One indirect draw call
- Simple barrier: COMPUTE → VERTEX

### GPU Efficiency

**Before:**
- Ring buffer waste (dead slots never reused efficiently)
- Per-emitter workgroup dispatch (fragmentation)
- Bounds checking in shaders

**After:**
- Atomic counter allocation (perfect packing)
- Single global dispatch (optimal workgroup utilization)
- Indirect drawing (no dead particle rendering)

## Validation Plan

### Performance Metrics

```rust
// Track during migration
struct ParticleMetrics {
    active_particles: u32,
    emit_rate: f32,
    gpu_time_ms: f32,
    cpu_time_us: f32,
    memory_mb: f32,
    descriptor_sets: u32,
}
```

### Test Cases

1. **Stress Test**: 100 emitters, 1000 particles each
2. **Burst Test**: Single emitter, 100K particles burst
3. **Memory Test**: Create/destroy emitters repeatedly
4. **Regression Test**: Compare visual output with old system

### Validation Commands

```bash
# Run particle validation
cargo run --bin particle_validation -- --emit-test --stress-test

# Compare performance
cargo run --release -- --benchmark-particles

# Memory profiling
VK_NV_DEVICE_DIAGNOSTICS_CONFIG=1 cargo run
```

## Risks and Mitigations

### Risk 1: BDA Not Available on All GPUs

**Mitigation**: Fallback to bindless descriptors
```rust
#[cfg(feature = "buffer_device_address")]
type ParticleBuffer = BDAParticleBuffer;
#[cfg(not(feature = "buffer_device_address"))]
type ParticleBuffer = BindlessParticleBuffer;
```

### Risk 2: Atomic Counter Performance

**Mitigation**: Profile and use append/consume buffers if needed
```rust
// Fallback to structured buffer append/consume
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require
```

### Risk 3: WGSL Atomic Limitations

**Mitigation**: Workgroup-scoped atomics + memory barriers
```wgsl
// WGSL requires explicit memory barriers for cross-workgroup synchronization
atomicWorkgroupBarrier(&counters);
```

### Risk 4: Push Descriptor Overhead

**Mitigation**: Batch all per-frame updates into single push
```rust
// Single push_descriptor_set call for all per-frame data
// Instead of multiple update_descriptor_sets calls
```

## Timeline

### Week 1: Foundation
- [ ] Delete old particle system files
- [ ] Create new module structure
- [ ] Implement global particle buffer
- [ ] Write compute shader skeleton

### Week 2: Core Logic
- [ ] Implement atomic counter management
- [ ] Write particle update compute shader
- [ ] Implement index list swapping
- [ ] Add indirect drawing setup

### Week 3: Rendering
- [ ] Write vertex/fragment shaders
- [ ] Implement billboard rendering
- [ ] Add depth sorting (optional)
- [ ] Integrate with renderer

### Week 4: Migration
- [ ] Update ECS components
- [ ] Migrate existing emitters
- [ ] Performance testing
- [ ] Remove old code

### Week 5: Polish
- [ ] Add particle pooling optimization
- [ ] Implement culling
- [ ] Documentation
- [ ] Final validation

## Success Criteria

1. **Performance**: 10x reduction in CPU overhead, 2x GPU throughput
2. **Memory**: 10x reduction in memory usage
3. **Code**: 60% reduction in particle system code (400 → 160 lines)
4. **Features**: All existing particle effects work identically
5. **Maintainability**: Single file API, no descriptor management

## References

1. **Wicked Engine GPU Particles**: https://wickedengine.net/2017/11/gpu-based-particle-simulation/
2. **Modern Vulkan 2025**: https://medium.com/@allenphilip78/using-modern-vulkan-in-2025-0bac45174304
3. **Sascha Willems Vulkan Examples**: https://github.com/SaschaWillems/Vulkan
4. **Vulkan Guide Bindless**: https://vkguide.dev/docs/gpudriven/gpu_driven_engines/
5. **Intel Parallel Techniques**: https://www.intel.com/content/www/us/en/developer/articles/technical/parallel-techniques-in-modeling-particle-systems-using-vulkan-api.html

---

**Document Status**: Draft v1.0  
**Last Updated**: 2025-03-14  
**Author**: AI Research Assistant  
**Review Status**: Pending Architecture Review
