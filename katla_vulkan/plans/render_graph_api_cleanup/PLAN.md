# Render Graph API Cleanup

## Objective
Clean up the render graph public API to be intuitive, consistent, and hard to misuse. This plan addresses usability issues BEFORE adding new features from the production plan.

## Context
A strategic review of the render_graph API identified several critical issues:
- Execution callbacks cannot access physical resources
- Confusing proliferation of "Context" types
- Too many similar resource ID types
- Verbose descriptor creation (will get worse with production plan)
- Split methods for same concept (virtual vs imported)
- Excessive public module surface
- Easy-to-misuse patterns (forgotten .build(), useless return values)

## Guiding Principles

1. **Make the right thing easy, the wrong thing hard/impossible**
2. **One obvious way to do common tasks**
3. **Hide implementation details (slotmaps, internal modules)**
4. **Sensible defaults with escape hatches**

---

## Part 1: Unify PassContext with Resource Access

### 1.1 Problem
```rust
// Current: PassContext has NO resource access
graph.add_pass("geometry")
    .write_attachment(color, AttachmentType::Color)
    .execute(|ctx, cmd| {
        // ctx only has pass_index and pass_name
        // NO WAY to get the actual vk::Image for 'color'!
    })
    .build();
```

### 1.2 Solution
Merge `PassContext` and `PassExecutionContext` into a single type passed to callbacks:

```rust
pub struct PassContext<'a> {
    // Core info
    pass_index: u32,
    pass_name: &'a str,
    extent: vk::Extent2D,
    
    // Resource access (from PassExecutionContext)
    allocations: &'a PhysicalAllocations,
    frame_data: &'a FrameData,
}

impl<'a> PassContext<'a> {
    /// Get the physical image for a virtual image handle.
    pub fn image(&self, handle: VirtualImage) -> Option<vk::Image>;
    
    /// Get the image view for a virtual image handle.
    pub fn image_view(&self, handle: VirtualImage) -> Option<vk::ImageView>;
    
    /// Get the physical buffer for a virtual buffer handle.
    pub fn buffer(&self, handle: VirtualBuffer) -> Option<vk::Buffer>;
    
    /// Get the current frame index.
    pub fn frame_index(&self) -> u64;
    
    /// Get the render extent.
    pub fn extent(&self) -> vk::Extent2D;
}
```

### 1.3 Migration
- Rename `PassExecutionContext` to `PassContext` (merge functionality)
- Delete old `PassContext` struct
- Update all callback signatures
- Update `PassExecuteFn` type alias

### 1.4 Files to Modify

| File | Changes |
|------|---------|
| pass.rs | Rename PassContext, add resource access methods |
| context.rs | Delete PassExecutionContext, move logic to PassContext |
| executor.rs | Update to pass new PassContext |
| builder.rs | Update execute() signature |

---

## Part 2: Simplify Context Type Hierarchy

### 2.1 Problem
Three confusing context types:
- `GraphContext` (only holds frame_index)
- `ExecutionContext` (holds allocations, frame data)
- `PassExecutionContext` (wraps ExecutionContext)

### 2.2 Solution
Rename and simplify:

| Current | New Name | Purpose |
|---------|----------|---------|
| `GraphContext` | `FrameInfo` | Just frame metadata |
| `ExecutionContext` | `GraphContext` | Main context for graph execution |
| `PassExecutionContext` | (merged into PassContext) | - |

```rust
/// Simple frame metadata (renamed from GraphContext)
pub struct FrameInfo {
    pub frame_index: u64,
}

/// Main execution context (renamed from ExecutionContext)
pub struct GraphContext {
    allocations: PhysicalAllocations,
    frame_data: FrameData,
}
```

### 2.3 Files to Modify

| File | Changes |
|------|---------|
| compiled.rs | Rename GraphContext → FrameInfo |
| context.rs | Rename ExecutionContext → GraphContext |
| mod.rs | Update re-exports |
| All consumers | Update imports |

---

## Part 3: Add Builder Pattern for Descriptors

### 3.1 Problem
```rust
// Current: Verbose, easy to forget fields
let color = graph.create_image(ImageDescriptor {
    format: vk::Format::R8G8B8A8_SRGB,
    extent: vk::Extent2D { width: 1920, height: 1080 },
    usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
    name: "color",
    aliasable: true,
});

// After production plan: EVEN WORSE
ImageDescriptor {
    format: vk::Format::R8G8B8A8_SRGB,
    extent: vk::Extent3D { width: 1920, height: 1080, depth: 1 },  // NEW
    mip_levels: 1,      // NEW
    array_layers: 1,    // NEW
    usage: ...,
    name: "color",
    aliasable: true,
}
```

