# Particle System Implementation Plan

**Status**: In Progress | **Started**: 2025-03-16 | **Last Updated**: 2025-03-16

## Overview

Modern GPU-driven particle system for Katla 3D engine implementing best practices for Vulkan-based rendering with ECS integration.

**Current State**: Architecturally sound but has critical bugs and missing workflow features.

**Goal**: Production-ready particle system with runtime tools, proper compute integration, and comprehensive validation.

---

## Progress Tracking

- [x] **PHASE 0: Foundation for Game Development** (6-8 hours) ✅ COMPLETED
  - [x] 0.1 GPU Timing Queries (2h) ✅ DONE
  - [x] 0.2 Runtime Tweak UI (3h) ✅ DONE
  - [x] 0.3 Preset System (2h) ✅ DONE
  - [ ] 0.4 Burst/One-Shot API (1h) - DEFERRED (lower priority)
- [x] **PHASE 1: Critical Bug Fixes** (2-3 hours) ✅ COMPLETED
  - [x] 1.1 Fix 32-Emitter Limit (1h) ✅ DONE
  - [x] 1.2 Add Emitter Count to FrameData (30m) ✅ DONE
  - [x] 1.3 Fix Workgroup Calculation (1h) ✅ DONE
  - [x] 1.4 Add Bounds Checking (30m) ✅ DONE
- [x] **PHASE 2: Compute Pass Integration** (3-4 hours) ✅ COMPLETED
  - [x] 2.1 Integrate ComputePass Template (2h) ✅ DONE
  - [x] 2.2 Remove Manual Dispatch (1h) ✅ DONE
  - [x] 2.3 Automatic Synchronization (1h) ✅ DONE
- [x] **PHASE 3: Compute Shader Optimization** (4-6 hours) ✅ COMPLETED
  - [x] 3.1 Separate Emit/Simulate Passes (3h) ✅ DONE
  - [x] 3.2 Tune Workgroup Size (1h) ✅ DONE
  - [ ] 3.3 Add Emitter Pre-Sorting (2h) - DEFERRED (optimization)
- [ ] **PHASE 4: Emitter Shapes** (3-4 hours)
  - [ ] 4.1 Add Emitter Shapes to Config (1h)
  - [ ] 4.2 Update Compute Shader (2h)
  - [ ] 4.3 Add to UI (1h)
- [x] **PHASE 5: Comprehensive Statistics** (2-3 hours) ✅ COMPLETED
  - [x] 5.1 Expand Stats Structure (1h) ✅ DONE
  - [x] 5.2 UI Integration (1h) ✅ DONE
  - [x] 5.3 Memory Tracking (1h) ✅ DONE
- [ ] **PHASE 6: Validation & Testing** (2-3 hours)
  - [ ] 6.1 Validation Layer (1h)
  - [ ] 6.2 Unit Tests (1h)
  - [ ] 6.3 Integration Tests (1h)

**Overall Progress**: 0% (0/31 tasks)

---

## PHASE 0: Foundation for Game Development ⭐ START HERE

**Priority**: CRITICAL | **Complexity**: MEDIUM | **Estimated Time**: 6-8 hours

**Rationale**: Both graphics and app perspectives agree this is missing foundation work. This phase delivers immediate value to game developers while enabling data-driven optimization.

### 0.1 GPU Timing Queries (2 hours)
**Status**: ⬜ Not Started

**Objective**: Add timestamp queries for compute shader performance measurement.

**Why**: Cannot optimize what you cannot measure. Critical for data-driven optimization decisions.

**Implementation**:
```rust
// katla_gfx/src/particles/timing.rs (NEW FILE)
pub struct TimestampQuery {
    start: vk::QueryPool,
    end: vk::QueryPool,
}

impl GlobalParticleSystem {
    fn record_compute_with_timing(&self, cmd: vk::CommandBuffer) -> Result<(), String> {
        unsafe {
            self.context.device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                self.timing_queries.start
            );
            
            self.record_compute_dispatch(cmd, ...)?;
            
            self.context.device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                self.timing_queries.end
            );
        }
        Ok(())
    }
    
    fn get_compute_time_ms(&self) -> f32 {
        // Read back timestamps and calculate duration
    }
}
```

