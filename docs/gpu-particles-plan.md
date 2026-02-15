# GPU Particle System Implementation Plan

This document describes the implementation plan for GPU-based particle effects in Katla using compute shaders.

## Status: ✅ Phase 1-5 Complete

All infrastructure and shaders have been implemented. The system is ready for integration into the render graph.

## Design Decisions

| Choice | Decision | Rationale |
|--------|----------|-----------|
| Buffering | **Single buffer, `read_write`** | Modern GPUs handle this well. Matches Unreal Niagara, Unity VFX Graph approach. |
| Rendering | **Billboard quads (instanced)** | No size limits, supports rotation/soft edges. AAA standard. |
| Max Count | **64K particles per emitter** | Fits cache, 4MB per emitter, matches typical workgroup dispatch sizes. |
| Buffer Type | **`DeviceAddressBuffer`** | Already has persistent mapping, BDA support, and implements `BufferDescriptorSource`. |

## Implementation Phases

### Phase 1: Compute Pipeline Infrastructure ✅

**Created Files:**
- `katla_vulkan/src/vulkan/material/compute_pipeline.rs` - ComputePipelineBuilder and ComputePipeline types

**Modified Files:**
- `katla_vulkan/src/sync.rs` - Added `BufferMemoryBarrier2` for compute-graphics sync
- `katla_vulkan/src/vulkan/commandbuffer.rs` - Added `dispatch()`, `dispatch_indirect()`, `push_constants()`
- `katla_vulkan/src/vulkan/material/mod.rs` - Re-exported compute pipeline types

**Key Types:**
```rust
// Compute pipeline creation
pub struct ComputePipelineBuilder { ... }
pub struct ComputePipeline { ... }

// Buffer barrier for compute-graphics sync
pub struct BufferMemoryBarrier2 { ... }

// Command buffer methods
pub fn dispatch(&self, x: u32, y: u32, z: u32)
pub fn dispatch_indirect(&self, buffer: vk::Buffer, offset: vk::DeviceSize)
pub fn push_constants<T: Pod + Zeroable>(&self, layout, stage_flags, offset, data)
```

### Phase 2: Render Graph Compute Passes ✅

**Modified Files:**
- `katla_vulkan/src/render_graph/pass.rs` - Added storage buffer helpers

**New PassBuilder Methods:**
```rust
pub fn read_storage(&mut self, resource_id: ResourceId) -> &mut Self
pub fn write_storage(&mut self, resource_id: ResourceId) -> &mut Self
pub fn read_write_storage(&mut self, resource_id: ResourceId) -> &mut Self
```

### Phase 3: Particle Buffer System ✅

**Created Files:**
- `katla_vulkan/src/vulkan/particle_buffer.rs` - Particle buffer types

**Modified Files:**
- `katla_vulkan/src/vulkan/mod.rs` - Exported particle_buffer module
- `katla_vulkan/Cargo.toml` - Added `derive` feature to bytemuck

**Key Types:**
```rust
pub const MAX_PARTICLES: usize = 65536; // 64K

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ParticleData {
    pub position: [f32; 3],
    pub _pad1: f32,
    pub velocity: [f32; 3],
    pub lifetime: f32,
    pub color: [f32; 4],
    pub scale: f32,
    pub _pad2: [f32; 3],
} // 64 bytes total - 4MB for 64K particles

pub struct ParticleBuffer {
    buffer: DeviceAddressBuffer,
    capacity: usize,
}

// Implements BufferDescriptorSource for easy descriptor binding
impl BufferDescriptorSource for ParticleBuffer { ... }

pub struct EmitterConfig { ... }  // Emitter settings
pub struct ParticlePushConstants { ... }  // Per-frame push constants
pub struct EmitterConfigBuffer { ... }  // Config uniform buffer

pub fn calculate_workgroup_count(particle_count: u32, workgroup_size: u32) -> u32
```

### Phase 4: ECS Integration ✅

**Created Files:**
- `katla_app/src/components/rendering/particle.rs` - ParticleEmitter component
- `katla_app/src/systems/particle/particle_system.rs` - ParticleSimulationSystem
- `katla_app/src/systems/particle/mod.rs` - Module exports

