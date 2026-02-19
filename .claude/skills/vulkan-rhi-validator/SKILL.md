# Vulkan RHI Validator

Validates that the katla_vulkan crate maintains proper RHI (Render Hardware Interface) abstraction over raw Vulkan.

## What This Skill Checks

### 1. Raw Type Exposure
The public API must NOT expose raw `ash::vk::*` types. All Vulkan types should be wrapped.

**Good:**
```rust
pub fn create_texture(format: ImageFormat, usage: ImageUsage) -> TextureHandle;
```

**Bad:**
```rust
pub fn create_texture(format: vk::Format, usage: vk::ImageUsageFlags) -> Texture;
```

### 2. Wrapper Type Pattern
Use wrapper types in `sync.rs` with `pub(crate) fn vk()` methods:

```rust
pub struct VkPipeline(pub vk::Pipeline);

impl VkPipeline {
    pub(crate) fn vk(&self) -> vk::Pipeline { self.0 }
}

impl From<VkPipeline> for vk::Pipeline {
    fn from(w: VkPipeline) -> vk::Pipeline { w.0 }
}
```

### 3. Abstraction Layers
The crate should have clear layers:

| Layer | Purpose | Examples |
|-------|---------|----------|
| High-level | Game engine API | `DrawCall`, `MeshHandle`, `MaterialHandle` |
| Mid-level | Render abstraction | `RenderGraph`, `Pass`, `Texture`, `Buffer` |
| Low-level | Vulkan internals | `Context`, `CommandBuffer`, `Pipeline` |

### 4. Common Violations to Check

1. **Public methods returning `vk::*` types** - Should return wrapper types
2. **`vk()` methods that are public** - Should be `pub(crate)`
3. **ResourceKind with raw flags** - Should use wrapper enums
4. **CommandBuffer bind methods** - Should take wrapper types
5. **Missing documentation** - All public items need docs

## Files to Review

- `katla_vulkan/src/lib.rs` - Main public API
- `katla_vulkan/src/render_graph/resource.rs` - Resource definitions
- `katla_vulkan/src/render_graph/types.rs` - Type wrappers (reference implementation)
- `katla_vulkan/src/vulkan/commandbuffer.rs` - Command recording
- `katla_vulkan/src/sync.rs` - Wrapper types

## Quick Check Commands

```bash
# Find raw vk types in public API signatures
grep -rn "pub fn.*vk::" katla_vulkan/src/lib.rs

# Find public vk() methods (should be pub(crate))
grep -rn "pub fn vk\(" katla_vulkan/src/

# Check for missing docs
cargo doc --no-deps 2>&1 | grep "warning: missing documentation"
```

## Reference: Proper RHI Pattern

The `rendering/types.rs` module demonstrates correct RHI abstraction:

```rust
// Opaque handles - no Vulkan knowledge required
pub struct MeshHandle(pub usize);
pub struct MaterialHandle(pub usize);

// Draw call builder - ergonomic API
let draw = DrawCall::new(mesh, material)
    .with_transform(matrix)
    .with_color(color);
```

All rendering should be possible using only these high-level types without any `ash::vk` imports.