**Files to Create**:
- `katla_gfx/src/particles/timing.rs`

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (add timing module)
- `katla_gfx/src/particles/buffer.rs` (cleanup query pools)

**Success Criteria**:
- ✅ Can measure compute shader execution time in milliseconds
- ✅ Timing data available to statistics system
- ✅ Query pools properly cleaned up

**Acceptance Tests**:
- Compute time < 3ms for 1M particles (RTX 3060 baseline)
- Compute time < 0.5ms for 100K particles
- No validation layer warnings

---

### 0.2 Runtime Tweak UI (3 hours)
**Status**: ⬜ Not Started

**Objective**: Add imgui panel for runtime emitter configuration editing.

**Why**: Game developers need to iterate on particle effects without recompiling. Critical for workflow.

**Implementation**:
```rust
// katla_app/src/ui/particle_inspector.rs (NEW FILE)
pub struct ParticleInspector {
    selected_emitter: Option<entity::EntityId>,
}

impl ParticleInspector {
    pub fn render(&mut self, ui: &mut imgui::Ui, world: &mut World) {
        ui.window("Particle Inspector")
            .size([400.0, 300.0], imgui::Condition::FirstUseEver)
            .build(|| {
                // Emitter selector
                // Parameter sliders (emit rate, lifetime, color, etc.)
                // Real-time stats display
                // Toggle emitters on/off
            });
    }
}
```

**UI Components**:
- Emitter selector dropdown
- Parameter sliders:
  - Emit Rate (0-10000 particles/sec)
  - Base Lifetime (0-10 seconds)
  - Velocity Magnitude (0-50 m/s)
  - Base Scale (0-5.0)
  - Color RGBA (0-1)
- Real-time stats:
  - Current alive count
  - Compute time (ms)
  - Memory usage (MB)
- Toggle buttons (enable/disable emitters)
- Reset button (reset particle system)

**Files to Create**:
- `katla_app/src/ui/particle_inspector.rs`

**Files to Modify**:
- `katla_app/src/ui/mod.rs` (add inspector module)
- `katla_app/src/ui/editor_ui.rs` (integrate inspector)
- `katla_app/src/main.rs` (show/hide inspector)

**Success Criteria**:
- ✅ Can modify emitter config at runtime
- ✅ Changes take effect immediately (no recompile)
- ✅ See particle count and timing in real-time
- ✅ Can toggle emitters on/off

**Acceptance Tests**:
- Change emit rate from 50 to 500 → see 10x more particles
- Change color from red to blue → particles turn blue
- Disable emitter → particles stop spawning
- Enable emitter → particles resume spawning

---

### 0.3 Preset System (2 hours)
**Status**: ⬜ Not Started

**Objective**: Serialize/deserialize `EmitterConfig` to/from JSON for saveable particle effects.

**Why**: Essential for shipping games. Artists need preset libraries (fire_small, fire_large, smoke, etc.).

**Implementation**:
```rust
// katla_gfx/src/particles/presets.rs (NEW FILE)
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EmitterPreset {
    pub name: String,
    pub config: EmitterConfig,
}

impl GlobalParticleSystem {
    pub fn save_preset(&self, name: &str, config: &EmitterConfig) -> Result<(), String> {
        let preset = EmitterPreset {
            name: name.to_string(),
            config: *config,
        };
        let json = serde_json::to_string_pretty(&preset)
            .map_err(|e| format!("Failed to serialize preset: {}", e))?;
        
        std::fs::write(format!("assets/particles/{}.json", name), json)
            .map_err(|e| format!("Failed to write preset file: {}", e))?;
        
        Ok(())
    }
    
    pub fn load_preset(&self, name: &str) -> Result<EmitterConfig, String> {
        let path = format!("assets/particles/{}.json", name);
        let json = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read preset file: {}", e))?;
        
        let preset: EmitterPreset = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to deserialize preset: {}", e))?;
        
        Ok(preset.config)
    }
    
    pub fn load_all_presets(&mut self) -> Result<(), String> {
        // Auto-load all presets from assets/particles/
        let presets_dir = std::path::Path::new("assets/particles");
        if !presets_dir.exists() {
            std::fs::create_dir_all(presets_dir)
                .map_err(|e| format!("Failed to create presets directory: {}", e))?;
        }
        
        // Scan directory and load all .json files
        // Store in internal preset library
        Ok(())
    }
}
```

