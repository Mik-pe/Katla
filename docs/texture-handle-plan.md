# Texture Handle Plan

## Status: ✅ COMPLETED


# Texture Handle Plan

## Overview

This document outlines the plan for integrating `TextureHandle` into katla_vulkan and katla_app, establishing a clean public API that doesn't expose Vulkan types.

## Current State

| Component | Current Pattern | Issue |
|-----------|-----------------|-------|
| Texture creation | `Texture::create_image_rgb(context, ...)` | Requires `Rc<VulkanContext>`, exposes `VkImageView` |
| Texture storage | `Rc<Texture>` held by callers | Manual lifetime management |
| Texture reference | `ImageInfo { vk::ImageView, vk::Sampler }` | Raw Vulkan types leak |
| Bindless integration | `manager.register_texture(VkImageView)` | Returns slot index, not handle |
| UIRenderer | `HashMap<u64, vk::ImageView>` | Raw Vulkan types |

## Target Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ katla_vulkan public API                                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  TextureDescriptor        // Plain data struct, no Vulkan types    │
│  TextureHandle            // Opaque u32 index                       │
│  TextureManager           // Creation, storage, lookup             │
│                                                                     │
│  VulkanRenderer                                                      │
│    ├─ texture_manager: TextureManager                               │
│    │    ├─ textures: ResourceStorage<Texture>                      │
│    │    └─ bindless_slots: HashMap<TextureHandle, u32>             │
│    │                                                                │
│    ├─ create_texture(desc) -> TextureHandle                        │
│    ├─ create_texture_from_bytes(data) -> TextureHandle             │
│    ├─ get_texture_view(handle) -> Option<VkImageView> (internal)   │
│    └─ destroy_texture(handle)                                       │
│                                                                     │
│  PbrTextureSet                                                      │
│    ├─ albedo: TextureHandle                                        │
│    ├─ normal: TextureHandle                                        │
│    └─ ...                                                           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
          │
          │ TextureHandle (opaque)
          ▼
┌─────────────────────────────────────────────────────────────────────┐
│ katla_app::UIRenderer                                               │
├─────────────────────────────────────────────────────────────────────┤
│  UITextures                                                         │
│    ├─ font_atlas: TextureHandle                                    │
│    ├─ white_texture: TextureHandle                                 │
│    └─ external_textures: HashMap<u64, TextureHandle>               │
│                                                                     │
│  register_texture(id: u64, handle: TextureHandle)                  │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Decisions

1. **TextureManager ownership**: Owned by `VulkanRenderer`
2. **Default textures**: Pre-created at TextureManager init
3. **Migration path**: Keep old API until new foundation is ready, then switch and remove old

---

## Implementation Summary

### Completed Steps

| Step | Status | Description |
|------|--------|-------------|
| 1 | ✅ | Created `texture/descriptor.rs` with `TextureDescriptor`, `TextureUsage` |
| 2 | ✅ | Created `texture/manager.rs` with `TextureManager` and default textures |
| 3 | ✅ | Created `texture/mod.rs` with re-exports |
| 4 | ✅ | Added `Texture::from_descriptor()` helper method |
| 5 | ✅ | Added `pub mod texture` to lib.rs with public exports |
| 6 | ✅ | Added `texture_manager: Option<TextureManager>` to VulkanRenderer |
| 7 | ✅ | Updated `PbrTextureSet` to use `TextureHandle` fields |
| 8 | ✅ | All compilation errors fixed |
| 9 | ✅ | All 157 unit tests pass |
| 10 | ✅ | Added `UIRenderer::register_texture_handle()` method |
| 11 | ✅ | Model loading uses `TextureHandle` via `PbrTextureSet` |
| 12 | ⏭️ | Remaining direct `Texture::create_*` usage kept for backward compatibility |

### Key API Changes

1. **New Types**:
   - `TextureDescriptor` - Plain data struct for texture creation
   - `TextureUsage` - Bitflags for texture usage
   - `TextureManager` - Centralized texture creation and storage

2. **PbrTextureSet Migration**:
   - Fields changed from `ImageInfo` to `TextureHandle`
   - `with_defaults(tm)` creates set with default textures
   - `with_placeholder_handles()` for bindless-only mode
   - `register_bindless()` helper for bindless registration

3. **Material API Changes**:
   - `Material::from_template_pbr_bindless()` no longer takes `texture_refs`
   - `Material::with_pbr_textures()` takes only `PbrTextureSet`
   - `Material::get_registration_data()` returns 7-tuple (removed `pbr_refs`)

4. **UIRenderer Additions**:
   - `register_texture_handle()` - Register texture via `TextureHandle`
   - Existing `register_texture()` still works for raw `VkImageView`

### Notes on Remaining Direct Usage

