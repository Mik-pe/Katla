# Render Graph Production Phase Implementation Plan

## Overview

### Goal
Transform the render graph from a working prototype into a production-ready system through subresource tracking, improved aliasing algorithms, performance profiling hooks, and comprehensive real-world testing.

### Current State (Post Medium-term)
The render graph is functional with:
- Whole-image resource tracking (no mip/array layer granularity)
- Simple first-fit aliasing algorithm  
- No performance profiling capabilities
- Limited real-world GPU workload testing
- Basic barrier generation (whole-resource transitions)

### Target State (Production Ready)
- **Subresource Tracking**: Per-mip and per-array-layer state tracking
- **Improved Aliasing**: Best-fit with memory type compatibility (10-30% better memory utilization)
- **Performance Profiling**: Built-in hooks for timing, memory metrics, barrier analysis
- **Battle-Tested**: Validated against complex real-world rendering scenarios

---

## Part 1: Subresource Tracking

### 1.1 Current Limitations

**File:** katla_vulkan/src/render_graph/resource.rs

The ImageDescriptor lacks subresource specification:
- No mip_levels field
- No array_layers field  
- Extent is only 2D (Extent2D)

**File:** katla_vulkan/src/render_graph/barrier.rs

Barriers always use whole-resource subresource ranges (hardcoded to 1 mip, 1 layer).

### 1.2 Design: New Types

**New File:** katla_vulkan/src/render_graph/subresource.rs

Create the following types:

```rust
/// Identifies a specific subresource within an image.
pub struct SubresourceId {
    pub mip_level: u8,
    pub array_layer: u16,
    pub aspect: AspectFlags,
}

/// Range of subresources for barrier operations.
pub struct SubresourceRange {
    pub base_mip_level: u8,
    pub level_count: u8,
    pub base_array_layer: u16,
    pub layer_count: u16,
    pub aspect_mask: AspectFlags,
}

/// State of a single subresource.
pub struct SubresourceState {
    pub layout: vk::ImageLayout,
    pub stage: vk::PipelineStageFlags2,
    pub access: vk::AccessFlags2,
}

/// Tracks state for all subresources of an image.
pub struct ImageSubresourceTracker {
    states: Vec<Vec<Vec<SubresourceState>>>,
}
```

Key methods:
- SubresourceRange::full() - all mips/layers
- SubresourceRange::mip(level) - single mip
- SubresourceRange::layer(layer) - single layer
- ImageSubresourceTracker::transition() - returns barriers needed

### 1.3 API Changes to ImageDescriptor

**File:** katla_vulkan/src/render_graph/resource.rs

```rust
pub struct ImageDescriptor {
    pub format: vk::Format,
    pub extent: vk::Extent3D,  // CHANGED from Extent2D
    pub mip_levels: u8,        // NEW
    pub array_layers: u16,     // NEW
    pub usage: vk::ImageUsageFlags,
    pub name: &'static str,
    pub aliasable: bool,
}
```

### 1.4 Pass Access API Extensions

**File:** katla_vulkan/src/render_graph/pass.rs

Add subresource-aware methods:
- read_image_mip(image, mip_level)
- read_image_layer(image, layer)
- write_attachment_mip(image, mip, att_type)
- write_attachment_layer(image, layer, att_type)

### 1.5 Files to Create/Modify

| File | Action | Changes |
|------|--------|---------|
| subresource.rs | CREATE | SubresourceId, SubresourceRange, ImageSubresourceTracker |
| resource.rs | MODIFY | ImageDescriptor: add mip_levels, array_layers, use Extent3D |
| barrier.rs | MODIFY | ResourceState: integrate subresource tracking |
| pass.rs | MODIFY | Add subresource-aware access methods |
| sync.rs | MODIFY | ImageBarrier: use SubresourceRange |
| allocation.rs | MODIFY | Create images with proper mip/layers |
| mod.rs | MODIFY | Export subresource module |

---

## Part 2: Improved Aliasing Algorithm

### 2.1 Current Limitations

**File:** katla_vulkan/src/render_graph/aliasing.rs

The current first-fit algorithm has limitations:
1. No memory type compatibility checking
2. Suboptimal packing for mixed-size workloads
3. Size estimation is inaccurate (no actual GPU memory requirements)
4. No consideration of alignment granularity

### 2.2 Design: Best-Fit Algorithm

```rust
/// Memory requirements for a resource.
pub struct MemoryRequirements {
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
    pub preferred_locations: MemoryLocationFlags,
}

/// Enhanced alias group with memory type tracking.
pub struct AliasGroup {
    pub resources: Vec<VirtualResourceId>,
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
}
```