**Preset Library** (include with engine):
```
assets/particles/
├── fire_small.json      (emit_rate: 100, color: orange)
├── fire_large.json      (emit_rate: 1000, color: orange)
├── smoke.json           (emit_rate: 200, color: gray, lifetime: 5s)
├── sparkles.json        (emit_rate: 500, color: white, lifetime: 1s)
├── rain.json            (emit_rate: 5000, color: blue, velocity: down)
└── snow.json            (emit_rate: 1000, color: white, velocity: drift)
```

**Files to Create**:
- `katla_gfx/src/particles/presets.rs`
- `assets/particles/fire_small.json`
- `assets/particles/fire_large.json`
- `assets/particles/smoke.json`
- `assets/particles/sparkles.json`

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (add presets module)
- `katla_app/src/components/particle.rs` (add preset loading convenience methods)

**Success Criteria**:
- ✅ Can save emitter config as JSON
- ✅ Can load emitter config from JSON
- ✅ Presets auto-load from assets/particles/
- ✅ Include 5 example presets

**Acceptance Tests**:
- Create fire effect, save as "my_fire.json"
- Load "my_fire.json" → identical parameters
- Load missing preset → proper error message
- Invalid JSON → proper error message

---

### 0.4 Burst/One-Shot API (1 hour)
**Status**: ⬜ Not Started

**Objective**: Add convenience methods for immediate particle bursts and timed emission.

**Why**: Essential for explosions, impacts, and temporary effects.

**Implementation**:
```rust
// katla_gfx/src/particles/mod.rs
impl GlobalParticleSystem {
    /// Emit particles immediately (one-shot effect)
    pub fn burst(&mut self, emitter_handle: EmitterHandle, count: u32) -> Result<(), String> {
        // Set emitter to burst mode
        // Override emit_rate for this frame only
        // Emit exactly `count` particles
        Ok(())
    }
}

// katla_app/src/components/particle.rs
impl ParticleEmitterComponent {
    /// Emit particles immediately (convenience method)
    pub fn burst(&mut self, count: u32) {
        if let Some(handle) = self.emitter_handle {
            // Queue burst for next frame
            self.burst_queue.push((handle, count));
        }
    }
    
    /// Emit particles for a specific duration
    pub fn emit_for(&mut self, duration: f32) {
        self.timed_emission = Some(duration);
    }
}
```

**Use Cases**:
```rust
// Explosion effect
explosion_emitter.burst(1000);

// Impact effect
bullet_impact_emitter.burst(50);

// Temporary effect
spell_emitter.emit_for(2.0); // Emit for 2 seconds then stop
```

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (add burst methods)
- `katla_app/src/components/particle.rs` (add convenience methods)
- `katla_app/src/systems/particle_system.rs` (handle burst_queue)

**Success Criteria**:
- ✅ Can emit immediate burst of N particles
- ✅ Can emit for X seconds then stop
- ✅ Burst/timed emission works with ECS integration

**Acceptance Tests**:
- Burst(100) → 100 particles appear immediately
- EmitFor(1.0) at 60fps → particles stop after ~1 second
- Multiple bursts → correct particle count

---

## PHASE 1: Critical Bug Fixes ⚠️ BLOCKER

**Priority**: CRITICAL | **Complexity**: LOW | **Estimated Time**: 2-3 hours