Some code still uses `Texture::create_*` directly:
- `application/mod.rs` - Thumbnail loading (extracts view for UI)
- `material_helpers.rs` - Checkerboard texture (managed via `Rc<Texture>`)
- `model_preview.rs` - GLTF loading (returns `Rc<Texture>`)
- `ui_renderer.rs` - Font atlas (internal management)

These are intentional - the current patterns work correctly and migrating to `TextureManager` would require additional refactoring. The new `TextureManager` API is available for new code.

---

## Phase 1: TextureDescriptor and TextureManager

### New files:

```
katla_vulkan/src/texture/
├── mod.rs           # Re-exports
├── descriptor.rs    # TextureDescriptor, TextureFormat, TextureUsage
└── manager.rs       # TextureManager
```

### 1.1 `descriptor.rs` - Public API types

```rust
use bitflags::bitflags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8UnormSrgb,
    Rgba8Unorm,
    R8Unorm,
    Rg8Unorm,
    Rgba16Float,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct TextureUsage: u32 {
        const SAMPLED = 1 << 0;
        const COPY_DST = 1 << 1;
    }
}

#[derive(Debug, Clone)]
pub struct TextureDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: TextureFormat,
    pub usage: TextureUsage,
    pub label: Option<&'static str>,
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
            label: None,
        }
    }
}
```

### 1.2 `manager.rs` - Texture storage and creation

```rust
use crate::handle::{ResourceStorage, TextureHandle};
use crate::sync::VkImageView;
use crate::vulkan::texture::Texture;
use crate::vulkan::context::VulkanContext;
use std::rc::Rc;
use ash::vk;

pub struct TextureManager {
    textures: ResourceStorage<Texture>,
    context: Rc<VulkanContext>,
    
    // Pre-created default textures
    default_white: TextureHandle,
    default_normal: TextureHandle,
    default_metallic_roughness: TextureHandle,
    default_occlusion: TextureHandle,
    default_emission: TextureHandle,
}

impl TextureManager {
    pub fn new(context: Rc<VulkanContext>) -> Result<Self, vk::Result> {
        let mut textures = ResourceStorage::new();
        
        // Pre-create defaults
        let default_white = Self::create_default(&mut textures, &context, || {
            Texture::create_default_albedo(context.clone())
        });
        // ... other defaults ...
        
        Ok(Self {
            textures,
            context,
            default_white,
            default_normal,
            default_metallic_roughness,
            default_occlusion,
            default_emission,
        })
    }
    
    // --- Creation API ---
    
    pub fn create(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        let texture = Texture::from_descriptor(&self.context, desc, data);
        TextureHandle::new(self.textures.insert(texture))
    }
    
    pub fn create_rgba(&mut self, width: u32, height: u32, data: &[u8]) -> TextureHandle {
        let desc = TextureDescriptor {
            width, height,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
            label: None,
        };
        self.create(&desc, data)
    }
    
    pub fn create_solid(&mut self, color: [u8; 4]) -> TextureHandle {
        self.create_rgba(1, 1, &color)
    }
    
    // --- Defaults ---
    
    pub fn default_white(&self) -> TextureHandle { self.default_white }
    pub fn default_normal(&self) -> TextureHandle { self.default_normal }
    pub fn default_metallic_roughness(&self) -> TextureHandle { self.default_metallic_roughness }
    pub fn default_occlusion(&self) -> TextureHandle { self.default_occlusion }
    pub fn default_emission(&self) -> TextureHandle { self.default_emission }
    
    // --- Lookup (internal use) ---
    
    pub fn get_view(&self, handle: TextureHandle) -> Option<VkImageView> {
        self.textures.get(handle.index()).map(|t| t.image_view)
    }
    
    pub fn get_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.index())
    }
    
    pub fn get_texture_mut(&mut self, handle: TextureHandle) -> Option<&mut Texture> {
        self.textures.get_mut(handle.index())
    }
    
    // --- Lifecycle ---
    
    pub fn destroy(&mut self, handle: TextureHandle) -> bool {
        self.textures.remove(handle.index()).is_some()
    }
}
```

### 1.3 `mod.rs` - Re-exports

```rust
mod descriptor;
mod manager;

pub use descriptor::*;
pub use manager::*;
```

### 1.4 Update `katla_vulkan/src/lib.rs`

```rust
pub mod texture;  // NEW

pub use texture::{TextureDescriptor, TextureFormat, TextureManager, TextureUsage};
```

---

## Phase 2: Integrate into VulkanRenderer

**File:** `katla_vulkan/src/renderer.rs`

```rust
use crate::texture::TextureManager;

pub struct VulkanRenderer {
    // ... existing fields ...
    pub texture_manager: TextureManager,  // NEW
}

impl VulkanRenderer {
    pub fn init(...) -> Self {
        // ... existing init ...
        let texture_manager = TextureManager::new(context.clone())?;
        
        Self {
            // ... existing ...
            texture_manager,
        }
    }
    
    // Convenience delegation
    pub fn create_texture(&mut self, desc: &TextureDescriptor, data: &[u8]) -> TextureHandle {
        self.texture_manager.create(desc, data)
    }
}
```

