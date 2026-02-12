---
name: render-graph-validator
description: Validate render graph resource lifetime analysis, pass dependencies, barrier insertion, and ExternalImage/ExternalBuffer usage. Use when implementing new render passes or debugging render graph issues.
allowed-tools: Read, Grep, Glob, Bash
---

# Render Graph Validator

## Overview

This skill validates render graph implementation in `katla_vulkan/src/render_graph/` to ensure correct resource lifetime management, barrier synchronization, and external resource usage.

## Render Graph Architecture

### Key Modules

- **types.rs** - Wrapper types for Vulkan (ImageFormat, ImageLayout, Extent2D/3D)
- **resource.rs** - ResourceKind (Buffer, Image, ExternalBuffer, ExternalImage), ResourceUsage
- **graph.rs** - RenderGraph builder and resource management
- **pass.rs** - PassBuilder, Pass, PassExecutionContext, ExecutionRegistry
- **compiled.rs** - CompiledRenderGraph, compilation pipeline

### Execution Flow

1. User builds graph with `RenderGraphBuilder::new()`
2. Add resources: `add_resource(name, ResourceKind)` → returns `ResourceId`
3. Add passes: `add_pass(name, |builder| { ... })`
4. Builder stores closures in `ExecutionRegistry` (avoids trait object lifetime issues)
5. `build(context)` → `CompiledRenderGraph` (transfers registry ownership)
6. `execute(command_buffer)` runs all passes with closure lookup

## Validation Checks

### 1. Resource Lifetime Analysis

**Check:** Resources are not used before creation or after destruction.

```bash
# Find resource creation
grep -rn "add_resource" katla_vulkan/src/render_graph/

# Find resource reads
grep -rn "\.read(" katla_vulkan/src/render_graph/

# Find resource writes
grep -rn "\.write(" katla_vulkan/src/render_graph/

# Find resource imports
grep -rn "import_resource\|ExternalImage\|ExternalBuffer" katla_vulkan/src/render_graph/
```

**Common Lifetime Issues:**

1. **Read before Write**: Reading a resource that hasn't been written yet
2. **Write after Write**: Multiple writes to same resource without synchronization
3. **Use after Free**: Using resource after its last pass
4. **External Resource Misuse**: External resources used with incorrect initial/final layout

### 2. Pass Dependency Validation

**Check:** Pass dependencies are correctly specified for read-after-write and write-after-read hazards.

```bash
# Find pass reads
grep -rn "PassBuilder.*read" katla_vulkan/src/render_graph/pass.rs

# Find pass writes
grep -rn "PassBuilder.*write" katla_vulkan/src/render_graph/pass.rs

# Check for dependency tracking
grep -rn "depends_on\|dependency\|barrier" katla_vulkan/src/render_graph/
```

**Dependency Rules:**

- **Pass A reads X, Pass B writes X**: B must execute after A (RAW hazard)
- **Pass A writes X, Pass B reads X**: B must execute after A (WAR hazard)
- **Pass A writes X, Pass B writes X**: B must execute after A (WAW hazard)
- **No data dependency**: Passes can execute in parallel

### 3. Barrier Synchronization

**Check:** Appropriate pipeline barriers are inserted for image layout transitions and buffer access.

```bash
# Find barrier insertion points
grep -rn "pipeline_barrier\|PipelineBarrier\|image_barrier\|buffer_barrier" \
  katla_vulkan/src/render_graph/

# Check image layout transitions
grep -rn "ImageLayout::\|old_layout\|new_layout" katla_vulkan/src/render_graph/

# Find src_stage_mask and dst_stage_mask
grep -rn "src_stage\|dst_stage\|PipelineStageFlags" katla_vulkan/src/render_graph/
```

**Barrier Requirements:**