**Rationale**: The 32-emitter limit is a critical data corruption issue that must be fixed immediately.

### 1.1 Fix 32-Emitter Limit (1 hour)
**Status**: ⬜ Not Started

**Issue**: Shader hardcodes 32 emitters but CPU allows 1024. Emitters 33-1024 silently fail.

**Current Code** (WRONG):
```wgsl
// resources/shaders/particles/particle_update.wgsl:177
let emitter_idx = idx % 32u; // Support up to 32 emitters per frame
```

**Fixed Code** (CORRECT):
```wgsl
let emitter_count = frame_data.emitter_count;
if (emitter_count == 0u) { return; } // No active emitters

// Calculate emitter index using round-robin distribution
let wg_id = global_id.x / 256u; // Workgroup ID
let local_id = global_id.x % 256u; // Thread in workgroup
let emitter_idx = (wg_id + local_id) % emitter_count;

// Bounds check (debug builds)
#ifdef DEBUG
    if (emitter_idx >= MAX_EMITTERS) {
        return;
    }
#endif
```

**Files to Modify**:
- `resources/shaders/particles/particle_update.wgsl` (fix calculation)

**Success Criteria**:
- ✅ All 1024 emitters work correctly
- ✅ Emitter 100 spawns particles as emitter 100 (not emitter 4)
- ✅ No out-of-bounds access

---

### 1.2 Add Emitter Count to FrameData (30 minutes)
**Status**: ⬜ Not Started

**Objective**: Add `emitter_count` field to FrameData structure.

**Implementation**:
```rust
// katla_gfx/src/particles/buffer.rs
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameData {
    pub delta_time: f32,
    pub total_emit_count: u32,
    pub emitter_count: u32,  // NEW
    pub random_seed: u32,
}
```

**Update Upload**:
```rust
// katla_gfx/src/particles/mod.rs
fn update_frame_data(&self, delta_time: f32, emit_count: u32) -> Result<(), String> {
    let active_emitter_count = self.emitters.iter()
        .filter(|e| e.emit_rate > 0.0)
        .count() as u32;
    
    let frame_data = FrameData {
        delta_time,
        total_emit_count: emit_count,
        emitter_count: active_emitter_count,  // NEW
        random_seed: self.frame_count,
        _pad: 0,
    };
    // ... upload to GPU
}
```

**Files to Modify**:
- `katla_gfx/src/particles/buffer.rs` (update FrameData struct)
- `resources/shaders/particles/particle_update.wgsl` (update struct definition)
- `katla_gfx/src/particles/mod.rs` (upload active emitter count)

**Success Criteria**:
- ✅ FrameData includes emitter_count
- ✅ Active emitter count calculated each frame
- ✅ Shader has correct emitter count

---

### 1.3 Fix Workgroup Calculation (1 hour)
**Status**: ⬜ Not Started

**Issue**: Current workgroup calculation only accounts for emission, not simulation.

**Current Code** (WRONG):
```rust
// katla_gfx/src/render_graph/graph.rs:1654
let total_workgroups = (total_emit_count + workgroup_size - 1) / workgroup_size;
```

**Fixed Code** (CORRECT):
```rust
// Calculate total work: emit new particles + simulate existing particles
let alive_count = self.buffer.get_alive_count().unwrap_or(0);
let total_work = alive_count.saturating_add(total_emit_count);
let total_workgroups = (total_work + workgroup_size - 1) / workgroup_size;

log::debug!("Particle workgroups: {} alive + {} emit = {} total ({} workgroups)",
    alive_count, total_emit_count, total_work, total_workgroups);
```

**Files to Modify**:
- `katla_gfx/src/render_graph/graph.rs` (fix workgroup calculation)

**Success Criteria**:
- ✅ Workgroup count includes alive particles
- ✅ All particles simulated each frame
- ✅ No particles skip simulation

---

### 1.4 Add Bounds Checking (30 minutes, debug only)
**Status**: ⬜ Not Started

