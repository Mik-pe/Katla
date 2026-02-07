# Ash::vk Type Exposure Analysis

## Current State Summary

### ✅ Already Wrapped (Public APIs working correctly)
1. **FrameData** - Uses `VkSemaphore`, `VkFence`
2. **Texture** - Uses `VkImage`, `VkImageView`, `VkSampler`
3. **CompiledPass** - Uses `VkRenderPass`, `VkFramebuffer` (partially)
4. **PassExecutionContext** - Uses `VkRenderPass`, `VkFramebuffer` (partially)

### ⚠️ Issues Found

## 1. PassExecutionContext.extent
**File:** `render_graph/pass.rs:250`
```rust
pub struct PassExecutionContext {
    pub command_buffer: std::rc::Rc<CommandBuffer>,
    pub resources: std::rc::Rc<std::collections::HashMap<ResourceId, CompiledResource>>,
    pub framebuffer: VkFramebuffer,     // ✅ Wrapped
    pub render_pass: VkRenderPass,      // ✅ Wrapped
    pub subpass: u32,
    pub extent: vk::Extent2D,           // ❌ NOT wrapped
}
```

**Problem:** Uses `vk::Extent2D` but there's already a `Extent2D` wrapper in `render_graph/types.rs`!

**Impact:** Public API exported in lib.rs. Not used by katla_app, but users could be exposed.

**Better approach:** Use the existing `Extent2D` wrapper from types.rs.

---

## 2. CompiledPass Structure
**File:** `render_graph/compiled.rs:33-44`
```rust
pub struct CompiledPass {
    pub name: String,
    pub vk_render_pass: VkRenderPass,              // ✅ Wrapped
    pub active_render_pass: VkRenderPass,          // ✅ Wrapped
    pub vk_framebuffers: Vec<VkFramebuffer>,       // ✅ Wrapped
    pub extent: vk::Extent2D,                      // ❌ NOT wrapped
    pub clear_values: Vec<vk::ClearValue>,         // ❌ NOT wrapped
    execute: PassExecute,
    pub pipeline_barriers_before: Vec<vk::MemoryBarrier<'static>>, // ❌ Vulkan type
}
```

**Problems:**
- `extent` uses `vk::Extent2D` instead of wrapper `Extent2D`
- `clear_values` uses `vk::ClearValue` instead of wrapper `ClearValue` (exists in types.rs!)
- `pipeline_barriers_before` is a complex Vulkan type

**Impact:** Public struct, but only used internally within render_graph compilation.

**Better approach:** Use wrapper types for `extent` and `clear_values`. Consider whether `pipeline_barriers_before` should be public.

---

## 3. ResourceUsage Structure
**File:** `render_graph/resource.rs:46-54`
```rust
pub struct ResourceUsage {
    pub resource_id: ResourceId,
    pub access: vk::AccessFlags,           // ❌ NOT wrapped
    pub stage: vk::PipelineStageFlags,     // ❌ NOT wrapped
    pub layout: vk::ImageLayout,           // ❌ NOT wrapped
    pub load_op: vk::AttachmentLoadOp,     // ❌ NOT wrapped
    pub store_op: vk::AttachmentStoreOp,   // ❌ NOT wrapped
    pub clear_value: Option<vk::ClearValue>, // ❌ NOT wrapped
}
```

**Problems:** All fields expose raw Vulkan types, but wrapper types exist (`Access`, `PipelineStage`, `ImageLayout`, `AttachmentLoadOp`, `AttachmentStoreOp`, `ClearValue`).

**Note:** Already has builder methods that accept wrapper types:
```rust
pub fn with_read(mut self, access: super::types::Access, stage: super::types::PipelineStage) -> Self
pub fn with_write(mut self, access: super::types::Access, stage: super::types::PipelineStage) -> Self
pub fn with_layout(mut self, layout: super::types::ImageLayout) -> Self
pub fn with_load_op(mut self, load_op: super::types::AttachmentLoadOp) -> Self
pub fn with_store_op(mut self, store_op: super::types::AttachmentStoreOp) -> Self
pub fn with_clear_value(mut self, clear_value: super::types::ClearValue) -> Self
```

**Impact:** Exported in lib.rs. Not used by katla_app. Internal fields accessed in compiled.rs.