- **Dynamic Rendering**: Katla uses `vkCmdBeginRendering`/`vkCmdEndRendering` (Vulkan 1.3), NOT traditional render passes
- **Synchronization2**: Barriers use `ImageMemoryBarrier2` and `DependencyInfo` with `vkCmdPipelineBarrier2`
- **Image Layout Transition**: Need barrier when layout changes (e.g., Undefined → TransferDstOptimal)
- **Buffer Access**: Need barrier when access type changes (e.g., TransferWrite → ShaderRead)
- **Queue Family Transfer**: Need barrier when transferring ownership between queues

**Important**: The render graph automatically inserts Synchronization2 barriers for layout transitions. When using external resources (swapchain, depth), ensure correct `initial_layout` and `final_layout` are specified.

**Common Barrier Issues:**

1. **Missing Barrier**: Layout transition without barrier → undefined behavior
2. **Wrong Stage Masks**: Source/destination stages don't match actual usage
3. **Incorrect Access Masks**: Access flags don't match operations
4. **Missing Queue Family Transfer**: Transferring between queues without ownership transfer

### 4. External Resource Validation

**Check:** ExternalImage and ExternalBuffer are used correctly with proper layouts.

```bash
# Find ExternalImage usage
grep -rn "ExternalImage\|ExternalBuffer" katla_vulkan/src/render_graph/

# Check initial_layout and final_layout specification
grep -rn "initial_layout\|final_layout" katla_vulkan/src/render_graph/

# Find external resource imports
grep -rn "import_resource\|external" katla_vulkan/src/render_graph/
```

**External Resource Rules:**

- **ExternalImage**: Must specify initial_layout (current layout before render graph)
- **ExternalImage**: Must specify final_layout (desired layout after render graph)
- **ExternalBuffer**: Must specify current usage state
- **Initial Layout**: Barrier inserted at graph start to transition to first use
- **Final Layout**: Barrier inserted at graph end to transition to final state

**Common External Resource Issues:**

1. **Undefined Initial Layout**: External image starts as Undefined → validation error
2. **Incorrect Final Layout**: Layout after graph doesn't match external expectations
3. **Missing External Declaration**: Using swapchain without ExternalImage
4. **Wrong Initial Layout**: External resource not in expected state at graph start

## Render Graph Usage Patterns

**Note**: The Katla engine uses **Dynamic Rendering** (Vulkan 1.3) via the render graph. Traditional render pass objects are NOT used.

### Creating a Render Graph

```rust
let mut graph_builder = RenderGraphBuilder::new();

// Add external resource (swapchain - managed externally)
let swapchain_resource = graph_builder.add_resource(
    "swapchain",
    ResourceKind::ExternalImage {
        vk_image: swapchain_image,
        image_view: swapchain_image_view,
        format: vk::Format::B8G8R8A8_SRGB,
        extent: vk::Extent2D { width: 1920, height: 1080 },
    },
);

// Add external depth resource
let depth_resource = graph_builder.add_resource(
    "depth",
    ResourceKind::ExternalImage {
        vk_image: depth_image,
        image_view: depth_image_view,
        format: vk::Format::D32_SFLOAT,
        extent: vk::Extent2D { width: 1920, height: 1080 },
    },
);
```

### Adding Passes with Dynamic Rendering

```rust
graph_builder.add_pass("geometry_pass", |pass| {
    pass.write(Attachment::Color(swapchain_resource))
        .write(Attachment::DepthStencil(depth_resource))
        .clear_color(swapchain_resource, [0.3, 0.5, 0.3, 1.0])
        .clear_depth_stencil(depth_resource, 1.0, 0)
        .execute("geometry_pass", |ctx| {
            // Record rendering commands
            // Dynamic rendering already active (vkCmdBeginRendering called)
            let cmd_buf = ctx.command_buffer;
            // ... draw commands ...
        });
});
```

### Building and Executing

```rust
let graph = graph_builder.build(&vulkan_context)?;
// Execute with dynamic rendering (uses vkCmdBeginRendering internally)
graph.execute(&mut command_buffer, image_index, swapchain_images, depth_image)?;
```