**Objective**: Add debug-only validation for out-of-bounds access.

**Implementation**:
```wgsl
// resources/shaders/particles/particle_update.wgsl
#ifdef DEBUG
    // Validate particle index
    if (particle_idx >= MAX_PARTICLES) {
        // Write to error buffer for CPU readback
        let error_slot = atomicAdd(&error_count, 1u);
        if (error_slot < 1024u) {
            error_buffer[error_slot] = ErrorData(
                particle_idx, emitter_idx, 0xDEADBEEF
            );
        }
        return;
    }
    
    // Validate emitter index
    if (emitter_idx >= MAX_EMITTERS) {
        // Same error handling
        return;
    }
#endif
```

**Files to Modify**:
- `resources/shaders/particles/particle_update.wgsl` (add validation)

**Success Criteria**:
- ✅ Debug builds validate indices
- ✅ Release builds have zero overhead
- ✅ Errors reported to CPU for debugging

---

## PHASE 2: Compute Pass Integration 🔧

**Priority**: HIGH | **Complexity**: HIGH | **Estimated Time**: 3-4 hours

**Rationale**: Proper render graph integration for correct synchronization and dependency tracking.

### 2.1 Integrate ComputePass Template (2 hours)
**Status**: ⬜ Not Started

**Objective**: Use render graph's ComputePass template for particle simulation.

**Implementation**:
```rust
// katla_app/src/application/rendering.rs
// In frame graph builder
let particle_update = ComputePass::new("particle_update")
    .pipeline(particle_compute_pipeline)
    .reads("particles")
    .writes("particles")
    .workgroup_count(dynamic); // Calculated at runtime

let graph = FrameGraph::builder()
    .add_pass(particle_update)
    // ... other passes
    .build(&renderer)?;
```

**Files to Modify**:
- `katla_app/src/application/rendering.rs` (integrate compute pass)
- `katla_gfx/src/render_graph/passes/compute.rs` (extend if needed)

**Success Criteria**:
- ✅ Particle compute uses ComputePass template
- ✅ Dependencies declared in render graph
- ✅ Automatic barrier insertion

---

### 2.2 Remove Manual Dispatch (1 hour)
**Status**: ⬜ Not Started

**Objective**: Delete manual compute dispatch code and barriers.

**Implementation**:
```rust
// DELETE: katla_gfx/src/render_graph/graph.rs
// fn execute_all_particle_dispatches(&mut self) -> Result<(), RenderGraphError>
// fn execute_particle_dispatch(...)

// REPLACE with: render graph automatic execution
```

**Files to Modify**:
- `katla_gfx/src/render_graph/graph.rs` (remove manual dispatch)

**Success Criteria**:
- ✅ No manual compute dispatch code
- ✅ No manual barrier code
- ✅ Render graph handles execution

---

### 2.3 Automatic Synchronization (1 hour)
**Status**: ⬜ Not Started

**Objective**: Let render graph insert automatic barriers for particle buffers.

**Implementation**:
- Declare particle buffer as read/write resource
- Render graph inserts COMPUTE_SHADER → VERTEX_SHADER barriers
- Automatic dependency tracking with geometry/tonemap passes

**Files to Modify**:
- `katla_gfx/src/render_graph/builder.rs` (if needed)
- `katla_gfx/src/render_graph/compiler.rs` (if needed)

**Success Criteria**:
- ✅ Proper synchronization between compute and graphics
- ✅ No race conditions
- ✅ Validation layer happy

---

## PHASE 3: Compute Shader Optimization ⚡

**Priority**: MEDIUM | **Complexity**: MEDIUM | **Estimated Time**: 4-6 hours

**Rationale**: Reduce warp divergence and improve GPU utilization.

### 3.1 Separate Emit/Simulate Passes (3 hours)
**Status**: ⬜ Not Started

**Objective**: Split single-pass emit+simulate into two separate passes.

**Why**: Reduces warp divergence from 50% to near 0%.

