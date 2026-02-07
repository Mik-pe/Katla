# Technical Debt Report

Generated: 2026-02-07
Last Updated: 2026-02-07

## Completed Fixes

### unwrap() Call Improvements (Completed 2026-02-07)

Fixed critical unwrap() calls with better error handling:

**katla_ecs/src/storage.rs**
- Added safety documentation explaining why downcast expect() calls are sound
- Improved error messages from "Downcast should succeed" to "TypeId lookup ensures correct type, downcast cannot fail"

**katla_app/src/entities/model.rs**
- Removed unnecessary Option wrapper on mesh_handle and material_handle
- Handles are now always present (MeshHandle(0) or MaterialHandle(0) when no renderer)
- Eliminated unwrap() calls on lines 60-61

**katla_vulkan/src/vulkan/commandbuffer.rs**
- Changed unwrap() to expect() with descriptive messages:
  - "Failed to allocate Vulkan command buffer - check device memory"
  - "Failed to begin command buffer - command buffer may be in invalid state"
  - "Failed to end command buffer - command buffer may not be in recording state"

**Note**: Many remaining unwrap() calls are sound (invariants guarantee success). They've been documented with expect() messages explaining the invariants rather than being removed.

## Critical Priority Issues

### 1. Ash Type Exposures in Public APIs

**Status**: Documented, requires major refactoring

**Rule**: `katla_vulkan` must NOT export or re-export `ash::vk` types in its public API.

**Violations Found**:

#### lib.rs
- `FrameData` struct (line 19-24) exposes: `vk::Semaphore`, `vk::Fence`

#### render_graph/compiled.rs
- `CompiledPass` struct (line 34-40) exposes: `vk::RenderPass`, `vk::Framebuffer`, `vk::Extent2D`, `vk::ClearValue`

#### render_graph/resource.rs
- `CompiledResource` variants and `ResourceAccessType` (line 48-51) expose: `vk::AccessFlags`, `vk::PipelineStageFlags`, `vk::ImageLayout`, `vk::AttachmentLoadOp`

#### render_graph/pass.rs
- `PassExecutionContext` struct (line 243-247) exposes: `vk::Framebuffer`, `vk::RenderPass`
- `PassExecutionContext::get_image()` (line 276) returns: `(vk::Image, vk::ImageView)`

#### vulkan/context.rs
- `RenderTexture` struct (line 32-36) exposes: `vk::Extent2D`, `vk::ImageView`, `vk::Format`, `vk::Image`
- `VulkanFrameCtx` struct (line 77-81) exposes: `vk::ImageView`, `vk::Image`
- `VulkanContext::destroy_framebuffer()` (line 256) takes: `vk::Framebuffer`

#### vulkan/swapchain.rs
- `SwapchainInfo` struct (line 9-13) exposes: `vk::SurfaceCapabilitiesKHR`, `vk::SurfaceFormatKHR`, `vk::PresentModeKHR`
- `Swapchain` struct (line 15-20) exposes: `vk::SwapchainKHR`, `vk::SurfaceFormatKHR`

#### vulkan/texture.rs
- `Texture` struct (line 17-18) exposes: `vk::ImageView`, `vk::Sampler`

#### vulkan/renderpass.rs
- `RenderPass::get_vk_renderpass()` (line 76) returns: `vk::RenderPass`

#### vulkan/framebuffer.rs
- `Framebuffer::vk_framebuffer()` (line 40) returns: `vk::Framebuffer`

#### vulkan/commandbuffer.rs
- `CommandBuffer::vk_command_buffer()` (line 28) returns: `vk::CommandBuffer`
- `CommandBuffer::begin_command()` (line 48) takes: `vk::CommandBufferUsageFlags`

#### vulkan/material/builder.rs
- `PipelineBuilder::with_descriptor_layouts()` (line 172) takes: `Vec<vk::DescriptorSetLayout>`
- `PipelineBuilder::build()` (line 202) takes: `vk::RenderPass`
- `Pipeline` struct (line 322-323) exposes: `vk::Pipeline`, `vk::PipelineLayout`

#### vulkan/material/mod.rs
- `ImageInfo` struct (line 41-42) exposes: `vk::ImageView`, `vk::Sampler`
- `ImageInfo::new()` (line 68) takes: `vk::ImageView`, `vk::Sampler`
- `UniformDescriptor` struct (line 60-61) exposes: `vk::DescriptorSet`, `vk::DescriptorPool`
- `UniformHandle` methods (lines 124-141) take: `&vk::DescriptorSetLayout`
- `UniformLayout::build()` (line 343) returns: `Result<vk::DescriptorSetLayout, vk::Result>`
- `MaterialPipeline` struct (line 358) exposes: `vk::DescriptorSetLayout`
- `MaterialPipeline::bind()` (line 425) takes: `vk::CommandBuffer`

**Action Plan**:
1. Create wrapper types module (e.g., `katla_vulkan::types`) with wrappers for:
   - `Semaphore`, `Fence`
   - `RenderPass`, `Framebuffer`
   - `ImageView`, `Sampler`, `Image`
   - `CommandBuffer`
   - `Pipeline`, `PipelineLayout`
   - `DescriptorSet`, `DescriptorPool`, `DescriptorSetLayout`
   - `SwapchainKHR` and related types
2. Implement `From<Wrapper> for vk::Type` and `From<vk::Type> for Wrapper` for conversions
3. Replace all public API uses of vk types with wrappers
4. Keep vk types internal to implementation modules
5. Update all dependent code

**Estimated Effort**: 2-3 days