**Key Points:**
- Use `ExternalImage` for resources created outside the graph (swapchain, depth)
- Use `Attachment::Color` and `Attachment::DepthStencil` to specify attachment types
- Dynamic rendering is automatic - no need to call `begin_rendering()` manually
- Synchronization2 barriers are inserted automatically for layout transitions

## Common Render Graph Bugs

### Bug 1: Missing Read Dependency

**Problem:** Pass B reads resource written by Pass A, but no dependency specified.

```rust
// WRONG: No dependency, might execute in parallel
graph_builder.add_pass("pass_a", |pass| {
    pass.write(resource1).execute("a", |ctx| { /* ... */ });
});

graph_builder.add_pass("pass_b", |pass| {
    pass.read(resource1).execute("b", |ctx| { /* ... */ });
});

// CORRECT: Dependency inferred from read/write
// The render graph should automatically detect pass_b depends on pass_a
```

**Detection:**
```bash
# Check if dependency tracking exists
grep -rn "track_dependency\|register_dependency\|add_edge" \
  katla_vulkan/src/render_graph/graph.rs
```

### Bug 2: Incorrect Image Layout

**Problem:** Image used in wrong layout without barrier.

```rust
// WRONG: Use image in ColorAttachmentOptimal layout as shader input
// after rendering
graph_builder.add_pass("compute_pass", |pass| {
    pass.read(color_target)  // Still in ColorAttachmentOptimal!
        .execute("compute", |ctx| {
            // Reading from ColorAttachmentOptimal as sampled image
        });
});

// CORRECT: Layout transition happens automatically
// Render graph should insert barrier:
// - src_stage: COLOR_ATTACHMENT_OUTPUT
// - dst_stage: COMPUTE_SHADER
// - old_layout: ColorAttachmentOptimal
// - new_layout: ShaderReadOnlyOptimal
```

**Detection:**
```bash
# Check for automatic barrier insertion
grep -rn "insert_barrier\|add_barrier\|transition_layout" \
  katla_vulkan/src/render_graph/
```

### Bug 3: External Resource Undefined

**Problem:** ExternalImage used without specifying initial layout.

```rust
// WRONG: ExternalImage doesn't specify initial_layout
ResourceKind::ExternalImage {
    external_handle: swapchain_image,
    // missing initial_layout!
    final_layout: ImageLayout::PresentSrcKHR,
}

// CORRECT: Specify current state
ResourceKind::ExternalImage {
    external_handle: swapchain_image,
    initial_layout: ImageLayout::Undefined,  // Current state
    final_layout: ImageLayout::PresentSrcKHR,  // Desired state
}
```

**Detection:**
```bash
# Find ExternalImage definitions
grep -rn "ExternalImage {" katla_vulkan/src/render_graph/
# Verify all have initial_layout and final_layout
```

### Bug 4: Resource Lifetime Violation

**Problem:** Resource used after its last write pass.

```rust
// WRONG: Using resource outside its lifetime
let depth = graph_builder.add_resource("depth", /* ... */);

graph_builder.add_pass("shadow_pass", |pass| {
    pass.write(depth).execute("shadow", |ctx| { /* ... */ });
});

// depth is now "dead" - no more passes use it
// but trying to use it later would be an error
```

**Detection:**
```bash
# Check if lifetime analysis is implemented
grep -rn "lifetime\|liveness\|dead_code_elimination" \
  katla_vulkan/src/render_graph/compiled.rs
```

## Render Graph Validation Checklist

### Resource Management
- [ ] All internal resources have extent, format, usage specified
- [ ] All external resources have initial_layout and final_layout
- [ ] Resource IDs are unique (no duplicate names)
- [ ] Resources are not read before first write
- [ ] Resources are not used after last pass

### Pass Dependencies
- [ ] Read-after-write dependencies tracked
- [ ] Write-after-read dependencies tracked
- [ ] Write-after-write dependencies tracked
- [ ] No circular dependencies between passes
- [ ] Passes with no dependencies can execute in parallel

