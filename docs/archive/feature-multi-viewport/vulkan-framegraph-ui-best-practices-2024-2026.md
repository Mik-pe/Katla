# Vulkan Frame Graph & UI Integration Best Practices (2024-2026)

**Research Date:** March 2026  
**Status:** State of the Art Analysis

> **Note for WGSL Projects:** This document uses GLSL examples for clarity, but all patterns are applicable to WGSL. Key differences:
> - Use **uniform buffers** instead of push constants (WGSL limitation)
> - Use **push descriptors** for dynamic descriptor updates
> - Shader objects work via SPIR-V compilation
> - Bindless descriptors work identically in both languages

## Key Findings

### 1. Modern Vulkan Features (2024-2026)

#### **Dynamic Rendering (VK_KHR_dynamic_rendering)**
- **Status**: Production-ready, widely adopted
- **Benefits**:
  - Eliminates render pass/framebuffer complexity
  - More flexible for multi-viewport setups
  - On-the-fly attachment configuration
  - Better for desktop GPUs
- **Trade-offs**: Tiled GPUs may still prefer traditional render passes

```rust
// Dynamic rendering example
let color_attachment = vk::RenderingAttachmentInfo::default()
    .image_view(color_view)
    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
    .load_op(vk::AttachmentLoadOp::CLEAR)
    .store_op(vk::AttachmentStoreOp::STORE);

cmd.begin_rendering(&[color_attachment], None, None, render_area, 1);
// ... draw calls ...
cmd.end_rendering();
```

#### **Bindless Descriptors**
- **Status**: Essential for modern architectures
- **Benefits**:
  - Single descriptor set for all resources
  - Index textures/buffers via shader arrays
  - Reduces descriptor management overhead
  - Critical for multi-viewport scenarios

```glsl
// Shader example
layout(binding = 0) uniform sampler2D viewportTextures[];
uniform int viewportIndex;

vec4 color = texture(viewportTextures[viewportIndex], uv);
```

#### **Buffer Device Address (BDA)**
- **Status**: Production-ready
- **Benefits**:
  - Direct GPU memory addressing for buffers
  - Eliminates descriptor management for buffers
  - Pass addresses via push descriptors (WGSL-compatible)
  - More efficient than traditional buffer descriptors

**Note for WGSL Projects:**  
Since WGSL doesn't support push constants, use **push descriptors** instead:

```rust
// Push descriptors (WGSL-compatible)
let buffer_info = vk::DescriptorBufferInfo::default()
    .buffer(buffer)
    .offset(0)
    .range(size);

let write = vk::WriteDescriptorSet::default()
    .dst_binding(0)
    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
    .buffer_info(&buffer_info);

// Push descriptor set before draw call
cmd.push_descriptor_set(
    vk::PipelineBindPoint::GRAPHICS,
    pipeline_layout,
    0, // set number
    &[write],
);
```

#### **Shader Objects (VK_EXT_shader_object)**
- **Status**: Emerging, gaining adoption
- **Benefits**:
  - Replaces traditional Pipeline State Objects
  - Reduces shader compilation stutters
  - More dynamic state management
  - Faster than PSOs on modern hardware
- **Trade-offs**: Not yet universally supported

**Note for WGSL Projects:**  
Shader objects work with WGSL via SPIR-V compilation. The main difference is that WGSL projects typically use uniform buffers instead of push constants for small data.

### 2. Multi-Viewport Architecture

#### **Texture Array Approach** (Recommended)
```rust
// Create texture array for multiple viewports
let viewport_textures = vec![
    create_viewport_texture(0),
    create_viewport_texture(1),
    create_viewport_texture(2),
];

// Register with bindless system
for (i, texture) in viewport_textures.iter().enumerate() {
    let slot = register_bindless_texture(texture);
    viewport_slots[i] = slot;
}
```