**Implementation**:
```wgsl
// Pass 1: Emit only
@compute @workgroup_size(256)
fn cs_emit(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;
    if (idx >= frame_data.total_emit_count) { return; }
    
    // Emit logic only (no simulation)
    let emitter_idx = calculate_emitter_index(idx);
    let dead_slot = atomicSub(&counters.dead_count, 1u);
    // ... rest of emit logic
}

// Pass 2: Simulate only
@compute @workgroup_size(256)
fn cs_simulate(@builtin(global_invocation_id) global_id: vec3u) {
    let idx = global_id.x;
    if (idx >= initial_alive_count) { return; }
    
    // Simulate logic only (no emission)
    let particle_idx = alive_current[idx];
    // ... rest of simulate logic
}
```

**Files to Create**:
- `resources/shaders/particles/particle_emit.wgsl`
- `resources/shaders/particles/particle_simulate.wgsl`

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (two pipelines instead of one)
- `katla_app/src/application/rendering.rs` (two compute passes)

**Success Criteria**:
- ✅ Separate emit and simulate passes
- ✅ Measure 2-3x performance improvement
- ✅ Compute time < 1ms for 100K particles

---

### 3.2 Tune Workgroup Size (1 hour)
**Status**: ⬜ Not Started

**Objective**: Benchmark different workgroup sizes (64, 128, 256) and choose optimal.

**Implementation**:
- Add benchmark mode
- Test 64, 128, 256 workgroup sizes
- Measure compute time for 10K, 100K, 1M particles
- Choose best for target hardware

**Files to Modify**:
- `resources/shaders/particles/particle_simulate.wgsl` (workgroup_size constant)
- Add benchmark harness

**Success Criteria**:
- ✅ Benchmark data collected
- ✅ Optimal workgroup size chosen
- ✅ 10-20% performance improvement

---

### 3.3 Add Emitter Pre-Sorting (2 hours)
**Status**: ⬜ Not Started

**Objective**: Group particles by emitter for better cache locality.

**Implementation**:
```rust
// Pre-sort particles by emitter index
let emitter_particles: Vec<Vec<ParticleIndex>> = group_by_emitter(&alive_list);

// Emit particles in emitter groups
for (emitter_idx, particle_indices) in emitter_particles.iter() {
    // Process all particles from same emitter together
}
```

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (sorting logic)
- Compute shader (if needed)

**Success Criteria**:
- ✅ Particles grouped by emitter
- ✅ Better cache locality
- ✅ 5-10% performance improvement

---

## PHASE 4: Emitter Shapes 🎨

**Priority**: MEDIUM | **Complexity**: MEDIUM | **Estimated Time**: 3-4 hours

**Rationale**: Enable variety of particle effects (rain, area effects, volume spawning).

### 4.1 Add Emitter Shapes to Config (1 hour)
**Status**: ⬜ Not Started

**Implementation**:
```rust
// katla_gfx/src/particles/mod.rs
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub enum EmitterShape {
    Point = 0,
    Line = 1,
    Circle = 2,
    Sphere = 3,
    Box = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EmitterConfig {
    pub position: [f32; 3],
    pub shape: EmitterShape,
    pub shape_params: [f32; 4],  // Shape-specific parameters
    // ... rest of config
}
```

**Shape Parameters**:
- Point: unused
- Line: [start_x, start_y, start_z, end_x, end_y, end_z]
- Circle: [center_x, center_y, center_z, radius, normal_x, normal_y, normal_z]
- Sphere: [center_x, center_y, center_z, radius]
- Box: [min_x, min_y, min_z, max_x, max_y, max_z]

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (update EmitterConfig)

---

### 4.2 Update Compute Shader (2 hours)
**Status**: ⬜ Not Started