Algorithm:
1. Sort resources by size (largest first)
2. For each resource:
   a. Filter groups by lifetime compatibility
   b. Filter groups by memory type compatibility
   c. Calculate waste score (size difference)
   d. Select group with lowest waste
   e. If no fit, create new group

### 2.3 Memory Type Compatibility

```rust
fn memory_types_compatible(a: u32, b: u32) -> bool {
    (a & b) != 0
}
```

### 2.4 Integration Points

**File:** katla_vulkan/src/render_graph/allocation.rs

Add query_memory_requirements() to get actual GPU requirements via vkGetImageMemoryRequirements.

**File:** katla_vulkan/src/render_graph/aliasing.rs

Modify analyze() to accept optional MemoryRequirements map.

### 2.5 Files to Modify

| File | Action | Changes |
|------|--------|---------|
| aliasing.rs | MODIFY | Add MemoryRequirements, best-fit algorithm |
| allocation.rs | MODIFY | Query actual memory requirements, pass to aliasing |
| lifetime.rs | MODIFY | Add free slot tracking for better packing |

---

## Part 3: Performance Profiling

### 3.1 Design: Profiling Data Structures

**New File:** katla_vulkan/src/render_graph/profiling.rs

```rust
/// Profiling data for a single pass.
pub struct PassProfile {
    pub name: String,
    pub record_time: Duration,
    pub gpu_time: Option<Duration>,
    pub pre_barrier_count: usize,
    pub post_barrier_count: usize,
    pub read_count: usize,
    pub write_count: usize,
}

/// Complete frame profiling data.
pub struct FrameProfile {
    pub frame_index: u64,
    pub graph_name: &'static str,
    pub total_time: Duration,
    pub compile_time: Duration,
    pub execute_time: Duration,
    pub passes: Vec<PassProfile>,
    pub barriers: BarrierProfile,
    pub aliasing: AliasingProfile,
    pub memory_allocated: u64,
}

/// Trait for profiling backends.
pub trait Profiler: Send + Sync {
    fn begin_scope(&self, name: &str);
    fn end_scope(&self);
    fn record_metric(&self, name: &str, value: f64);
}

/// RAII scope timer.
pub struct ScopeTimer<'a> { ... }
```

### 3.2 Built-in Profilers

- NullProfiler: No-op implementation
- LoggingProfiler: Outputs to log crate  
- ChromeTracingProfiler: Export to chrome://tracing format

### 3.3 Integration Points

- compile_graph(): Add timing for each compilation step
- CompiledGraph::execute(): Time pass execution
- AliasingAnalysis::analyze(): Track analysis time
- BarrierGenerator::generate(): Track generation time

### 3.4 Feature Flag

Add compile-time feature flag `render_graph_profiling` to enable/disable profiling overhead.

### 3.5 Files to Create/Modify

| File | Action | Changes |
|------|--------|---------|
| profiling.rs | CREATE | PassProfile, FrameProfile, Profiler trait |
| compiled.rs | MODIFY | Add profiling hooks to compile/execute |
| aliasing.rs | MODIFY | Track analysis time |
| barrier.rs | MODIFY | Track generation time |
| mod.rs | MODIFY | Export profiling module |

---

## Part 3: Performance Profiling

### 3.1 Design: Profiling Data Structures

**New File:** katla_vulkan/src/render_graph/profiling.rs

```rust
pub struct PassProfile {
    pub name: String,
    pub record_time: Duration,
    pub gpu_time: Option<Duration>,
    pub pre_barrier_count: usize,
    pub post_barrier_count: usize,
}

pub struct BarrierProfile {
    pub total_barriers: usize,
    pub image_barriers: usize,
    pub buffer_barriers: usize,
    pub generation_time: Duration,
}

pub struct AliasingProfile {
    pub resource_count: usize,
    pub aliasable_count: usize,
    pub group_count: usize,
    pub memory_saved: u64,
    pub analysis_time: Duration,
}

pub struct FrameProfile {
    pub frame_index: u64,
    pub graph_name: &'static str,
    pub total_time: Duration,
    pub compile_time: Duration,
    pub execute_time: Duration,
    pub passes: Vec<PassProfile>,
    pub barriers: BarrierProfile,
    pub aliasing: AliasingProfile,
}
```

### 3.2 Profiler Trait