```glsl
// Compositing shader
layout(binding = 0) uniform sampler2D viewportTextures[];
// For WGSL: use uniform buffer instead of push constant
layout(binding = 1) uniform ViewportParams {
    vec4 viewportRects[MAX_VIEWPORTS];
    int viewportCount;
} params;

void main() {
    vec2 pixelPos = gl_FragCoord.xy;
    vec4 result = vec4(0.0);
    
    for(int i = 0; i < params.viewportCount; i++) {
        vec4 rect = params.viewportRects[i];
        if(pixelPos.x >= rect.x && pixelPos.x <= rect.z &&
           pixelPos.y >= rect.y && pixelPos.y <= rect.w) {
            vec2 localUV = (pixelPos - rect.xy) / (rect.zw - rect.xy);
            result = texture(viewportTextures[i], localUV);
        }
    }
    
    outColor = result;
}
```

**WGSL Equivalent:**
```wgsl
@group(0) @binding(0) var viewportTextures: array<texture_2d<f32>>;
struct ViewportRect {
    x: f32, y: f32, z: f32, w: f32,
};
struct ViewportParams {
    rects: array<ViewportRect, 4>,
    count: u32,
};
@group(0) @binding(1) var<uniform> params: ViewportParams;

@fragment
fn main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let pixelPos = uv * screenSize;
    var result: vec4<f32> = vec4<f32>(0.0);
    
    for (var i: u32 = 0u; i < params.count; i++) {
        let rect = params.rects[i];
        if (pixelPos.x >= rect.x && pixelPos.x <= rect.z &&
            pixelPos.y >= rect.y && pixelPos.y <= rect.w) {
            let localUV = (pixelPos - rect.xy) / (rect.zw - rect.xy);
            result = textureSample(viewportTextures[i], sampler, localUV);
        }
    }
    
    return result;
}
```

#### **Frame Graph Integration**
```rust
// Define passes for each viewport
let graph = FrameGraph::builder()
    .add_pass(ViewportPass::new("viewport_0").write("viewport_0"))
    .add_pass(ViewportPass::new("viewport_1").write("viewport_1"))
    .add_pass(CompositePass::new("composite")
        .read("viewport_0")
        .read("viewport_1")
        .write("backbuffer"))
    .build(&renderer)?;
```

### 3. UI Integration Patterns

#### **Pattern 1: Separate Pass Approach** (Recommended)
- UI render pass after scene rendering
- Scene output sampled as texture in UI shaders
- More flexible, works with dynamic rendering
- Better for complex UI with scene integration

```rust
let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("geometry")
        .write_color("scene_color", format)
        .write_depth("depth", format))
    .add_pass(UIPass::new("ui")
        .read("scene_color")  // Sample scene in UI shaders
        .write("backbuffer"))
    .build(&renderer)?;
```

```glsl
// UI shader with scene integration
layout(binding = 0) uniform sampler2D sceneTexture;
layout(binding = 1) uniform sampler2D uiTexture;

vec4 sceneColor = texture(sceneTexture, screenUV);
vec4 uiColor = texture(uiTexture, uv);
outColor = mix(sceneColor, uiColor, uiColor.a);
```

#### **Pattern 2: Subpass Approach** (Traditional)
- UI as final subpass in main render pass
- Input attachment access for depth buffer sharing
- Automatic subpass dependencies
- Limited flexibility

#### **Pattern 3: Overlay Approach** (Simple)
- UI rendered directly over swapchain
- Scene texture sampled by UI elements
- Alpha blending for composition
- Best for traditional game UI

### 4. Synchronization Best Practices

#### **Automatic Synchronization via Render Graph**
```rust
// Render graph handles barriers automatically
impl FrameGraph {
    fn insert_barriers(&mut self, pass_index: usize) {
        let pass = &self.passes[pass_index];
        
        for write_name in &pass.writes {
            let required_state = ResourceState::ColorAttachment;
            let current_state = self.get_resource_state(write_name);
            
            if current_state != required_state {
                self.transition_resource(write_name, required_state);
            }
        }
        
        for read_name in &pass.reads {
            let required_state = ResourceState::ShaderRead;
            let current_state = self.get_resource_state(read_name);
            
            if current_state != required_state {
                self.transition_resource(read_name, required_state);
            }
        }
    }
}
```

#### **Layout Transition Tracking**
```rust
pub struct TransientTexture {
    image: vk::Image,
    current_layout: RefCell<vk::ImageLayout>,
}

impl TransientTexture {
    fn transition(&self, cmd: &CommandBuffer, new_layout: vk::ImageLayout) {
        let old_layout = *self.current_layout.borrow();
        
        if old_layout != new_layout {
            let barrier = vk::ImageMemoryBarrier2::default()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS);
            
            cmd.pipeline_barrier2(&barrier);
            *self.current_layout.borrow_mut() = new_layout;
        }
    }
}
```

