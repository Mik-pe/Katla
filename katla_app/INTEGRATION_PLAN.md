# Render Graph Integration Plan for katla_app

## Overview

This plan outlines how to integrate the render graph system with the katla_app application layer, transitioning from the current immediate-mode rendering to a declarative render graph approach.

## Current Architecture

### Current Rendering Flow
```
Application::resumed()
  └─> VulkanRenderer::init()
       ├─> Creates single render pass (color + depth)
       ├─> Creates swapchain framebuffers
       └─> Creates per-frame command buffers

WindowEvent::RedrawRequested
  ├─> renderer.swap_frames()
  ├─> world.update(dt)
  ├─> Get camera matrices (view/proj)
  ├─> renderer.get_commandbuffer_opaque_pass()
  │    └─> Begins command buffer
  │    └─> Begins render pass
  ├─> Loop through DrawableComponent entities
  │    └─> drawable.update(view, proj, dt)
  │    └─> drawable.draw(command_buffer)
  ├─> command_buffer.end_render_pass()
  ├─> command_buffer.end_command()
  └─> renderer.submit_frame(vec![&command_buffer])
```

### Issues with Current Approach
- **Manual command buffer management** - error-prone, verbose
- **Fixed render pass** - no flexibility for multi-pass rendering
- **Mixed responsibilities** - application manages too much Vulkan state
- **No automatic synchronization** - barriers must be managed manually if adding passes

## Target Architecture

### Desired Rendering Flow
```
Application::resumed()
  ├─> VulkanRenderer::init()
  └─> Build and compile render graph
       ├─> Create swapchain resources
       ├─> Add geometry pass
       └─> renderer.set_render_graph(compiled_graph)

WindowEvent::RedrawRequested
  ├─> world.update(dt)
  └─> renderer.render_frame()
       └─> Render graph executes all passes automatically
```

## Integration Strategy

### Phase 1: Minimal Integration (Current Behavior)

**Goal**: Replace current rendering with render graph while maintaining identical behavior.

**Steps**:

1. **Add render graph creation to Application::resumed()**

   In `katla_app/src/application/mod.rs`, after VulkanRenderer initialization:

   ```rust
   // After renderer initialization
   self.setup_render_graph();
   ```

2. **Create setup_render_graph() method**

   ```rust
   impl Application {
       fn setup_render_graph(&mut self) {
           let renderer = self.renderer.as_mut().unwrap();

           // Create render graph builder
           let mut graph_builder = RenderGraphBuilder::new();

           // Get swapchain extent
           let extent = renderer.frame_context.swapchain.get_extent();

           // Create swapchain resource (external image)
           let swapchain_resource = renderer.create_swapchain_resource(
               &mut graph_builder,
               0, // image_index - will be managed per-frame
           );

           // Add geometry pass
           graph_builder.add_pass("geometry_pass", |pass| {
               pass.write(swapchain_resource)
                   .clear_color(swapchain_resource, [0.3, 0.5, 0.3, 1.0])
                   .bind_point(PipelineBindPoint::Graphics)
                   .execute("geometry_pass", |ctx| {
                       // This closure will be called during execution
                       // Need to capture world and camera for drawing
                   });
           });

           // Compile graph
           let vulkan_context = renderer.context.clone();
           match graph_builder.build(&vulkan_context) {
               Ok(graph) => renderer.set_render_graph(graph),
               Err(e) => eprintln!("Failed to compile render graph: {:?}", e),
           }
       }
   }
   ```

3. **Capture world and camera in closures**

   **Challenge**: The execution closure needs access to the world and camera, but:
   - The render graph is built once in `resumed()`
   - World and camera change every frame
   - Need to either:
     - Store references in the renderer
     - Rebuild graph every frame (expensive)
     - Use mutable context stored in renderer

   **Solution**: Store frequently accessed data in VulkanRenderer:

   ```rust
   // In katla_vulkan/src/lib.rs
   pub struct VulkanRenderer {
       // ... existing fields ...
       pub world: Option<World>,
       pub camera: Option<Rc<RefCell<Camera>>>,
   }
   ```