**Better approaches:**
1. Make fields private, provide getter methods that return wrapper types
2. Or replace fields with wrapper types entirely (requires conversion in all access sites)

---

## 4. create_swapchain_framebuffers Method
**File:** `render_graph/compiled.rs:71-75`
```rust
pub fn create_swapchain_framebuffers(
    &mut self,
    swapchain_images: &[(vk::Image, vk::ImageView, vk::Extent2D, vk::Format)],
    immediate_render_pass: vk::RenderPass,
) -> Result<(), RenderGraphError>
```

**Problems:** All parameters are raw Vulkan types.

**Impact:** Used by lib.rs (VulkanRenderer::setup_render_graph), so it's part of the public API surface.

**Better approach:** Create wrapper types or use existing wrappers:
```rust
pub fn create_swapchain_framebuffers(
    &mut self,
    swapchain_images: &[(VkImage, VkImageView, Extent2D, ImageFormat)],
    immediate_render_pass: VkRenderPass,
) -> Result<(), RenderGraphError>
```

---

## 5. PassExecutionContext Methods
**File:** `render_graph/pass.rs:277, 289`
```rust
pub fn get_image(&self, resource_id: ResourceId) -> Option<(vk::Image, vk::ImageView)>
pub fn get_buffer(&self, resource_id: ResourceId) -> Option<vk::Buffer>
```

**Problems:** Return raw Vulkan handles.

**Impact:** Public methods on a public type. Not used by katla_app.

**Better approach:** Return wrapper types:
```rust
pub fn get_image(&self, resource_id: ResourceId) -> Option<(VkImage, VkImageView)>
pub fn get_buffer(&self, resource_id: ResourceId) -> Option<vk::Buffer>
// Note: No VkBuffer wrapper exists yet
```

---

## 6. SubpassDescriptor Fields
**File:** `render_graph/compiled.rs:62-64`
```rust
pub struct SubpassDescriptor {
    pass_index: usize,
    input_attachments: Vec<(u32, ResourceId)>,
    color_attachments: Vec<(u32, ResourceId)>,
    depth_stencil: Option<(u32, ResourceId)>,
    #[allow(dead_code)]
    resolve_attachments: Vec<(u32, ResourceId)>,
    vk_input_refs: Vec<vk::AttachmentReference>,   // ❌ Vulkan type
    vk_color_refs: Vec<vk::AttachmentReference>,   // ❌ Vulkan type
    vk_depth_ref: Option<vk::AttachmentReference>, // ❌ Vulkan type
}
```

**Problems:** Internal fields store Vulkan types.

**Impact:** All fields except the vk_* ones are private. This is internal-only, not used externally.

**Better approach:** This is fine for internal use, but the struct shouldn't be public if it contains vk types. (Currently exported in lib.rs)

---

## Recommendations

### High Priority (Public API Surface)
1. **PassExecutionContext.extent** → Use `Extent2D` wrapper
2. **CompiledPass.extent** → Use `Extent2D` wrapper
3. **CompiledPass.clear_values** → Use `Vec<ClearValue>` wrapper
4. **create_swapchain_framebuffers** → Use wrapper types in signature

### Medium Priority (Exported but not used by katla_app)
5. **ResourceUsage fields** → Make private with getter methods, or replace with wrapper types
6. **PassExecutionContext methods** → Return wrapper types

### Low Priority (Internal implementation)
7. **CompiledPass.pipeline_barriers_before** → Keep as-is if truly internal
8. **SubpassDescriptor** → Should be `pub(crate)` if not for external use

---

## Additional Findings

### Wrapper Types Already Exist (types.rs)
The following wrappers are already defined but not consistently used:
- `Extent2D` (line 92)
- `Extent3D` (line 122)
- `ImageLayout` (line 27)
- `AttachmentLoadOp` (line 60)
- `AttachmentStoreOp` (line 77)
- `ClearValue` (line 471)
- `Access` (line 369)
- `PipelineStage` (line 426)

### Missing Wrappers
- `VkBuffer` - doesn't exist in sync.rs
- `vk::MemoryBarrier` - complex struct with lifetime
- `vk::AttachmentReference` - internal struct
- `vk::Format` - has ImageFormat enum but not direct wrapper