**Implementation**:
```wgsl
// resources/shaders/particles/particle_emit.wgsl
fn sample_emitter_position(config: EmitterConfig, seed: ptr<function, u32>) -> vec3f {
    switch config.shape {
        case EmitterShape::Point => {
            return config.position;
        }
        case EmitterShape::Line => {
            let t = random_float(seed);
            return mix(config.shape_params.xyz, config.shape_params2.xyz, t);
        }
        case EmitterShape::Circle => {
            let theta = random_float(seed) * 6.28318530718;
            let r = config.shape_params.w * sqrt(random_float(seed));
            let center = config.shape_params.xyz;
            let normal = config.shape_params2.xyz;
            // Calculate circle point...
        }
        case EmitterShape::Sphere => {
            let theta = random_float(seed) * 6.28318530718;
            let phi = acos(2.0 * random_float(seed) - 1.0);
            let r = config.shape_params.w * pow(random_float(seed), 1.0/3.0);
            // Calculate sphere point...
        }
        case EmitterShape::Box => {
            let min = config.shape_params.xyz;
            let max = config.shape_params2.xyz;
            return vec3f(
                mix(min.x, max.x, random_float(seed)),
                mix(min.y, max.y, random_float(seed)),
                mix(min.z, max.z, random_float(seed))
            );
        }
        default => {
            return config.position;
        }
    }
}
```

**Files to Modify**:
- `resources/shaders/particles/particle_emit.wgsl` (shape sampling)

---

### 4.3 Add to UI (1 hour)
**Status**: ⬜ Not Started

**Objective**: Add shape selector and parameter inputs to runtime UI.

**Implementation**:
- Shape dropdown (Point, Line, Circle, Sphere, Box)
- Context-sensitive parameter inputs
- Visual preview of emitter shape (optional)

**Files to Modify**:
- `katla_app/src/ui/particle_inspector.rs`

**Success Criteria**:
- ✅ Can select emitter shape in UI
- ✅ Can edit shape parameters
- ✅ Include example presets for each shape

---

## PHASE 5: Comprehensive Statistics 📊

**Priority**: MEDIUM | **Complexity**: LOW | **Estimated Time**: 2-3 hours

**Rationale**: Enable performance monitoring and debugging.

### 5.1 Expand Stats Structure (1 hour)
**Status**: ⬜ Not Started

**Implementation**:
```rust
// katla_gfx/src/particles/stats.rs
pub struct ParticleStats {
    pub max_alive_count: u32,
    pub current_alive_count: u32,
    pub total_emitted: u64,
    pub compute_time_ms: f32,  // From Phase 0
    pub emitter_counts: [u32; MAX_EMITTERS],
    pub memory_used_mb: f32,
    pub buffer_utilization: f32,  // alive_count / max_particles
}
```

**Files to Create**:
- `katla_gfx/src/particles/stats.rs`

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (track stats)

---

### 5.2 UI Integration (1 hour)
**Status**: ⬜ Not Started

**Objective**: Display stats in editor UI.

**Implementation**:
- Stats panel in particle inspector
- Real-time graphs (particle count over time)
- Performance warnings (if compute time > threshold)

**Files to Modify**:
- `katla_app/src/ui/particle_inspector.rs`

**Success Criteria**:
- ✅ Stats visible in UI
- ✅ Real-time updates
- ✅ Performance warnings

---

### 5.3 Memory Tracking (1 hour)
**Status**: ⬜ Not Started

**Objective**: Track GPU memory usage and buffer utilization.