4. **Update RedrawRequested handler**

   Replace current manual command buffer code with:

   ```rust
   WindowEvent::RedrawRequested => {
       let dt = self.timer.delta_time();

       // Update world
       self.world.update(dt);

       // Render using render graph
       if let Err(e) = self.renderer.as_mut().unwrap().render_frame() {
           eprintln!("Render error: {:?}", e);
       }
   }
   ```

5. **Implement drawing in execution closure**

   The closure needs to:
   - Get command buffer from PassExecutionContext
   - Get view/proj matrices from camera
   - Query drawable components from world
   - Draw each component

   ```rust
   .execute("geometry_pass", |ctx| {
       // Get renderer world and camera
       let world = &renderer.world.as_ref().unwrap();
       let camera = renderer.camera.as_ref().unwrap();

       // Get matrices
       let view = camera.borrow().get_view_mat(world);
       let proj = camera.borrow().get_proj_mat(world);

       // Draw all entities
       for (_, drawable) in world.query::<&mut DrawableComponent>() {
           drawable.0.update(&view, &proj, dt);
           drawable.0.draw(&ctx.command_buffer);
       }
   });
   ```

6. **Update VulkanRenderer::render_frame()**

   The current implementation in `lib.rs:270-377` already handles:
   - Swap chain image acquisition
   - Iterating through passes
   - Command buffer recording
   - Submission and present

   Ensure it correctly:
   - Uses per-frame swapchain image index
   - Creates external image resource per-frame
   - Executes passes with proper context

**Deliverables**:
- [ ] Application builds and runs with render graph
- [ ] Visual output identical to current rendering
- [ ] No regression in functionality
- [ ] Code is cleaner (less manual command buffer management)

---

### Phase 2: Multi-Pass Rendering

**Goal**: Demonstrate the power of render graphs by adding multiple passes.

**Steps**:

1. **Add depth pre-pass**

   ```rust
   // Create depth resource
   let depth_resource = graph_builder.add_resource(
       "depth",
       ResourceKind::Image {
           extent: Extent3D { width: extent.width, height: extent.height, depth: 1 },
           format: ImageFormat::D32Sfloat,
           usage: vec![ImageUsage::DepthStencilAttachment],
           samples: SampleCount::Sample1,
           tiling: ImageTiling::Optimal,
           initial_layout: ImageLayout::Undefined,
           final_layout: ImageLayout::DepthStencilAttachmentOptimal,
       },
   );

   // Add depth pre-pass
   graph_builder.add_pass("depth_prepass", |pass| {
       pass.write(depth_resource)
           .execute("depth_prepass", |ctx| {
               // Render depth only (optimization for complex scenes)
           });
   });

   // Geometry pass reads depth
   graph_builder.add_pass("geometry_pass", |pass| {
       pass.read(depth_resource)
           .write(swapchain_resource)
           .execute("geometry_pass", |ctx| {
               // Normal render pass, can use depth from pre-pass
           });
   });
   ```

2. **Add post-processing pass**

   ```rust
   // Create intermediate color resource
   let color_resource = graph_builder.add_resource(
       "color",
       ResourceKind::Image {
           extent: Extent3D { width: extent.width, height: extent.height, depth: 1 },
           format: ImageFormat::R8G8B8A8Srgb,
           usage: vec![ImageUsage::ColorAttachment, ImageUsage::Sampled],
           samples: SampleCount::Sample1,
           tiling: ImageTiling::Optimal,
           initial_layout: ImageLayout::Undefined,
           final_layout: ImageLayout::ShaderReadOnlyOptimal,
       },
   );

   // Geometry pass writes to color resource
   graph_builder.add_pass("geometry_pass", |pass| {
       pass.write(color_resource)
           .execute("geometry_pass", |ctx| {
               // Render scene to color resource
           });
   });

   // Post-processing pass samples color resource
   graph_builder.add_pass("postprocess_pass", |pass| {
       pass.read(color_resource)
           .write(swapchain_resource)
           .execute("postprocess_pass", |ctx| {
               // Apply tone mapping, bloom, etc.
           });
   });
   ```

**Deliverables**:
- [ ] Multi-pass rendering working
- [ ] Automatic barrier synchronization (if implemented)
- [ ] Demonstrates resource transitions

---

### Phase 3: ECS Integration