---

### 2. Unsafe unwrap() Calls

**Status**: Partially documented, requires systematic fixing

**Total Found**: 66+ instances

**Critical Locations**:

#### katla_ecs/src/storage.rs
- Lines 215, 226, 237: `expect()` used for downcasting in `get_storage_mut()`
  - Impact: Will panic on type mismatch instead of returning error
  - Fix: Return `Option` or `Result`

#### katla_app/src/entities/model.rs
- Lines 60-61: `unwrap()` on mesh and material handles
  - Impact: Will panic if handles are None
  - Fix: Handle None case properly or document invariant

#### katla_vulkan/src/vulkan/commandbuffer.rs
- Line 19: Unsafe allocation with `unwrap()`
  - Impact: Could panic on allocation failure
  - Fix: Proper error handling

**Systematic Fixes Needed**:

1. **Replace `unwrap()` with `?` operator** where appropriate
2. **Use `expect()` with descriptive messages** for truly invariant conditions
3. **Return `Result<T, E>`** for recoverable errors
4. **Document invariants** that justify `unwrap()` usage

**Files with Most unwrap() Calls**:
- `katla_vulkan/src/lib.rs`: 12 instances
- `katla_app/src/application/mod.rs`: 8 instances
- `katla_vulkan/src/vulkan/context.rs`: 6 instances
- `katla_vulkan/src/rendering/registry.rs`: 5 instances

**Estimated Effort**: 1-2 days

---

## High Priority Issues

### 3. Incomplete Animation System

**Files**:
- `katla_app/src/animation/systems.rs`
- `katla_app/src/animation/gltf_loader.rs`

**Incomplete Implementations**:

#### systems.rs
- Line 76: TODO: Check parent/child relationships (returns None)
- Line 94: TODO: Implement skeletal animation system (placeholder)
- Line 116: TODO: Implement morph target animation system (placeholder)

#### gltf_loader.rs
- Line 9: TODO: Full implementation requires proper gltf crate accessor usage
- Line 62: TODO: Create AnimatedModel component (only logs)
- Line 95: TODO: Store skin data properly (only prints)
- Line 109: TODO: Extract node transforms from GLTF scene graph (returns identity)

**Impact**: Animation features are advertised but not functional

**Estimated Effort**: 3-5 days

---

### 4. Missing Test Coverage

**Critical Areas Without Tests**:
- **katla_ecs**: No tests for system execution, entity lifecycle, component queries
- **katla_vulkan**: No tests for resource management, render graph compilation
- **katla_app**: No tests for application flow, entity creation, rendering
- **Animation system**: Complete lack of coverage
- **Physics integration**: No tests

**Current Test Coverage**: ~30% (mostly basic unit tests for math and data structures)

**Estimated Effort**: 1-2 weeks

---

## Medium Priority Issues

### 5. Code Duplication

#### Builder Pattern Duplication
- Cube, Sphere, Cylinder, Torus builders have nearly identical implementations
- Only differences are parameters and geometry generation
- **Fix**: Create trait or macro for common builder functionality

#### Material Creation Patterns
- Similar patterns repeated across different components
- Duplicate descriptor set creation logic
- **Fix**: Extract common functions

**Estimated Effort**: 2-3 days

---

### 6. Performance Issues

#### Inefficient Operations
- `katla_math/src/quat.rs` line 157: Commented "slow version" of slerp
- `katla_vulkan/src/vulkan/texture.rs` line 235: Synchronous command buffer submission
- `katla_vulkan/src/vulkan/context.rs` line 344: Multi-threading not fully utilized

#### Memory Management
- Staging buffers created/destroyed per upload
- Potential memory leaks in command buffer management

**Estimated Effort**: 3-5 days

---

### 7. Compiler Warnings

**Unused Code** (Run `cargo clippy` for full list):
- `katla_derive/src/lib.rs`: Unused import `ash::vk`
- Multiple unused builder methods
- Unused struct fields in components
- Unused APIs: `get_mesh_mut`, `get_material`, `remove_mesh`, `remove_material`, `create_colored_checkerboard_material`

**Fix**: Remove unused code or mark with `#[allow(dead_code)]` with justification

**Estimated Effort**: 1 day

---

## Low Priority Issues

### 8. Documentation

- Missing documentation on many public functions
- No usage examples for key APIs
- Complex algorithms lack implementation notes

**Estimated Effort**: 3-5 days

---

## Recommended Action Order

### Immediate (This Week)
1. **Fix critical unwrap() calls** in core systems (ECS storage, rendering)
2. **Remove unused imports** and fix compiler warnings

### High Priority (Next 2 Weeks)
3. **Complete animation system TODOs** for feature completeness
4. **Add tests for critical paths** (ECS queries, render graph compilation)

### Medium Priority (Next Month)
5. **Refactor builder patterns** to reduce duplication
6. **Optimize texture loading** (async upload)
7. **Fix ash::vk type exposures** (start with high-traffic APIs)

### Low Priority (Ongoing)
8. **Improve documentation** as features are added
9. **Performance profiling** and optimization

---

## Metrics

- **Total Issues Documented**: 8 major categories
- **Critical Violations**: 2 (ash types, unwrap calls)
- **Incomplete Features**: 1 (animation system)
- **Test Coverage**: ~30%
- **Compiler Warnings**: 16+

---

## Notes

- This report focuses on actionable technical debt
- Some violations (like ash::vk types) were inherited from earlier development
- Prioritization based on: stability risk, feature impact, and fix effort
- Regular updates recommended as codebase evolves