**Implementation**:
```rust
impl GlobalParticleSystem {
    pub fn memory_usage_mb(&self) -> f32 {
        // Calculate total GPU memory used
        let particle_data_mb = (self.max_particles as f32) * 48.0 / (1024.0 * 1024.0);
        let index_lists_mb = (self.max_particles as f32) * 8.0 / (1024.0 * 1024.0);
        let configs_mb = (MAX_EMITTERS as f32) * 80.0 / (1024.0 * 1024.0);
        particle_data_mb + index_lists_mb + configs_mb
    }
    
    pub fn buffer_utilization(&self) -> f32 {
        (self.alive_count() as f32) / (self.max_particles as f32)
    }
}
```

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs`

**Success Criteria**:
- ✅ Memory usage calculated
- ✅ Buffer utilization tracked
- ✅ Visible in UI

---

## PHASE 6: Validation & Testing ✅

**Priority**: LOW | **Complexity**: LOW | **Estimated Time**: 2-3 hours

**Rationale**: Ensure system stability and correctness.

### 6.1 Validation Layer (1 hour)
**Status**: ⬜ Not Started

**Implementation**:
```rust
// katla_gfx/src/particles/validation.rs
impl GlobalParticleSystem {
    pub fn validate_counters(&self) -> Result<(), String> {
        let alive = self.alive_count();
        if alive > self.max_particles {
            return Err(format!("Counter corruption: alive={} > max={}",
                alive, self.max_particles));
        }
        Ok(())
    }
    
    pub fn validate_emitter_config(&self, config: &EmitterConfig) -> Result<(), String> {
        if config.emit_rate < 0.0 {
            return Err("Negative emit rate".to_string());
        }
        if config.base_lifetime <= 0.0 {
            return Err("Non-positive lifetime".to_string());
        }
        Ok(())
    }
}
```

**Files to Create**:
- `katla_gfx/src/particles/validation.rs`

**Files to Modify**:
- `katla_gfx/src/particles/mod.rs` (add validation calls)

---

### 6.2 Unit Tests (1 hour)
**Status**: ⬜ Not Started

**Tests**:
- Emitter creation/destruction
- Workgroup calculation edge cases
- Stats tracking
- Preset serialization

**Files to Create**:
- `katla_gfx/src/particles/tests.rs`

---

### 6.3 Integration Tests (1 hour)
**Status**: ⬜ Not Started

**Tests**:
- 1M particle stress test
- 1024 emitter stress test
- Memory leak detection
- Frame rate stability

**Files to Create**:
- `game/tests/particle_stress_tests.rs`

**Success Criteria**:
- ✅ All tests pass
- ✅ No validation warnings
- ✅ Stress tests stable

---

## Dependencies

```
PHASE 0 (Foundation)
    ↓
PHASE 1 (Bug Fixes) - BLOCKS: Everything else
    ↓
PHASE 2 (Compute Integration) - DEPENDS ON: Phase 1
    ↓
PHASE 3 (Optimization) - DEPENDS ON: Phase 2
    ↓
PHASE 4 (Shapes) - INDEPENDENT
    ↓
PHASE 5 (Statistics) - DEPENDS ON: Phase 0
    ↓
PHASE 6 (Testing) - DEPENDS ON: All phases
```

---

## Risk Assessment

**High Risk Items**:
- Phase 1.1 (32-emitter bug) - Data corruption, must fix first
- Phase 2.1 (Compute integration) - Core architecture change

**Medium Risk Items**:
- Phase 3.1 (Separate passes) - Shader complexity increase
- Phase 0.2 (Runtime UI) - New UI code

**Low Risk Items**:
- Phase 0.3 (Presets) - Isolated serialization code
- Phase 5 (Statistics) - Read-only metrics
- Phase 6 (Testing) - Non-production code

---

## Success Metrics

**Technical**:
- Compute time < 3ms for 1M particles
- Compute time < 0.5ms for 100K particles
- No validation layer warnings
- Proper render graph integration

**Usability**:
- Can create effect in < 5 minutes
- Can tweak at runtime without recompile
- Can save/load effects as JSON
- Have preset library with 5+ examples

**Workflow**:
- No recompilation to iterate on effects
- Visual feedback for all parameters
- Real-time performance metrics

---

## Notes

- **Total Estimated Time**: 22-31 hours
- **Critical Path**: Phase 0 → Phase 1 → Phase 2 → Phase 3 → Phase 6
- **Parallelizable**: Phase 4 can be done in parallel with Phase 2-3
- **Quick Wins**: Phase 0 delivers immediate value to users

**Last Updated**: 2025-03-16