**Goal**: Integrate render graph with ECS systems for better organization.

**Steps**:

1. **Create render graph system**

   ```rust
   pub struct RenderGraphSystem {
       renderer: Rc<RefCell<VulkanRenderer>>,
   }

   impl System for RenderGraphSystem {
       fn update(&mut self, world: &mut World, delta_time: f32) {
           // The render graph execution is now a system
           if let Err(e) = self.renderer.borrow_mut().render_frame() {
               eprintln!("Render error: {:?}", e);
           }
       }
   }
   ```

2. **Filter entities by render pass**

   ```rust
   // Add component to tag entities for specific passes
   #[derive(Component)]
   pub struct ShadowCaster;

   #[derive(Component)]
   pub struct Transparent;

   // In shadow pass closure
   for (_, (drawable, _)) in world.query::<(&mut DrawableComponent, &ShadowCaster)>() {
       drawable.0.draw(&ctx.command_buffer);
   }

   // In transparent pass closure
   for (_, (drawable, _)) in world.query::<(&mut DrawableComponent, &Transparent)>() {
       drawable.0.draw(&ctx.command_buffer);
   }
   ```

**Deliverables**:
- [ ] Render graph execution as ECS system
- [ ] Pass-specific entity filtering
- [ ] Clean separation of concerns

---

## Implementation Considerations

### Data Flow for Closures

**Problem**: Execution closures need access to:
- World (for querying entities)
- Camera (for view/proj matrices)
- Delta time (for animations)
- Per-frame data (swapchain image index)

**Solutions**:

1. **Store in VulkanRenderer** (Simplest)
   ```rust
   pub struct VulkanRenderer {
       pub world: Option<World>,
       pub camera: Option<Rc<RefCell<Camera>>>,
       pub delta_time: Option<f32>,
   }
   ```
   - Pro: Easy to implement
   - Con: Couples renderer to application structures

2. **Pass through context** (Cleaner)
   ```rust
   pub struct FrameContext {
       pub world: &World,
       pub camera: &Camera,
       pub delta_time: f32,
   }

   // In execution closure
   .execute("geometry_pass", |ctx, frame_ctx| {
       for (_, drawable) in frame_ctx.world.query::<&DrawableComponent>() {
           drawable.0.draw(&ctx.command_buffer);
       }
   });
   ```
   - Pro: Clean separation
   - Con: Requires modifying ExecutionRegistry

3. **Capture in closure during graph build** (Problematic)
   ```rust
   // This captures by reference, but graph outlives the data
   let world = &self.world;
   graph_builder.add_pass("pass", |pass| {
       pass.execute("pass", |ctx| {
           // World reference is invalid here!
       });
   });
   ```
   - ❌ Doesn't work due to lifetimes

**Recommended**: Store references in VulkanRenderer for Phase 1, refactor to context approach in Phase 3.

### Resource Management

**Current**: Materials and textures manage their own Vulkan resources.

**With Render Graph**: Need to integrate existing resources as external resources.

**Approach**:
```rust
// For existing textures
let texture_resource = graph_builder.add_resource(
    "albedo_texture",
    ResourceKind::ExternalImage {
        vk_image: texture.vk_image,
        image_view: texture.image_view,
        format: ImageFormat::R8G8B8A8Srgb,
        extent: texture.extent,
    },
);
```

### Swapchain Integration

**Current**: `VulkanRenderer::create_swapchain_resource()` exists but needs per-frame handling.

**Issue**: Swapchain has multiple images (usually 2-3), need to select correct one per frame.

**Solution**:
```rust
// In render_frame(), before executing passes:
let frame_data = self.current_framedata.as_ref().unwrap();
let image_index = frame_data.image_index as usize;

// Update the external image resource with current swapchain image
// (This requires mutating the graph's resources)
```

Alternatively: Create one render graph per swapchain image (simpler but uses more memory).

### Barrier Synchronization

**Current**: `calculate_barriers()` returns empty vectors.

**Impact**: Multi-pass graphs may have synchronization issues.

**Workaround**: For Phase 1 (single pass), no barriers needed. For Phase 2 (multi-pass), either:
1. Implement barrier calculation
2. Add manual barriers in execution closures
3. Use external synchronization

---

## Testing Strategy