**Modified Files:**
- `katla_app/src/components/rendering/mod.rs` - Export particle module
- `katla_app/src/systems/mod.rs` - Export particle_system module
- `katla_app/Cargo.toml` - Added ash dependency

**Key Types:**
```rust
#[derive(Component)]
pub struct ParticleEmitter {
    pub particle_buffer: ParticleBuffer,
    pub compute_pipeline: ComputePipeline,
    pub descriptor_set: BufferDescriptorSet,
    pub config: EmitterConfig,
    pub emit_accumulator: f32,
    pub emit_rate: f32,
    pub is_active: bool,
    pub alive_count: u32,
    pub random_seed: u32,
}

pub struct ParticleSimulationSystem;
```

### Phase 5: Particle Rendering ✅

**Created Shader Files:**
- `resources/shaders/particles/particle_sim.wgsl` - Compute shader for simulation
- `resources/shaders/particles/particle_render.wgsl` - Billboard rendering shaders

**Compute Shader Design:**
```wgsl
@group(0) @binding(0) var<storage, read_write> particles: array<ParticleData>;
// Or via push constant BDA:
// var<storage, read_write> particles: array<ParticleData> = @buffer_address(particle_address);

struct ParticleData {
    position: vec3f,
    velocity: vec3f,
    lifetime: f32,
    color: vec4f,
    scale: f32,
}

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) id: vec3u) {
    let index = id.x;
    if (index >= arrayLength(&particles)) { return; }

    var particle = particles[index];

    // Update lifetime
    particle.lifetime -= pc.delta_time;

    if (particle.lifetime <= 0.0) {
        // Emit new particle (atomic counter for emit index)
    } else {
        // Simulate: position += velocity * dt
        particle.position += particle.velocity * pc.delta_time;
    }

    particles[index] = particle;
}
```

**Render Graph Integration:**
```
particle_sim (Compute) -> barrier -> geometry_pass (Graphics)
```

## Usage Example

```rust
// Create particle buffer
let particle_buffer = ParticleBuffer::with_max_capacity(context.clone())?;

// Create compute pipeline
let compute_pipeline = ComputePipelineBuilder::new(context.clone())
    .with_shader(compute_shader_module)
    .with_descriptor_layouts(vec![descriptor_set_layout])
    .add_push_constant_range(vk::ShaderStageFlags::COMPUTE, 0, size_of::<ParticlePushConstants>() as u32)
    .build()?;

// Create descriptor set
let descriptor_set = BufferDescriptorSetBuilder::new(&context)
    .add_entire_buffer(&particle_buffer, 0)
    .build(layout)?;

// Dispatch in compute pass
command_buffer.bind_pipeline(pipeline.vk_pipeline(), vk::PipelineBindPoint::COMPUTE);
command_buffer.bind_descriptor_sets(
    vk::PipelineBindPoint::COMPUTE,
    pipeline.vk_layout(),
    &[descriptor_set.set()],
);
command_buffer.push_constants(
    pipeline.vk_layout(),
    vk::ShaderStageFlags::COMPUTE,
    0,
    &push_constants,
);
command_buffer.dispatch(workgroup_count, 1, 1);
```

## Critical Files Reference

| Pattern Source | Path | Usage |
|----------------|------|-------|
| DeviceAddressBuffer | `katla_vulkan/src/vulkan/bda.rs` | Base buffer type with persistent mapping |
| BufferDescriptorSetBuilder | `katla_vulkan/src/vulkan/material/buffer_descriptor.rs` | Generic descriptor creation |
| BufferDescriptorSource | Same as above | Trait implemented by ParticleBuffer |
| ComputePipelineBuilder | `katla_vulkan/src/vulkan/material/compute_pipeline.rs` | Compute pipeline creation |
| BufferMemoryBarrier2 | `katla_vulkan/src/sync.rs` | Buffer synchronization |
| PassBuilder | `katla_vulkan/src/render_graph/pass.rs` | Compute pass helpers |
| ParticleBuffer | `katla_vulkan/src/vulkan/particle_buffer.rs` | Particle storage |

## Verification

1. **Build**: `cargo build` after each phase ✅
2. **Tests**: `cargo test -p katla_vulkan` ✅
3. **Validation**: `cargo run -- -s` for Vulkan validation (25 frames)
4. **Visual**: Render basic particle emitter with 64K particles (TODO)