### 5. Performance Optimization

#### **Memory Aliasing**
- Reuse textures across non-overlapping passes
- Transient attachments for temporary data
- Ring buffers for per-frame data

```rust
// Resource aliasing example
let gbuffer_pass = Pass::new("gbuffer")
    .create_texture("color0", format, size)
    .create_texture("color1", format, size)
    .create_texture("depth", format, size);

let tonemap_pass = Pass::new("tonemap")
    .alias_texture("color0", "tonemap_temp");  // Reuse color0
```

#### **Async Compute**
- UI preparation in compute queue
- Parallel scene rendering
- GPU-driven UI updates

```rust
// Async compute for UI
let compute_queue = device.get_compute_queue();
compute_queue.submit_ui_prepare();
graphics_queue.submit_scene_render();
graphics_queue.submit_ui_render();
```

#### **Pipeline Caching**
- Cache PSO permutations
- Shader object precompilation
- Descriptor set layout sharing

### 6. State-of-the-Art Implementations

**Recommended Projects to Study:**
- **Granite Engine** (Hans-Kristian Arntzen)
  - Advanced render graph implementation
  - Automatic barrier placement
  - Resource aliasing
- **Vuk** (Marcell Kiss)
  - Modern Rust Vulkan framework
  - Excellent render graph design
  - Type-safe API
- **bgfx**
  - Multi-api renderer
  - Production-proven architecture
- **Forge**
  - BGFX-inspired
  - Modern C++ design

### 7. Shader Architecture

#### **Viewport Compositing Shader**
```glsl
#version 460

layout(binding = 0) uniform sampler2D viewportTextures[];
layout(push_constant) uniform ViewportParams {
    vec4 viewportRects[MAX_VIEWPORTS];
    int viewportCount;
} params;

layout(location = 0) in vec2 inUV;
layout(location = 0) out vec4 outColor;

void main() {
    vec2 pixelPos = inUV * screenSize;
    outColor = vec4(0.0);
    
    for(int i = 0; i < params.viewportCount; i++) {
        vec4 rect = params.viewportRects[i];
        if(pixelPos.x >= rect.x && pixelPos.x <= rect.z &&
           pixelPos.y >= rect.y && pixelPos.y <= rect.w) {
            vec2 localUV = (pixelPos - rect.xy) / (rect.zw - rect.xy);
            outColor = texture(viewportTextures[i], localUV);
        }
    }
}
```

#### **UI Shader with Scene Integration**
```glsl
#version 460

layout(binding = 0) uniform sampler2D sceneTexture;
layout(binding = 1) uniform sampler2D uiTextures[];
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 outColor;

// Scene integration
vec4 sceneColor = texture(sceneTexture, screenUV);
vec4 uiColor = texture(uiTextures[materialIndex], uv);
outColor = mix(sceneColor, uiColor, uiColor.a);
```

## Key Takeaways

1. **Dynamic rendering** is the future for desktop
2. **Bindless descriptors** are essential for modern architectures
3. **Shader objects** reduce compilation overhead
4. **Render graphs** provide automatic synchronization
5. **Multi-viewport** works best with texture arrays + final composite
6. **UI integration** should leverage the render graph's barrier system

## References

- [Using Modern Vulkan in 2025](https://medium.com/@allenphilip78/using-modern-vulkan-in-2025-0bac45174304) by Allen Philip
- [Building a Vulkan Render Graph](https://tadriansen.dev/2025-04-21-building-a-vulkan-render-graph/) by Tony Adriansen
- [Render graphs and Vulkan — a deep dive](https://themaister.net/blog/2017/08/15/render-graphs-and-vulkan-a-deep-dive/) by Hans-Kristian Arntzen
- [Vulkan Dynamic Rendering](https://quadbit.medium.com/vulkan-dynamic-rendering-f993a9a8ca58)
- [VK_KHR_dynamic_rendering tutorial](https://lesleylai.info/en/vk-khr-dynamic-rendering/)
