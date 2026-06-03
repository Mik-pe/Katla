# Multi-Viewport & Bindless Implementation Plan

**Date:** March 2026  
**Status:** Ready for Implementation  
**Approach:** Remove old patterns, add new features (no hybrid)

## 📋 Table of Contents
1. [Current State Analysis](#current-state-analysis)
2. [Implementation Strategy](#implementation-strategy)
3. [Phase 1: Bindless Texture Arrays](#phase-1-bindless-texture-arrays)
4. [Phase 2: Multi-Viewport Compositing](#phase-2-multi-viewport-compositing)
5. [Phase 3: Cleanup & Migration](#phase-3-cleanup--migration)
6. [Testing Strategy](#testing-strategy)
7. [Rollback Plan](#rollback-plan)

---

## Current State Analysis

### ✅ What We Have
- **Viewport System** (`viewport.rs`, `viewport_manager.rs`)
  - `ViewportBuilder` for creating viewports
  - `ViewportManager` for managing multiple viewports
  - `ViewportRenderTarget` for color/depth textures
  - Output modes: `Offscreen`, `DirectToSwapchain`

- **Frame Graph** (`render_graph/graph.rs`)
  - Dynamic rendering (no traditional render passes)
  - Automatic barrier synchronization
  - Transient texture management
  - Pass templates: `GeometryPass`, `FullscreenPass`, `UIPass`, `ShadowPass`

- **Bindless System** (partial)
  - `BindlessTextureManager` exists
  - Font atlas uses bindless
  - Single texture registration only

### ❌ What We Need
- **Bindless texture arrays** for multi-viewport
- **Compositing pass** to combine viewports
- **Viewport render graph integration** (viewports currently outside frame graph)
- **Remove old patterns** (no hybrid approaches)

### 🎯 What We'll Remove
- **Old viewport rendering** (manual, not integrated with frame graph)
- **Single-texture bindless API** (replace with array-based API)
- **Direct viewport rendering** (route everything through frame graph)

---

## Implementation Strategy

### Core Principles
1. **No Hybrid Approaches** - Remove old code completely, don't maintain compatibility
2. **Frame Graph First** - All viewport rendering goes through frame graph
3. **Bindless Arrays** - Use texture array binding for all viewports
4. **Single Compositing Pass** - One pass to combine all viewports
5. **WGSL Compatibility** - Use uniform buffers + push descriptors (no push constants)

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Frame Graph                               │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Viewport 0   │  │ Viewport 1   │  │ Viewport 2   │      │
│  │ Pass         │  │ Pass         │  │ Pass         │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                 │               │
│         └─────────────────┼─────────────────┘               │
│                           │                                 │
│                    ┌──────▼──────┐                          │
│                    │ Compositing │                          │
│                    │ Pass        │                          │
│                    └──────┬──────┘                          │
│                           │                                 │
│                    ┌──────▼──────┐                          │
│                    │   UI Pass   │                          │
│                    └──────┬──────┘                          │
│                           │                                 │
│                    ┌──────▼──────┐                          │
│                    │ Backbuffer  │                          │
│                    └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Compositing Pass Descriptor Set

### Goal
Create a dedicated descriptor set for the multi-viewport compositing pass with fixed texture array bindings.

### Architectural Decision
**Why not bindless texture arrays?**
- Bindless descriptors already solve the "many textures" problem
- Texture arrays also solve the "many textures" problem
- Combining them (bindless + texture arrays) solves the same problem twice = over-engineering
- For the compositing pass specifically, we know all viewports at pass creation time
- A dedicated descriptor set with fixed array bindings is simpler and more explicit

### Changes Required

#### 1.1 Create Compositing Descriptor Set Layout

**File:** `katla_gfx/src/vulkan/material/descriptor.rs` (or similar)

**Add:**
```rust
/// Descriptor set for compositing pass.
///
/// Set 2: Compositing-specific bindings
/// - Binding 0: Sampler2D array for viewport textures (max 8 viewports)
/// - Binding 1: Sampler
/// - Binding 2: Uniform buffer for viewport parameters
pub struct CompositingDescriptorSet {
    descriptor_set: vk::DescriptorSet,
    layout: vk::DescriptorSetLayout,
    viewport_textures: Vec<vk::ImageView>, // Max 8
}

impl CompositingDescriptorSet {
    /// Create a new compositing descriptor set.
    ///
    /// # Arguments
    /// * `device` - Vulkan device
    /// * `viewport_textures` - Image views for each viewport (max 8)
    ///
    /// # Returns
    /// Compositing descriptor set ready for binding
    pub fn new(
        device: &Device,
        viewport_textures: Vec<vk::ImageView>,
    ) -> Result<Self, DescriptorError> {
        if viewport_textures.len() > 8 {
            return Err(DescriptorError::TooManyTextures(
                viewport_textures.len(),
                8,
            ));
        }

        // Create descriptor set layout
        let layout_bindings = [
            // Binding 0: Texture array (max 8)
            vk::DescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: 8, // Fixed array size
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                p_immutable_samplers: ptr::null(),
                ..Default::default()
            },
            // Binding 1: Sampler
            vk::DescriptorSetLayoutBinding {
                binding: 1,
                descriptor_type: vk::DescriptorType::SAMPLER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
            // Binding 2: Uniform buffer (viewport rects)
            vk::DescriptorSetLayoutBinding {
                binding: 2,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
        ];

        let layout_info = vk::DescriptorSetLayoutCreateInfo {
            binding_count: layout_bindings.len() as u32,
            p_bindings: layout_bindings.as_ptr(),
            ..Default::default()
        };

        let layout = unsafe {
            device.create_descriptor_set_layout(&layout_info, None)?
                // Error handling
        };

        // Allocate descriptor set from pool
        let descriptor_set = allocate_descriptor_set(device, layout)?;

        // Write descriptors
        Self::write_descriptors(device, descriptor_set, &viewport_textures)?;

        Ok(Self {
            descriptor_set,
            layout,
            viewport_textures,
        })
    }

    fn write_descriptors(
        device: &Device,
        descriptor_set: vk::DescriptorSet,
        viewport_textures: &[vk::ImageView],
    ) -> Result<(), DescriptorError> {
        // Write texture array (binding 0)
        let image_infos: Vec<vk::DescriptorImageInfo> = viewport_textures
            .iter()
            .map(|_| vk::DescriptorImageInfo {
                sampler: vk::Sampler::null(),
                image_view: vk::ImageView::null(), // Filled in loop
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            })
            .collect();

        let write_texture_array = vk::WriteDescriptorSet {
            dst_set: descriptor_set,
            dst_binding: 0,
            dst_array_element: 0,
            descriptor_count: viewport_textures.len() as u32,
            descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
            p_image_info: image_infos.as_ptr(),
            ..Default::default()
        };

        unsafe { device.update_descriptor_sets(&[write_texture_array], &[]) };

        // Write sampler (binding 1)
        // Write uniform buffer (binding 2)
        // ...

        Ok(())
    }

    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout
    }

    pub fn set(&self) -> vk::DescriptorSet {
        self.descriptor_set
    }
}
```

#### 1.2 Update Pipeline Layout for Compositing

**File:** `katla_gfx/src/vulkan/pipeline.rs`

**Add:**
```rust
/// Pipeline layout for compositing pass.
///
/// Set 0: Per-frame data (existing bindless set)
/// Set 1: Material data (existing)
/// Set 2: Compositing-specific (new)
pub struct CompositingPipelineLayout {
    pipeline_layout: vk::PipelineLayout,
}

impl CompositingPipelineLayout {
    pub fn new(
        device: &Device,
        bindless_set_layout: vk::DescriptorSetLayout,
        compositing_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self, PipelineError> {
        let set_layouts = [
            bindless_set_layout,   // Set 0: Per-frame
            compositing_set_layout, // Set 2: Compositing
        ];

        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: set_layouts.len() as u32,
            p_set_layouts: set_layouts.as_ptr(),
            ..Default::default()
        };

        let pipeline_layout = unsafe {
            device.create_pipeline_layout(&layout_info, None)?
        };

        Ok(Self { pipeline_layout })
    }
}
```

### Testing
```rust
#[test]
fn test_compositing_descriptor_set() {
    // Create 3 viewport textures
    let textures = vec![view1, view2, view3];

    // Create compositing descriptor set
    let set = CompositingDescriptorSet::new(&device, textures).unwrap();

    // Should have layout and set
    assert_ne!(set.layout(), vk::DescriptorSetLayout::null());
    assert_ne!(set.set(), vk::DescriptorSet::null());
}

#[test]
fn test_compositing_too_many_textures() {
    // Try to create with 9 textures (max is 8)
    let textures: Vec<_> = (0..9).map(|_| create_texture()).collect();

    let result = CompositingDescriptorSet::new(&device, textures);
    assert!(matches!(result, Err(DescriptorError::TooManyTextures(9, 8))));
}
```

---

## Phase 2: Multi-Viewport Compositing

### Goal
Add compositing pass to combine multiple viewport textures into final output using dedicated descriptor set.

### Changes Required

#### 2.1 Create `CompositePass`

**New File:** `katla_gfx/src/render_graph/passes/composite.rs`

```rust
//! Multi-viewport compositing pass.
//!
//! Combines multiple viewport textures into final output using
//! viewport rectangles for positioning. Uses a dedicated descriptor set
//! with fixed texture array bindings (max 8 viewports).

use std::collections::HashMap;

use crate::handle::MaterialHandle;
use crate::render_graph::builder::{InternalPassBuilder, PassBuilder};
use crate::render_graph::pass::PassType;
use crate::render_graph::resource::GraphResourceHandle;
use crate::texture::ImageFormat;

/// Viewport rectangle for compositing.
#[derive(Clone, Copy, Debug)]
pub struct ViewportRect {
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
}

impl ViewportRect {
    /// Create a new viewport rectangle.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Convert to WGSL-compatible array [x, y, z, w] where z=x+width, w=y+height
    pub fn to_array(&self) -> [f32; 4] {
        [self.x, self.y, self.x + self.width, self.y + self.height]
    }
}

/// Multi-viewport compositing pass.
///
/// Combines multiple viewport textures into a single output using
/// viewport rectangles for positioning. Uses a dedicated descriptor set
/// (Set 2) with fixed texture array bindings for simplicity.
///
/// # Example
///
/// ```ignore
/// let composite = CompositePass::new("composite")
///     .viewport("viewport_0", ViewportRect::new(0.0, 0.0, 960.0, 1080.0))
///     .viewport("viewport_1", ViewportRect::new(960.0, 0.0, 960.0, 1080.0))
///     .write("backbuffer");
///
/// let graph = FrameGraph::builder()
///     .add_pass(ViewportPass::new("viewport_0").write("viewport_0", format))
///     .add_pass(ViewportPass::new("viewport_1").write("viewport_1", format))
///     .add_pass(composite)
///     .build(&renderer)?;
/// ```
pub struct CompositePass {
    name: String,
    viewports: Vec<(String, ViewportRect)>, // (texture_name, rect)
    output: Option<String>,
    output_format: Option<ImageFormat>,
    material: Option<MaterialHandle>,
}

impl CompositePass {
    /// Create a new compositing pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            viewports: Vec::new(),
            output: None,
            output_format: None,
            material: None,
        }
    }

    /// Add a viewport to composite.
    ///
    /// # Arguments
    /// * `texture_name` - Name of the viewport texture to read
    /// * `rect` - Viewport rectangle for positioning
    pub fn viewport(mut self, texture_name: impl Into<String>, rect: ViewportRect) -> Self {
        self.viewports.push((texture_name.into(), rect));
        self
    }

    /// Set the output (backbuffer or transient texture).
    pub fn write(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Set the output format (required if not writing to backbuffer).
    pub fn output_format(mut self, format: ImageFormat) -> Self {
        self.output_format = Some(format);
        self
    }

    /// Set the material for compositing shader.
    pub fn material(mut self, material: MaterialHandle) -> Self {
        self.material = Some(material);
        self
    }

    /// Get viewport configuration (for uniform buffer).
    pub(crate) fn get_viewport_data(&self) -> Vec<[f32; 4]> {
        // Returns rect_array for each viewport
        self.viewports.iter().map(|(_, rect)| rect.to_array()).collect()
    }

    /// Get number of viewports (max 8).
    pub(crate) fn viewport_count(&self) -> u32 {
        self.viewports.len() as u32
    }
}

impl PassBuilder for CompositePass {
    fn as_builder(self) -> InternalPassBuilder {
        let reads: Vec<String> = self.viewports.iter().map(|(n, _)| n.clone()).collect();
        let writes: Vec<String> = self.output.into_iter().collect();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads,
            writes,
            pipeline: None, // Compositing uses material pipeline
            tonemap_params: None,
            material: self.material,
            output_format: self.output_format,
            build_fn: Box::new(move |_resource_map| {
                Ok(Box::new(CompositePassData))
            }),
            uses_depth: false, // No depth testing for compositing
        }
    }
}

/// Internal data for compositing pass.
pub(crate) struct CompositePassData;
```

#### 2.2 Create Compositing Shader

**New File:** `resources/shaders/composite.wgsl`

```wgsl
// Multi-viewport compositing shader
// Combines multiple viewport textures into final output
// Uses dedicated descriptor set (Set 2) with fixed texture array

struct ViewportRect {
    x: f32, y: f32, z: f32, w: f32, // x, y, x+width, y+height
}

struct ViewportParams {
    rects: array<ViewportRect, 8>,  // Max 8 viewports
    viewport_count: u32,
    screen_size: vec2<f32>,
    _padding: f32,
};

// Set 2: Compositing-specific bindings
@group(2) @binding(0) var viewportTextures: array<texture_2d<f32>, 8>; // Fixed array
@group(2) @binding(1) var sampler0: sampler;
@group(2) @binding(2) var<uniform> params: ViewportParams;

struct VertexOutput {
    @location(0) uv: vec2<f32>,
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@location(0) position: vec2<f32>) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = position * 0.5 + 0.5; // [-1,1] -> [0,1]
    output.uv.y = 1.0 - output.uv.y;  // Flip Y
    return output;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixelPos = in.uv * params.screen_size;
    var result: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);

    // Check each viewport in reverse order (topmost last)
    for (var i: i32 = i32(params.viewport_count) - 1; i >= 0; i--) {
        let u = u32(i);
        let rect = params.rects[u];

        // Check if pixel is in this viewport
        if (pixelPos.x >= rect.x && pixelPos.x < rect.z &&
            pixelPos.y >= rect.y && pixelPos.y < rect.w) {
            // Calculate local UV
            let localUV = (pixelPos - rect.xy) / (rect.zw - rect.xy);

            // Sample from viewport texture (direct array indexing)
            let viewportColor = textureSample(
                viewportTextures[u],
                sampler0,
                localUV
            );

            // If opaque, overwrite and exit
            if (viewportColor.a >= 0.95) {
                return viewportColor;
            }

            // Otherwise blend
            result = mix(result, viewportColor, viewportColor.a);
        }
    }

    return result;
}
```

#### 2.3 Integrate with Frame Graph Execution

**File:** `katla_gfx/src/render_graph/graph.rs`

**Update `execute_graphics_pass`:**
```rust
fn execute_graphics_pass(
    &mut self,
    cmd: &CommandBuffer,
    pass: &PassDesc,
    data: PassExecutionData,
) -> Result<(), RenderGraphError> {
    // ... existing code ...

    // Check if this is a compositing pass
    if let Some(composite_data) = pass.composite_data {
        self.execute_compositing_pass(cmd, pass, composite_data)?;
    } else {
        // Existing graphics pass logic
        // ...
    }
}

/// Execute compositing pass (fullscreen quad with viewport parameters).
fn execute_compositing_pass(
    &mut self,
    cmd: &CommandBuffer,
    pass: &PassDesc,
    composite_data: &CompositePassData,
) -> Result<(), RenderGraphError> {
    // Update uniform buffer with viewport rectangles
    let viewport_rects = composite_data.get_viewport_data();
    let viewport_count = composite_data.viewport_count();

    renderer.update_compositing_uniform(
        frame_idx,
        &viewport_rects,
        viewport_count,
        screen_size,
    )?;

    // Bind compositing descriptor set (Set 2)
    cmd.bind_descriptor_sets(
        vk::PipelineBindPoint::GRAPHICS,
        pipeline_layout,
        2, // Set 2
        &[composite_data.descriptor_set()],
        &[],
    );

    // Draw fullscreen quad
    cmd.draw(3, 1, 0, 0);

    Ok(())
}
```

#### 2.4 Integration Example

**Example Usage:**
```rust
// Create frame graph with multi-viewport compositing
let graph = FrameGraph::builder()
    // Viewport 0: Left side of screen
    .create_resource(GraphResourceDesc {
        name: "viewport_0".to_string(),
        width: 960,
        height: 1080,
        format: ImageFormat::R16G16B16A16Sfloat,
        resource_type: GraphResourceType::ColorAttachment { clear_value: ClearValue::OPAQUE_BLACK },
    })
    // Viewport 1: Right side of screen
    .create_resource(GraphResourceDesc {
        name: "viewport_1".to_string(),
        width: 960,
        height: 1080,
        format: ImageFormat::R16G16B16A16Sfloat,
        resource_type: GraphResourceType::ColorAttachment { clear_value: ClearValue::OPAQUE_BLACK },
    })
    // Render passes for each viewport
    .add_pass(GeometryPass::new("viewport_0_pass")
        .write_color("viewport_0", ImageFormat::R16G16B16A16Sfloat)
        .material(viewport_material))
    .add_pass(GeometryPass::new("viewport_1_pass")
        .write_color("viewport_1", ImageFormat::R16G16B16A16Sfloat)
        .material(viewport_material))
    // Compositing pass
    .add_pass(CompositePass::new("composite")
        .viewport("viewport_0", ViewportRect::new(0.0, 0.0, 960.0, 1080.0))
        .viewport("viewport_1", ViewportRect::new(960.0, 0.0, 960.0, 1080.0))
        .write("backbuffer")
        .material(composite_material))
    // UI on top
    .add_pass(UIPass::new("ui")
        .write("backbuffer")
        .read("backbuffer") // Sample composite output
        .material(ui_material))
    .build(&renderer)?;

// Initialize transient textures
graph.initialize_transient_textures(&renderer)?;

// Create compositing descriptor set with viewport textures
let viewport_textures = vec![
    graph.transient_texture("viewport_0", 0).unwrap().image_view_vk(),
    graph.transient_texture("viewport_1", 0).unwrap().image_view_vk(),
];
let compositing_set = CompositingDescriptorSet::new(&renderer.device(), viewport_textures)?;

// Execute frame
graph.execute(&renderer, |frame| {
    // Submit draw lists to each viewport
    frame.submit("viewport_0_pass", &viewport_0_draw_list);
    frame.submit("viewport_1_pass", &viewport_1_draw_list);
})?;
```

---

## Architectural Decision: Why Not Bindless + Texture Arrays?

### The Original Plan (Over-Engineered)
The initial implementation plan proposed using **bindless texture arrays** for the compositing pass:
- Extend bindless system to support texture arrays
- Register viewport textures as bindless array
- Access via `texture_array[bindless_index]`

### The Problem: Solving the Same Problem Twice
**Bindless descriptors** and **texture arrays** both solve the "many textures" problem:
- **Bindless**: Access thousands of textures via dynamic indexing in shaders
- **Texture arrays**: Access multiple textures via array indexing

Combining them (bindless + texture arrays) is **redundant**:
- Adds unnecessary complexity to bindless manager
- Makes the compositing pass harder to understand
- No performance benefit for this use case

### The Better Approach: Dedicated Descriptor Set
For the **compositing pass specifically**, we have key advantages:
- **Known at creation time**: All viewports are known when creating the compositing pass
- **Small, fixed number**: Max 8 viewports is reasonable for most use cases
- **Isolated use case**: Only the compositing pass needs these textures

**Solution**: Use a dedicated descriptor set (Set 2) with fixed bindings:
```wgsl
// Set 2: Compositing-specific (simple, explicit)
@group(2) @binding(0) var viewportTextures: array<texture_2d<f32>, 8>;
@group(2) @binding(1) var sampler0: sampler;
@group(2) @binding(2) var<uniform> params: ViewportParams;
```

### Benefits of the Simpler Approach
1. **No bindless complexity**: Don't need to extend bindless system
2. **Explicit and clear**: Easy to see what textures the compositing pass uses
3. **Easier to debug**: Fixed layout, no dynamic indexing confusion
4. **Same performance**: For 8 textures, no performance difference
5. **Future-proof**: Can still use bindless for other passes that need it

### Implementation Impact
- **Removed**: Phase 1 bindless texture array extensions
- **Simplified**: Phase 2 compositing pass uses dedicated descriptor set
- **Result**: Faster to implement, easier to understand, same functionality
- **Timeline**: Reduced from ~3 weeks to ~2 weeks (less code to write and test)

### Key Takeaway
**Use the right tool for the job:**
- **Bindless**: For dynamic, unknown-at-compile-time texture access (e.g., materials, UI)
- **Texture arrays**: For fixed, known sets of textures (e.g., multi-viewport compositing)
- **Not both**: Don't combine them unless you have a clear reason

This aligns with the project's **"no hybrid approaches"** principle—choose one simple approach and stick with it.

---

## Phase 3: Cleanup & Migration

### Goal
Remove old viewport rendering code, migrate all usage to frame graph approach.

### Files to Remove/Refactor

#### 3.1 Remove Old Viewport Rendering

**File:** `katla_gfx/src/renderer.rs` (or similar)

**Remove:**
```rust
// Remove these functions - rendering now goes through frame graph
impl VulkanRenderer {
    pub fn render_viewport(&mut self, viewport: ViewportHandle, ...) { ... }
    pub fn viewport_texture(&self, viewport: ViewportHandle) -> ... { ... }
}
```

**Keep:**
```rust
// Keep viewport creation and management
impl VulkanRenderer {
    pub fn create_viewport(&mut self) -> ViewportBuilder { ... }
    pub fn destroy_viewport(&mut self, viewport: ViewportHandle) { ... }
}

// But Viewport becomes just data storage, rendering happens via frame graph
```

#### 3.2 Refactor `Viewport` Struct

**File:** `katla_gfx/src/viewport.rs`

**Before:**
```rust
pub struct Viewport {
    pub render_target: ViewportRenderTarget, // Has color/depth textures
    pub storage_manager: Option<StorageUniformManager>,
    pub draw_list: DrawList,
    // ... rendering state ...
}
```

**After:**
```rust
pub struct Viewport {
    // Configuration only (no rendering state)
    pub label: String,
    pub extent: Size2D,
    pub clear_color: [f32; 4],
    pub output_mode: OutputMode,
    // No: render_target, storage_manager, draw_list, frame_uniforms
    // These are managed by the frame graph
}
```

#### 3.3 Update `ViewportManager`

**File:** `katla_gfx/src/renderer/viewport_manager.rs`

**Simplify:**
```rust
pub(crate) struct ViewportManager {
    viewports: Vec<Viewport>, // Just configuration, no rendering
}

impl ViewportManager {
    // Keep creation, lookup, destruction
    // Remove: texture_id, extent (now on Viewport struct)
    
    // Add: Helper to build frame graph passes
    pub fn build_viewport_passes(
        &self,
        viewport_handles: &[ViewportHandle],
        graph: &mut FrameGraphBuilder,
    ) -> Result<Vec<String>, RenderGraphError> {
        // For each viewport, create a geometry pass
        // Return list of viewport texture names for compositing
    }
}
```

#### 3.4 Migration Guide for Existing Code

**Before (old approach):**
```rust
// Create viewport
let viewport = renderer.create_viewport()
    .size(512, 512)
    .build(&mut renderer)?;

// Render to viewport
renderer.render_viewport(viewport, &camera, &draw_list);

// Get texture for UI
let texture_id = renderer.viewport_texture(viewport);
ui.image(texture_id);
```

**After (frame graph approach):**
```rust
// Create viewport (configuration only)
let viewport = renderer.create_viewport()
    .size(512, 512)
    .build(&mut renderer)?;

// Build frame graph with viewport passes
let viewport_textures = renderer.viewport_manager()
    .build_viewport_passes(&[viewport], &mut graph_builder)?;

let graph = graph_builder
    .add_pass(CompositePass::new("composite")
        .viewport(&viewport_textures[0], ViewportRect::new(0.0, 0.0, 512.0, 512.0))
        .write("backbuffer"))
    .build(&renderer)?;

// Register viewport textures as bindless array
graph.register_texture_array_bindless(&mut renderer, &viewport_textures)?;

// Execute frame
graph.execute(&renderer, |frame| {
    frame.submit("viewport_pass", &draw_list);
})?;

// Get texture for UI (now from frame graph)
let texture = graph.transient_texture("viewport_0", frame_idx);
ui.image(texture.image_view_vk());
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositing_descriptor_set() {
        // Test creating compositing descriptor set
        let textures = vec![view1, view2, view3];
        let set = CompositingDescriptorSet::new(&device, textures).unwrap();

        // Should have valid layout and set
        assert_ne!(set.layout(), vk::DescriptorSetLayout::null());
        assert_ne!(set.set(), vk::DescriptorSet::null());
    }

    #[test]
    fn test_composite_pass_builder() {
        let composite = CompositePass::new("test")
            .viewport("v0", ViewportRect::new(0.0, 0.0, 100.0, 100.0))
            .viewport("v1", ViewportRect::new(100.0, 0.0, 100.0, 100.0))
            .write("backbuffer");

        assert_eq!(composite.viewports.len(), 2);
        assert_eq!(composite.viewport_count(), 2);
    }

    #[test]
    fn test_viewport_rect_conversion() {
        let rect = ViewportRect::new(10.0, 20.0, 100.0, 200.0);
        let arr = rect.to_array();
        assert_eq!(arr, [10.0, 20.0, 110.0, 220.0]);
    }

    #[test]
    fn test_frame_graph_multi_viewport() {
        // Build frame graph with multiple viewports
        let graph = FrameGraph::builder()
            .add_pass(GeometryPass::new("v0").write_color("v0", format))
            .add_pass(GeometryPass::new("v1").write_color("v1", format))
            .add_pass(CompositePass::new("comp")
                .viewport("v0", ViewportRect::new(0.0, 0.0, 960.0, 1080.0))
                .viewport("v1", ViewportRect::new(960.0, 0.0, 960.0, 1080.0))
                .write("backbuffer"))
            .build(&renderer).unwrap();

        // Should compile without errors
        assert!(graph.compile().is_ok());
    }

    #[test]
    fn test_compositing_too_many_viewports() {
        // Max 8 viewports allowed
        let mut composite = CompositePass::new("test");
        for i in 0..9 {
            composite = composite.viewport(
                &format!("v{}", i),
                ViewportRect::new(0.0, 0.0, 100.0, 100.0),
            );
        }

        // Should fail when creating descriptor set
        let textures: Vec<_> = (0..9).map(|_| create_texture()).collect();
        let result = CompositingDescriptorSet::new(&device, textures);
        assert!(matches!(result, Err(DescriptorError::TooManyTextures(9, 8))));
    }
}
```

### Integration Tests

```rust
#[test]
fn test_split_screen_rendering() {
    // Create 2 viewports side-by-side
    let graph = setup_split_screen_graph(&mut renderer);

    // Create compositing descriptor set
    let viewport_textures = vec![
        graph.transient_texture("viewport_0", 0).unwrap().image_view_vk(),
        graph.transient_texture("viewport_1", 0).unwrap().image_view_vk(),
    ];
    let compositing_set = CompositingDescriptorSet::new(&renderer.device(), viewport_textures).unwrap();

    // Render to both viewports
    graph.execute(&renderer, |frame| {
        frame.submit("viewport_0", &left_scene);
        frame.submit("viewport_1", &right_scene);
    }).unwrap();

    // Verify output
    verify_render_output("split_screen_expected.png");
}

#[test]
fn test_four_viewport_grid() {
    // 2x2 grid of viewports
    let graph = setup_grid_viewports(&mut renderer, 2, 2);

    // Create compositing descriptor set with 4 textures
    let viewport_textures = (0..4)
        .map(|i| graph.transient_texture(&format!("viewport_{}", i), 0).unwrap().image_view_vk())
        .collect();
    let compositing_set = CompositingDescriptorSet::new(&renderer.device(), viewport_textures).unwrap();

    graph.execute(&renderer, |frame| {
        for i in 0..4 {
            frame.submit(&format!("viewport_{}", i), &scenes[i]);
        }
    }).unwrap();
}

#[test]
fn test_viewport_overlap() {
    // Test overlapping viewports (transparency)
    let graph = setup_overlapping_viewports(&mut renderer);

    // Create compositing descriptor set
    let viewport_textures = vec![
        graph.transient_texture("viewport_0", 0).unwrap().image_view_vk(),
        graph.transient_texture("viewport_1", 0).unwrap().image_view_vk(),
    ];
    let compositing_set = CompositingDescriptorSet::new(&renderer.device(), viewport_textures).unwrap();

    // Should blend correctly
    verify_alpha_blending();
}
```

### Validation Tests

```rust
#[test]
fn validate_barrier_placement() {
    // Ensure barriers are inserted correctly between viewport passes
    let graph = build_multi_viewport_graph();
    let barriers = graph.collect_barriers();

    // Should have barrier after each viewport write
    assert_barriers_before_compositing(&barriers);
}

#[test]
fn validate_descriptor_set_bindings() {
    // Ensure compositing descriptor set has correct bindings
    let textures = vec![view1, view2];
    let set = CompositingDescriptorSet::new(&device, textures).unwrap();

    // Should have 3 bindings: texture array, sampler, uniform buffer
    assert_eq!(set.binding_count(), 3);
}
```

---

## Rollback Plan

### If Phase 1 Fails (Bindless Arrays)
- **Symptom:** Cannot register texture arrays
- **Rollback:** Keep single-texture API, manually register each viewport texture
- **Impact:** More verbose code, but functional

### If Phase 2 Fails (Compositing)
- **Symptom:** Compositing shader doesn't work correctly
- **Rollback:** Use separate render passes for each viewport (no compositing)
- **Impact:** No overlapping/alpha blending support

### If Phase 3 Fails (Cleanup)
- **Symptom:** Breaking existing viewport usage
- **Rollback:** Keep old viewport rendering API alongside new API
- **Impact:** Hybrid approach (violates our principle, but maintains compatibility)

### Git Strategy
```bash
# Create feature branch
git checkout -b feature/multi-viewport-framegraph

# Phase 1: Bindless arrays
git commit -m "feat(gfx): add bindless texture array support"

# Phase 2: Compositing
git commit -m "feat(gfx): add multi-viewport compositing pass"

# Phase 3: Cleanup
git commit -m "refactor(gfx): remove old viewport rendering, migrate to frame graph"

# If rollback needed, can revert individual commits
```

---

## Success Criteria

### Phase 1 Success
- ✅ Can register 4+ textures as bindless array
- ✅ WGSL shaders can index texture array
- ✅ No performance regression vs single texture

### Phase 2 Success
- ✅ Can render 2+ viewports side-by-side
- ✅ Viewport rectangles work correctly
- ✅ Alpha blending works for overlapping viewports
- ✅ No visual artifacts or black screens

### Phase 3 Success
- ✅ All old viewport rendering code removed
- ✅ All existing usage migrated to frame graph
- ✅ No hybrid approaches remain
- ✅ Tests pass, no regressions

---

## References

- **Best Practices:** `docs/vulkan-framegraph-ui-best-practices-2024-2026.md`
- **Current Analysis:** `docs/katla-rendergraph-analysis.md`
- **WGSL Spec:** https://www.w3.org/TR/WGSL/
- **Vulkan Dynamic Rendering:** https://registry.khronos.org/vulkan/specs/1.3/html/chap7.html#renderpass