1. **Phase 1 Testing**:
   - Compare screenshots before/after integration
   - Verify frame timing is similar
   - Check for memory leaks (Valgrind/dr. memory)

2. **Phase 2 Testing**:
   - Verify depth pre-pass improves performance (should help with complex scenes)
   - Test post-processing passes
   - Validate barriers work correctly (Vulkan validation layers)

3. **Phase 3 Testing**:
   - Ensure system execution order is correct
   - Test entity filtering
   - Verify no performance regression

---

## Migration Checklist

### Phase 1: Minimal Integration
- [x] Modify `Application::resumed()` to call `setup_render_graph()`
- [x] Implement `Application::render_with_render_graph()` method
- [x] Add `world` and `camera` access via method closure capture
- [x] Update `RedrawRequested` handler to use render graph
- [x] Ensure drawing works with render graph
- [x] Test: Application runs without validation errors ✅
- [x] Fix validation errors:
  - [x] Fixed command buffer usage flags (use `default()` instead of `ONE_TIME_SUBMIT`)
  - [x] Fixed submit to only use the recorded command buffer
  - [x] Added wait dst stage mask for semaphore synchronization

**Status**: Phase 1 COMPLETE ✅

### Known Technical Debt

**⚠️ Ash Dependency in katla_app**:
- The original codebase had `pub use ash::vk;` in katla_vulkan/lib.rs
- This implicitly exposed all ash types to downstream crates
- We removed the re-export but katla_app still needs ash types for:
  - `Texture::create_image()` takes `vk::Format`
  - `Material` and mesh code use `vk::IndexType`
- **Current solution**: Added explicit ash dependency to katla_app with TODO comment
- **Proper fix**: See `katla_vulkan/WRAPPER_TYPES_PLAN.md` for detailed implementation plan
- **Note**: This is pre-existing technical debt, not introduced by render graph integration

### Future Cleanup

The proper fix for the dependency violation is documented in:
**`katla_vulkan/WRAPPER_TYPES_PLAN.md`**

Key points:
1. Create wrapper types for Format, IndexType, Rect2D, etc.
2. Update Texture, IndexBuffer, and CommandBuffer APIs
3. Remove ash dependency from katla_app
4. Ensure katla_vulkan's public API doesn't expose ash types

Estimated effort: 2-3 days for complete implementation.

### Phase 2: Multi-Pass Rendering
- [ ] Add depth pre-pass
- [ ] Implement barrier synchronization (or manual barriers)
- [ ] Add post-processing pass
- [ ] Test: Multiple passes execute correctly
- [ ] Profile: Performance impact

### Phase 3: ECS Integration
- [ ] Create `RenderGraphSystem`
- [ ] Add pass-specific components
- [ ] Filter entities by pass
- [ ] Test: Clean architecture

---

## Open Questions

1. **How to handle swapchain multi-image with render graph?**
   - Option A: Update graph resources per-frame
   - Option B: Create one graph per swapchain image
   - Option C: Use resource aliasing

2. **Should execution closures be rebuildable?**
   - Current: Built once in `resumed()`
   - Alternative: Rebuild when world/camera changes
   - Impact: Performance vs flexibility

3. **How to integrate existing texture/material system?**
   - Current: Manages own Vulkan resources
   - With render graph: Should use external resources
   - Migration: May require refactoring Material/Texture

4. **When to implement barrier calculation?**
   - Phase 1: Not needed (single pass)
   - Phase 2: Needed for multi-pass
   - Priority: Can use manual barriers initially

---

## Success Criteria

**Phase 1 Success**:
- Application renders identically to current behavior
- Code is cleaner and more maintainable
- No performance regression
- All existing functionality works

**Phase 2 Success**:
- Multi-pass rendering works correctly
- Demonstrates advantage of render graph (flexibility)
- Performance is acceptable (or improved with depth pre-pass)

**Phase 3 Success**:
- Clean ECS integration
- Pass-specific entity filtering works
- Architecture is scalable for future features

---

## Next Steps

1. **Review this plan** and adjust based on discussion
2. **Start Phase 1 implementation** - focus on minimal integration
3. **Test thoroughly** before moving to Phase 2
4. **Iterate** based on what we learn during implementation