### 3.2 Solution
Add builder pattern with sensible defaults:

```rust
impl ImageDescriptor {
    /// Create a builder for image descriptor.
    pub fn builder() -> ImageDescriptorBuilder {
        ImageDescriptorBuilder::new()
    }
}

pub struct ImageDescriptorBuilder { /* fields */ }

impl ImageDescriptorBuilder {
    pub fn format(mut self, format: vk::Format) -> Self;
    pub fn extent(mut self, width: u32, height: u32) -> Self;
    pub fn extent_3d(mut self, width: u32, height: u32, depth: u32) -> Self;
    pub fn mip_levels(mut self, levels: u8) -> Self;
    pub fn array_layers(mut self, layers: u16) -> Self;
    pub fn usage(mut self, usage: vk::ImageUsageFlags) -> Self;
    pub fn add_usage(mut self, usage: vk::ImageUsageFlags) -> Self;
    pub fn name(mut self, name: &'static str) -> Self;
    pub fn aliasable(mut self, aliasable: bool) -> Self;
    
    // Convenience presets
    pub fn color_attachment(mut self) -> Self {
        self.add_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
    }
    pub fn depth_attachment(mut self) -> Self {
        self.format(vk::Format::D32_SFLOAT)
            .add_usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
    }
    pub fn sampled(mut self) -> Self;
    pub fn storage(mut self) -> Self;
    pub fn transfer_src(mut self) -> Self;
    pub fn transfer_dst(mut self) -> Self;
    
    pub fn build(self) -> ImageDescriptor;
}

// Usage becomes:
let color = graph.create_image(
    ImageDescriptor::builder()
        .format(vk::Format::R8G8B8A8_SRGB)
        .extent(1920, 1080)
        .color_attachment()
        .name("color")
        .build()
);
```

### 3.3 Convenience Shortcuts
Add shortcut methods on FrameGraph for common cases:

```rust
impl FrameGraph {
    /// Create a color attachment with sensible defaults.
    pub fn create_color_attachment(
        &mut self,
        width: u32,
        height: u32,
        format: vk::Format,
        name: &'static str,
    ) -> VirtualImage {
        self.create_image(
            ImageDescriptor::builder()
                .format(format)
                .extent(width, height)
                .color_attachment()
                .name(name)
                .build()
        )
    }
    
    /// Create a depth attachment with sensible defaults.
    pub fn create_depth_attachment(
        &mut self,
        width: u32,
        height: u32,
        name: &'static str,
    ) -> VirtualImage {
        self.create_image(
            ImageDescriptor::builder()
                .format(vk::Format::D32_SFLOAT)
                .extent(width, height)
                .depth_attachment()
                .name(name)
                .build()
        )
    }
    
    /// Create a uniform buffer.
    pub fn create_uniform_buffer(
        &mut self,
        size: u64,
        name: &'static str,
    ) -> VirtualBuffer {
        self.create_buffer(
            BufferDescriptor::builder()
                .size(size)
                .uniform_buffer()
                .name(name)
                .build()
        )
    }
}
```

### 3.4 Files to Modify

| File | Changes |
|------|---------|
| resource.rs | Add ImageDescriptorBuilder, BufferDescriptorBuilder |
| builder.rs | Add convenience shortcut methods |
| mod.rs | Export builders |

---

## Part 4: Unify Resource Access Methods

### 4.1 Problem
```rust
// Two methods for same concept
.write_attachment(color, AttachmentType::Color)        // virtual
.write_attachment_imported(swapchain, AttachmentType::Color)  // imported

// Inconsistent pattern
.read_image(img)
.read_imported(imp)
// But no .read_attachment() or .read_attachment_imported()
```

### 4.2 Solution
Unify using `ResourceRef` enum internally, single method externally:

```rust
impl GraphPassBuilder<'_> {
    // UNIFIED: Works for both virtual and imported
    pub fn write(&mut self, resource: impl Into<ResourceRef>, access: AccessType) -> &mut Self;
    pub fn read(&mut self, resource: impl Into<ResourceRef>, access: AccessType) -> &mut Self;
    
    // Convenience for common cases (internally call write/read)
    pub fn color_attachment(&mut self, resource: impl Into<ResourceRef>) -> &mut Self {
        self.write(resource, AccessType::ColorAttachmentWrite)
    }
    pub fn depth_attachment(&mut self, resource: impl Into<ResourceRef>) -> &mut Self {
        self.write(resource, AccessType::DepthStencilWrite)
    }
    pub fn depth_read(&mut self, resource: impl Into<ResourceRef>) -> &mut Self {
        self.read(resource, AccessType::DepthStencilRead)
    }
    pub fn sample(&mut self, resource: impl Into<ResourceRef>) -> &mut Self {
        self.read(resource, AccessType::ShaderRead)
    }
}

// Implement Into<ResourceRef> for all handle types
impl From<VirtualImage> for ResourceRef { ... }
impl From<VirtualBuffer> for ResourceRef { ... }
impl From<ImportedResource> for ResourceRef { ... }

// Usage becomes clean and consistent:
graph.add_pass("geometry")
    .color_attachment(color)           // virtual image
    .depth_attachment(depth)           // virtual image
    .sample(textures)                  // virtual image
    .build();

graph.add_pass("present")
    .color_attachment(swapchain)       // imported resource - SAME API!
    .sample(color)
    .build();
```

### 4.3 Migration (No Deprecation - Direct Removal)
1. Add new unified methods
2. Update ALL internal code to use new methods
3. Update ALL external usages (examples, tests, app crate)
4. Remove old methods immediately

**Old methods to REMOVE:**
- `write_attachment(resource, AttachmentType)` → use `color_attachment(resource)` or `write(resource, AccessType)`
- `write_attachment_imported(resource, AttachmentType)` → use `color_attachment(resource)` (same API!)
- `read_image(image)` → use `sample(image)` or `read(image, AccessType)`
- `read_buffer(buffer)` → use `read(buffer, AccessType)`
- `read_imported(resource)` → use `read(resource, AccessType)`
- `write_image(image)` → use `write(image, AccessType)`
- `write_buffer(buffer)` → use `write(buffer, AccessType)`
- `write_imported(resource)` → use `write(resource, AccessType)`
- `transfer_src_image(image)` → use `read(image, AccessType::TransferRead)`
- `transfer_dst_image(image)` → use `write(image, AccessType::TransferWrite)`

### 4.4 Files to Modify

| File | Changes |
|------|---------|
| pass.rs | Add unified read/write methods, deprecate old ones |
| handle.rs | Add From impls for ResourceRef |
| builder.rs | Add convenience methods |

---

## Part 5: Add Compile-Time Safety

### 5.1 Problem
```rust
// Easy to forget .build() - pass silently not registered
graph.add_pass("geometry")
    .write_attachment(color, AttachmentType::Color);
    // Oops! No .build() - pass doesn't exist!
```

### 5.2 Solution
Add `#[must_use]` attribute:

```rust
impl FrameGraph {
    #[must_use = "Pass builder must be finalized with .build() to register the pass"]
    pub fn add_pass(&mut self, name: &str) -> GraphPassBuilder<'_> {
        // ...
    }
}
```

### 5.3 Fix PassHelpers Return Value
```rust
// Current: Returns useless PassId::default()
pub fn clear_image(...) -> PassId {
    graph.add_pass(name).transfer_dst_image(image).execute(...).build();
    PassId::default()  // USELESS!
}

// Fixed: Return nothing (or actual PassId)
pub fn clear_image(...) {
    graph.add_pass(name)
        .transfer_dst_image(image)
        .execute(...)
        .build();
    // No return value - use graph.pass_id("name") if needed
}
```

### 5.4 Files to Modify

| File | Changes |
|------|---------|
| builder.rs | Add #[must_use] to add_pass() |
| helpers.rs | Fix PassHelpers return values |

---

## Part 6: Reduce Public Module Surface

### 6.1 Problem
20+ public modules expose internal architecture:
```rust
pub mod aliasing;
pub mod allocation;
pub mod barrier;
pub mod builder;
pub mod compiled;
pub mod context;
pub mod cull;
pub mod debug;
pub mod dependency;
pub mod error;
pub mod executor;
pub mod handle;
pub mod helpers;
pub mod lifetime;
pub mod optimize;
pub mod pass;
pub mod render_pass;
pub mod resource;
pub mod sort;
pub mod sync;
pub mod template;
```

### 6.2 Solution
Create a clean public API with `pub(crate)` for internals:

```rust
// mod.rs - Clean public API

// Core types (what users actually need)
pub use builder::{FrameGraph, GraphPassBuilder, ExportDescriptor, ExportId, ImportId, PassId};
pub use compiled::{CompiledGraph, compile_graph};
pub use context::{GraphContext, FrameInfo};
pub use handle::{VirtualImage, VirtualBuffer, VirtualResource, ImportedResource};
pub use pass::{PassContext, PassExecute, AccessType, AttachmentType};
pub use resource::{ImageDescriptor, BufferDescriptor, ImportDescriptor, ImageDescriptorBuilder, BufferDescriptorBuilder};
pub use error::GraphError;
pub use template::GraphTemplate;
pub use helpers::{PassHelpers, ResourceHelpers, clear_colors};

// Convenience prelude
pub mod prelude {
    pub use super::{FrameGraph, CompiledGraph, GraphContext, PassContext};
    pub use super::{VirtualImage, VirtualBuffer, ImportedResource};
    pub use super::{ImageDescriptor, BufferDescriptor};
    pub use super::{AccessType, AttachmentType};
    pub use super::{GraphError, GraphTemplate};
}

// Internal modules (pub(crate))
pub(crate) mod aliasing;
pub(crate) mod allocation;
pub(crate) mod barrier;
pub(crate) mod cull;
pub(crate) mod debug;
pub(crate) mod dependency;
pub(crate) mod executor;
pub(crate) mod lifetime;
pub(crate) mod optimize;
pub(crate) mod sort;
pub(crate) mod sync;

// These stay public (advanced use cases)
pub mod render_pass;  // Custom render pass creation
pub mod debug;        // Debug utilities
```

### 6.3 Files to Modify

| File | Changes |
|------|---------|
| render_graph/mod.rs | Reorganize exports, add prelude module |
| All internal modules | Change pub → pub(crate) where appropriate |

---

## Part 7: Default `aliasable` to `false`

### 7.1 Problem
```rust
impl Default for ImageDescriptor {
    fn default() -> Self {
        Self {
            aliasable: true,  // Dangerous default!
        }
    }
}
```

Resources default to aliasable, which could cause issues for resources that need dedicated memory.

### 7.2 Solution
```rust
impl Default for ImageDescriptor {
    fn default() -> Self {
        Self {
            format: vk::Format::UNDEFINED,
            extent: vk::Extent3D { width: 0, height: 0, depth: 1 },
            mip_levels: 1,
            array_layers: 1,
            usage: vk::ImageUsageFlags::empty(),
            name: "",
            aliasable: false,  // SAFE DEFAULT
        }
    }
}
```

### 7.3 Files to Modify

| File | Changes |
|------|---------|
| resource.rs | Change aliasable default to false |

---

## Implementation Order

### Phase 1: Critical Fixes (Priority: HIGH)
1. **Part 1**: Unify PassContext with resource access
2. **Part 2**: Simplify context type hierarchy
3. **Part 5**: Add compile-time safety

### Phase 2: Ergonomics (Priority: MEDIUM)
4. **Part 3**: Add builder pattern for descriptors
5. **Part 4**: Unify resource access methods

### Phase 3: Cleanup (Priority: LOW)
6. **Part 6**: Reduce public module surface
7. **Part 7**: Default aliasable to false

---

## Breaking Changes Summary

| Change | Migration |
|--------|-----------|
| PassContext merged | Use new `ctx.image()`, `ctx.buffer()` methods |
| GraphContext → FrameInfo | Rename imports |
| ExecutionContext → GraphContext | Rename imports |
| write_attachment_imported removed | Use unified `write()` or `color_attachment()` |
| read_image/write_image removed | Use unified `read()`/`write()` or convenience methods |
| read_imported/write_imported removed | Use unified `read()`/`write()` |
| 20+ modules → clean API | Use `render_graph::prelude::*` |
| aliasable default: true → false | Explicitly set `aliasable(true)` if needed |

---

## Verification Checklist

### Phase 1
- [ ] PassContext provides resource access methods
- [ ] execute() callbacks can access physical resources
- [ ] Context types renamed and simplified
- [ ] #[must_use] warning appears when .build() forgotten
- [ ] PassHelpers no longer returns useless values

### Phase 2
- [ ] ImageDescriptor::builder() works
- [ ] BufferDescriptor::builder() works
- [ ] Convenience methods (color_attachment, depth_attachment) work
- [ ] Unified read/write methods work for both virtual and imported
- [ ] Old methods removed (not deprecated)
- [ ] All usages updated to new API

### Phase 3
- [ ] Public API reduced to essential types
- [ ] prelude module works
- [ ] Internal modules are pub(crate)
- [ ] aliasable defaults to false
- [ ] All tests pass

---

## Testing Strategy

1. **Unit Tests**: Test each new method/type in isolation
2. **Integration Tests**: Update existing examples to use new API
3. **Migration Guide**: Create examples showing old vs new API
4. **Documentation**: Update all doc comments and examples