```rust
pub trait Profiler: Send + Sync {
    fn begin_scope(&self, name: &str);
    fn end_scope(&self);
    fn record_metric(&self, name: &str, value: f64);
}

```

### 3.3 Integration Points
Add `profile_scope!` macro to scoped timing.

### 3.4 Files to create/Modify
| File | Action | Changes |
|------|--------|---------|
| profiling.rs | CREATE | Core types |
| compiled.rs | MODIFY | Add profiling hooks |
| mod.rs | MODIFY | Export profiling module |

---

## Part 4: Testing Strategy

### 4.1 Real-World Test Scenarios

| Scenario | Description | Validates |
|----------|-------------|-----------|
| Deferred Shading | G-Buffer + Lighting + Post | Aliasing, barrier correctness |
| Mipmap Generation | Upload -> Generate mips -> Sample | Subresource transitions |
| Cubemap Rendering | 6-face cubemap rendering | Array layer tracking |
| Shadow Mapping | Multi-pass depth sampling | Lifetime analysis |
| Post Processing | Bloom + Tone mapping | Chain dependencies |

### 4.2 Stress Tests
- 1000+ resources with chain dependencies
- Memory pressure with many large textures
- Rapid compilation/deallocation cycles

- Concurrent graph compilation

### 4.3 Memory Leak Detection
- Run with AddressSanitizer or Valgrind
- Create/destroy CompiledGraph 100+ times
- Monitor GPU memory usage

- Check for leaked Vulkan objects

### 4.4 Benchmarks
- Compile time for various graph sizes
- Aliasing analysis time vs. resource count
- Barrier generation time vs. pass count

### 4.5 Files to Create/Modify
| File | Action | Changes |
|------|--------|---------|
| tests/production_scenarios.rs | CREATE | Real-world test implementations |
| tests/stress_tests.rs | CREATE | Stress test implementations |
| tests/memory_leak_tests.rs | CREATE | Leak detection tests |
| tests/benchmarks.rs | CREATE | Performance benchmarks |

---

## API Changes Summary

### Breaking Changes
| Change | Migration |
|------|------------|
| ImageDescriptor.extent: Extent2D to Extent3D | Set depth: 1 |
| ImageDescriptor.mip_levels | NEW | Set to 1 |
| ImageDescriptor.array_layers | NEW | Set to 1 |

### Non-Breaking Additions
| Addition | Description |
|----------|-------------|
| SubresourceRange | Specify partial image transitions |
| PassBuilder::read_image_mip() | Read specific mip level |
| PassBuilder::read_image_layer() | Read specific array layer |
| profiling module | Performance profiling infrastructure |
| compile_graph_with_profiler() | Compile with profiling |

---

## Verification Checklist

### Subresource Tracking
- [ ] Unit tests pass for SubresourceRange operations
- [ ] Mipmap generation produces correct barriers
- [ ] Cubemap rendering tracks per-layer state
- [ ] Barrier coalescing works correctly

### Improved Aliasing
- [ ] Best-fit produces better packing than first-fit
- [ ] Memory type compatibility is respected
- [ ] Large graphs compile in reasonable time
- [ ] Memory savings improved by 10-30%

### Performance Profiling
- [ ] Profiling hooks work without crashes
- [ ] Metrics accurately reflect performance
- [ ] Memory tracking is accurate
- [ ] Chrome tracing export is valid

### Testing
- [ ] All real-world scenarios pass
- [ ] Stress tests complete without crashes
- [ ] No memory leaks detected
- [ ] Benchmarks show acceptable performance

---

## Risks and Edge Cases

### Subresource Tracking
- **Risk**: Complex state tracking increases memory overhead
- **Mitigation**: Use compact state representation, lazy initialization
- **Edge Case**: Transitions spanning multiple mips/layers with different states

### Improved Aliasing
- **Risk**: Best-fit may be slower for small graphs
- **Mitigation**: Fall back to first-fit for graphs under threshold (e.g., < 10 resources)
- **Edge Case**: Resources with no compatible memory types (cannot alias)

- **Mitigation**: Create separate groups with warning

### Performance Profiling
- **Risk**: Profiling overhead affects frame timing
- **Mitigation**: Use compile-time feature flag, minimal overhead design
- **Edge Case**: Thread safety in multi-threaded scenarios
- **Mitigation**: Document that profiler is not thread-safe, require external sync

### General
- **Risk**: API changes break existing code
- **Mitigation**: Provide migration guide, default values for new fields
- **Edge Case**: Empty graphs, single-pass graphs
- **Mitigation**: Handle gracefully with validation