### Synchronization
- [ ] Image layout transitions have barriers
- [ ] Buffer access changes have barriers
- [ ] Barrier src_stage_mask matches producing stage
- [ ] Barrier dst_stage_mask matches consuming stage
- [ ] Queue family transfers have ownership transfer barriers

### External Resources
- [ ] ExternalImage has valid initial_layout
- [ ] ExternalImage has valid final_layout
- [ ] ExternalBuffer has valid current state
- [ ] Initial layout matches actual resource state at graph start
- [ ] Final layout matches expected resource state after graph

### Execution
- [ ] ExecutionRegistry stores closures correctly
- [ ] CompiledRenderGraph transfers registry ownership
- [ ] execute() looks up closures by name
- [ ] Command buffer recording happens in correct order
- [ ] Framebuffers created correctly per pass

## Debugging Render Graph Issues

### Enable Validation Layers

```bash
export VK_LAYER_KHRONOS_VALIDATION=1
export VK_LAYER_FLAGS_KHRONOS_VALIDATION=best-practices
cargo run 2>&1 | tee validation.txt
```

### Check for Common Validation Errors

1. **Image Layout Mismatch**: "VUID-VkImageMemoryBarrier-oldLayout-01197"
   - Image used in layout different from barrier specification

2. **Invalid Stage Mask**: "VUID-VkImageMemoryBarrier-srcStageMask-03937"
   - Pipeline stage doesn't match operations performed

3. **Missing Synchronization**: "UNASSIGNED-CoreValidation-DrawState-InvalidImageLayout"
   - Layout transition without proper barrier

4. **Descriptor Set Mismatch**: "UNASSIGNED-CoreValidation-Shader-InputNotProduced"
   - Pass reading resource that wasn't written

### Add Debug Logging

```rust
// In render_graph/compiled.rs
impl CompiledRenderGraph {
    pub fn execute(&self, command_buffer: &mut CommandBuffer) -> Result<()> {
        println!("=== Render Graph Execution ===");
        for pass in &self.passes {
            println!("Executing pass: {}", pass.name);
            // ...
        }
        println!("=== Execution Complete ===");
    }
}
```

## Integration with Application

### External Resources for Swapchain

```rust
// In katla_app/src/rendering/
let swapchain_images = renderer.get_swapchain_images();

for (i, image) in swapchain_images.iter().enumerate() {
    let external_image = graph_builder.import_resource(
        &format!("swapchain_{}", i),
        ResourceKind::ExternalImage {
            external_handle: image.handle,
            extent: Extent3D { /* ... */ },
            format: ImageFormat::R8G8B8A8Srgb,
            initial_layout: ImageLayout::Undefined,
            final_layout: ImageLayout::PresentSrcKHR,
        },
    );
}
```

### Per-Material Uniform Buffers

```rust
// ExternalBuffer for uniform buffers
let uniform_buffer = graph_builder.import_resource(
    "camera_uniforms",
    ResourceKind::ExternalBuffer {
        external_handle: buffer.handle,
        size: 256,
        usage: BufferUsage::UniformBuffer,
    },
);
```

## Code Review Checklist for Render Graph

- [ ] Resources added with add_resource() have all required fields
- [ ] External resources have initial_layout and final_layout
- [ ] Passes use read()/write() to declare resource access
- [ ] No duplicate resource names
- [ ] Resource lifetimes don't overlap incorrectly
- [ ] Barrier insertion handles all layout transitions
- [ ] External resource layouts match actual Vulkan state
- [ ] Closure lookup in execute() is correct
- [ ] Framebuffer creation respects resource dimensions
- [ ] Validation layers run without errors

## Resources

- `katla_vulkan/src/render_graph/Plan.md` - Render graph integration plan
- `katla_vulkan/src/render_graph/types.rs` - Wrapper types
- `CLAUDE.md` - Render graph architecture documentation