---

## Phase 3: Update PbrTextureSet

**File:** `katla_vulkan/src/vulkan/material/mod.rs`

```rust
pub struct PbrTextureSet {
    pub albedo: TextureHandle,
    pub normal: TextureHandle,
    pub metallic_roughness: TextureHandle,
    pub occlusion: TextureHandle,
    pub emission: TextureHandle,
}

impl PbrTextureSet {
    pub fn new(
        albedo: TextureHandle,
        normal: TextureHandle,
        metallic_roughness: TextureHandle,
        occlusion: TextureHandle,
        emission: TextureHandle,
    ) -> Self {
        Self { albedo, normal, metallic_roughness, occlusion, emission }
    }
    
    pub fn with_defaults(tm: &TextureManager) -> Self {
        Self {
            albedo: tm.default_white(),
            normal: tm.default_normal(),
            metallic_roughness: tm.default_metallic_roughness(),
            occlusion: tm.default_occlusion(),
            emission: tm.default_emission(),
        }
    }
}
```

---

## Phase 4: Update UIRenderer

**File:** `katla_app/src/rendering/ui_renderer.rs`

```rust
use katla_vulkan::{TextureHandle, TextureManager};

struct UITextures {
    font_atlas: TextureHandle,
    white: TextureHandle,
    external_textures: HashMap<u64, TextureHandle>,
    // ... descriptor/sampler stuff unchanged ...
}

pub struct UIRenderer {
    buffers: FrameBuffer<UIBuffers>,
    textures: UITextures,
    pipeline: PipelineHandle,
}

impl UIRenderer {
    pub fn new(
        renderer: &mut VulkanRenderer,  // Changed signature
        vertex_capacity: u64,
        index_capacity: u64,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Result<Self, vk::Result> {
        let tm = &mut renderer.texture_manager;
        
        let white = tm.create_solid([255, 255, 255, 255]);
        let white_atlas = vec![255u8; (atlas_width * atlas_height * 4) as usize];
        let font_atlas = tm.create_rgba(atlas_width, atlas_height, &white_atlas);
        
        // Get views for descriptor setup
        let font_view = tm.get_view(font_atlas).unwrap();
        // ... descriptor setup using font_view ...
        
        Ok(Self { buffers, textures, pipeline })
    }
    
    pub fn register_texture(&mut self, texture_id: u64, handle: TextureHandle) {
        self.textures.external_textures.insert(texture_id, handle);
    }
    
    pub fn font_atlas(&self) -> TextureHandle {
        self.textures.font_atlas
    }
}
```

---

## Execution Order

| Step | Action | Files |
|------|--------|-------|
| 1 | Create `texture/descriptor.rs` | NEW |
| 2 | Create `texture/manager.rs` | NEW |
| 3 | Create `texture/mod.rs` | NEW |
| 4 | Add `Texture::from_descriptor()` helper | `vulkan/texture.rs` |
| 5 | Add `pub mod texture` to lib.rs | `lib.rs` |
| 6 | Add `texture_manager` field to VulkanRenderer | `renderer.rs` |
| 7 | Update `PbrTextureSet` to use handles | `vulkan/material/mod.rs` |
| 8 | Run `cargo check` and fix compilation errors | - |
| 9 | Run `cargo test` | - |
| 10 | Update UIRenderer (separate commit) | `ui_renderer.rs` |
| 11 | Update model loading code | `model.rs`, etc. |
| 12 | Remove old `Texture::create_*` direct usage | - |

---

## API Comparison

### Before (leaks Vulkan types):

```rust
let texture = Rc::new(Texture::create_image_rgb(context.clone(), w, h, pixels));
let view = texture.image_view;  // VkImageView exposed
manager.register_texture(view); // Returns slot index
```

### After (clean API):

```rust
let handle = renderer.create_texture(&TextureDescriptor {
    width: 512, height: 512,
    format: TextureFormat::Rgba8UnormSrgb,
    usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
    label: Some("font_atlas"),
}, &pixels);

ui.register_texture(FONT_ATLAS_ID, handle);
```

---

## Benefits

| Aspect | Current | Proposed |
|--------|---------|----------|
| Type safety | Raw Vulkan wrappers exposed | Opaque handles only |
| Memory management | Manual Rc<VulkanContext> passing | Centralized in manager |
| API complexity | Exposes GPU implementation | Simple descriptor-based creation |
| Bindless integration | Manual slot tracking | Automatic via manager |
| Hot reload | Complex (direct references) | Handle-based lookup is trivial |
| Testing | Requires Vulkan context | Can mock TextureManager |
