# Metal Backend Implementation Plan

Native Metal backend for the `katla_gfx` crate, targeting macOS via `objc2-metal`.

This document is the implementation reference. For the Vulkan-to-Metal API mapping, see `vulkan_to_metal_mapping.md`.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Crate Dependencies](#2-crate-dependencies)
3. [Module Structure](#3-module-structure)
4. [Backend Trait Definitions](#4-backend-trait-definitions)
5. [Metal Context Implementation](#5-metal-context-implementation)
6. [Command Recording](#6-command-recording)
7. [Resource Management](#7-resource-management)
8. [Shader Compilation Pipeline](#8-shader-compilation-pipeline)
9. [Pipeline State](#9-pipeline-state)
10. [Bindless Textures via Argument Buffers](#10-bindless-textures-via-argument-buffers)
11. [Render Graph Integration](#11-render-graph-integration)
12. [Swapchain and Presentation](#12-swapchain-and-presentation)
13. [Synchronization Model](#13-synchronization-model)
14. [Format and Enum Conversion Tables](#14-format-and-enum-conversion-tables)
15. [Platform Considerations](#15-platform-considerations)
16. [Testing Strategy](#16-testing-strategy)
17. [Migration Checklist](#17-migration-checklist)

---

## 1. Architecture Overview

### Current State

```
katla_app
  └── VulkanRenderer (concrete struct)
        └── VulkanContext (concrete struct, wraps ash::Device, ash::Instance)
              └── ash (Vulkan bindings)
                    └── MoltenVK → Metal (runtime translation)
```

### Target State

```
katla_app
  └── Renderer (generic over B: GpuBackend)
        └── GpuContext<B: GpuBackend>
              ├── VulkanContext  → ash → GPU drivers / MoltenVK
              └── MetalContext   → objc2-metal → Metal framework
```

### Design Principles

- **No trait objects** — generic over backend type, monomorphized at compile time.
- **cfg-based selection** — `#[cfg(feature = "metal")]` / `#[cfg(feature = "vulkan")]` for zero-cost backend selection.
- **No hybrid state** — one active backend per build, selected at compile time.
- **Reuse existing Katla-native types** — `Handle<T>`, `ImageFormat`, `LoadOp`, `StoreOp`, `ClearValue`, `CompareOp`, `CullMode`, `FrontFace`, `DrawList`, `FrameUniforms`, pass templates, and the render graph compiler all stay untouched.
- **Backend owns its resource types** — `GpuImage`, `GpuBuffer`, `GpuPipeline` are associated types on the backend trait, implemented concretely per backend.

---

## 2. Crate Dependencies

### New dependencies for `katla_gfx/Cargo.toml`

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-metal = { version = "0.3", features = ["MTLDevice", "MTLCommandQueue", "MTLCommandBuffer", "MTLRenderCommandEncoder", "MTLComputeCommandEncoder", "MTLBlitCommandEncoder", "MTLTexture", "MTLBuffer", "MTLSampler", "MTLPipeline", "MTLArgumentEncoder", "MTLFence", "MTLEvent", "MTLHeap", "MTLBinaryArchive"] }
objc2-quartz-core = { version = "0.3", features = ["CAMetalLayer"] }
objc2-foundation = { version = "0.3", features = ["NSThread", "NSArray", "NSString"] }
block2 = "0.6"  # For completion handlers

# Shader compilation (already a workspace dependency, just need msl feature)
naga = { workspace = true, features = ["wgsl-in", "msl-out"] }
```

### Existing dependencies that remain

```toml
ash = { workspace = true, optional = true }       # Vulkan backend
gpu-allocator = { workspace = true, optional = true }  # Vulkan only
naga = { workspace = true }                         # Shader IR (both backends)
raw-window-handle = { workspace = true }
bytemuck = { workspace = true }
```

### Feature flags

```toml
[features]
default = ["vulkan"]
vulkan = ["dep:ash", "dep:gpu-allocator"]
metal = []
```

---

## 3. Module Structure

```
katla_gfx/src/
├── lib.rs                      # pub use backend::Renderer
├── handle.rs                   # Handle<T> (unchanged)
├── error.rs                    # RendererError (add Metal error variant)
├── pipeline.rs                 # Katla-native enums (unchanged)
├── vertex.rs                   # Vertex types (unchanged)
├── viewport.rs                 # Viewport management (generic over backend)
├── compute.rs                  # Compute dispatch helpers (generic)
├── size.rs                     # Size2D (unchanged)
│
├── backend/                    # NEW: Backend abstraction layer
│   ├── mod.rs                  # GpuBackend trait, public re-exports
│   ├── traits.rs               # Core trait definitions
│   ├── resource.rs             # GpuImage, GpuBuffer, GpuPipeline associated types
│   ├── command.rs              # GpuCommandBuffer trait
│   ├── sync.rs                 # GpuFence, GpuSemaphore traits
│   └── format.rs               # Backend-agnostic format enums (reuse ImageFormat)
│
├── vulkan/                     # EXISTING: Vulkan backend (behind #[cfg(feature = "vulkan")])
│   ├── context/
│   ├── material/
│   ├── ... (unchanged internals)
│
├── metal/                      # NEW: Metal backend (behind #[cfg(feature = "metal")])
│   ├── mod.rs                  # pub(crate) re-exports
│   ├── context.rs              # MetalContext (MTLDevice, MTLCommandQueue)
│   ├── surface.rs              # CAMetalLayer setup and drawable management
│   ├── command_buffer.rs       # MetalCommandBuffer wrapper
│   ├── render_encoder.rs       # MTLRenderCommandEncoder wrapper
│   ├── compute_encoder.rs      # MTLComputeCommandEncoder wrapper
│   ├── blit_encoder.rs         # MTLBlitCommandEncoder wrapper
│   ├── buffer.rs               # MetalBuffer (MTLBuffer wrapper)
│   ├── texture.rs              # MetalTexture (MTLTexture wrapper)
│   ├── texture_view.rs         # Texture view creation
│   ├── pipeline.rs             # MetalGraphicsPipeline, MetalComputePipeline
│   ├── depth_stencil.rs        # MTLDepthStencilState management
│   ├── sampler.rs              # MTLSamplerState creation
│   ├── shader.rs               # WGSL → MSL compilation via naga
│   ├── argument_buffer.rs      # Bindless texture array via MTLArgumentEncoder
│   ├── sync.rs                 # MTLSharedEvent, dispatch_semaphore_t wrappers
│   └── format.rs               # ImageFormat → MTLPixelFormat conversions
│
├── renderer/                   # REFACTORED: Generic over backend
│   ├── mod.rs                  # Renderer<B: GpuBackend> (was VulkanRenderer)
│   ├── types.rs                # FrameUniforms, InstanceData, DrawList (unchanged)
│   ├── frame_lifecycle.rs      # Generic frame begin/end
│   ├── mesh_manager.rs         # Generic mesh management
│   ├── material_api.rs         # Generic material compilation
│   ├── texture_api.rs          # Generic texture creation
│   ├── destroy_api.rs          # Generic resource destruction
│   ├── shadow.rs               # Generic shadow pass
│   ├── depth_prepass.rs        # Generic depth prepass
│   ├── ...
│
├── render_graph/               # PARTIALLY REFACTORED
│   ├── compiler.rs             # Unchanged (pure analysis, no GPU code)
│   ├── builder.rs              # Unchanged (pass template abstraction)
│   ├── pass.rs                 # Unchanged (abstract pass descriptions)
│   ├── resource.rs             # Unchanged (abstract resource declarations)
│   ├── frame_graph.rs          # Generic over backend (resource creation + execution)
│   ├── transient_texture.rs    # Generic over backend
│   ├── frame/                  # Command recording (generic over GpuCommandBuffer)
│   ├── passes/                 # Pass templates (unchanged declarations)
│   └── descriptor_sets/        # Generic over backend
│
├── texture/                    # Unchanged (handle-based, backend-agnostic)
├── render_pass/                # Unchanged (LoadOp, StoreOp, ClearValue)
├── material/                   # Unchanged (MaterialOptions, descriptors)
├── particles/                  # Refactored to use generic backend
├── animation/                  # Refactored to use generic backend
├── shadow/                     # Unchanged (types only)
├── lighting/                   # Unchanged (types only)
└── primitives/                 # Unchanged (CPU-side mesh generation)
```

---

## 4. Backend Trait Definitions

### Core Backend Trait

```rust
// backend/traits.rs

use crate::handle::Handle;
use crate::pipeline::*;
use crate::vertex::VertexType;

/// Marker type for resource handles.
pub struct BackendResource;

/// Core GPU backend trait. Implemented once per graphics API.
pub trait GpuBackend: Sized + 'static {
    // -- Associated types --

    type Context: GpuContext<Self>;
    type CommandBuffer: GpuCommandBuffer<Self>;
    type RenderEncoder: GpuRenderEncoder;
    type ComputeEncoder: GpuComputeEncoder;
    type BlitEncoder: GpuBlitEncoder;

    type Image: GpuImage;
    type ImageView: GpuImageView;
    type Buffer: GpuBuffer;
    type GraphicsPipeline: GpuGraphicsPipeline;
    type ComputePipeline: GpuComputePipeline;
    type Sampler: GpuSampler;
    type Fence: GpuFence;
    type Event: GpuEvent;

    // -- Format conversion --

    type NativeFormat: Copy + Clone + Debug;
    type NativeImageLayout: Copy + Clone + Debug;

    // -- Backend identification --

    fn name() -> &'static str;
}
```

### GpuContext Trait

```rust
/// GPU device and queue management.
pub trait GpuContext<B: GpuBackend>: Sized {
    // -- Initialization --

    fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        validation: ValidationMode,
        app_name: &CString,
        engine_name: &CString,
    ) -> Result<Self, RendererError>;

    fn init_headless(
        validation: ValidationMode,
        app_name: &CString,
        engine_name: &CString,
    ) -> Result<Self, RendererError>;

    // -- Command buffers --

    fn create_command_buffer(&self) -> B::CommandBuffer;

    // -- Resource creation --

    fn create_buffer(
        &self,
        size: u64,
        usage: BufferUsage,
        memory_location: MemoryLocation,
    ) -> Result<B::Buffer, RendererError>;

    fn create_texture(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(B::Image, B::ImageView), RendererError>;

    fn create_texture_view(
        &self,
        image: &B::Image,
        format: ImageFormat,
    ) -> Result<B::ImageView, RendererError>;

    fn create_sampler(
        &self,
        filter: Filter,
        address_mode: AddressMode,
        anisotropy: Option<f32>,
    ) -> Result<B::Sampler, RendererError>;

    // -- Pipeline creation --

    fn create_graphics_pipeline(
        &self,
        descriptor: &GraphicsPipelineDescriptor<B>,
    ) -> Result<B::GraphicsPipeline, RendererError>;

    fn create_compute_pipeline(
        &self,
        shader_source: &str,      // WGSL source
        entry_point: &str,
    ) -> Result<B::ComputePipeline, RendererError>;

    // -- Presentation --

    fn acquire_next_image(&self) -> Result<(B::ImageView, u32), RendererError>;
    fn present(&self, image_index: u32) -> Result<(), RendererError>;
    fn resize(&self, width: u32, height: u32);

    // -- Synchronization --

    fn create_fence(&self, signaled: bool) -> Result<B::Fence, RendererError>;
    fn create_event(&self) -> Result<B::Event, RendererError>;
    fn wait_fence(&self, fence: &B::Fence) -> Result<(), RendererError>;
    fn reset_fence(&self, fence: &B::Fence) -> Result<(), RendererError>;

    // -- Frame lifecycle --

    fn begin_frame(&self) -> Result<(), RendererError>;
    fn end_frame(&self) -> Result<(), RendererError>;
    fn frame_index(&self) -> u32;

    // -- Device properties --

    fn max_texture_size(&self) -> u32;
    fn max_bindless_textures(&self) -> u32;
    fn supports_buffer_device_address(&self) -> bool;
}
```

### GpuCommandBuffer Trait

```rust
/// Command buffer for recording GPU work.
pub trait GpuCommandBuffer<B: GpuBackend> {
    fn begin(&mut self);
    fn end(&mut self);
    fn submit(&self, context: &B::Context);

    // -- Encoder creation --

    fn begin_render_pass(&mut self, desc: &RenderPassDescriptor<B>) -> B::RenderEncoder;
    fn begin_compute_pass(&mut self) -> B::ComputeEncoder;
    fn begin_blit_pass(&mut self) -> B::BlitEncoder;

    // -- Synchronization --

    fn pipeline_barrier(&mut self, barriers: &[BarrierInfo<B>]);

    // -- Copy operations --

    fn copy_buffer_to_texture(
        &mut self,
        src: &B::Buffer,
        dst: &B::Image,
        regions: &[BufferImageCopy],
    );
}
```

### GpuRenderEncoder Trait

```rust
/// Render pass command recording.
pub trait GpuRenderEncoder {
    fn end_encoding(self);

    // -- Binding --

    fn bind_graphics_pipeline(&mut self, pipeline: &impl GpuGraphicsPipeline);
    fn bind_vertex_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32);
    fn bind_index_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index_type: IndexType);
    fn bind_storage_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32, stages: ShaderStages);
    fn bind_texture(&mut self, view: &impl GpuImageView, index: u32, stages: ShaderStages);
    fn bind_sampler(&mut self, sampler: &impl GpuSampler, index: u32, stages: ShaderStages);
    fn bind_argument_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32, stages: ShaderStages);
    fn set_push_constants(&mut self, data: &[u8], index: u32, stages: ShaderStages);

    // -- Dynamic state --

    fn set_viewport(&mut self, viewport: &Viewport);
    fn set_scissor(&mut self, scissor: &Rect2D);
    fn set_depth_bias(&mut self, bias: f32, slope: f32, clamp: f32);
    fn set_stencil_reference(&mut self, front: u32, back: u32);

    // -- Draw --

    fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32);
    fn draw_indexed(&mut self, index_count: u32, instance_count: u32, first_index: u32, vertex_offset: i32, first_instance: u32);
}
```

### Resource Traits

```rust
/// GPU buffer resource.
pub trait GpuBuffer: Sized {
    fn size(&self) -> u64;
    fn map(&self) -> *mut u8;
    fn unmap(&self);
    fn flush(&self, offset: u64, size: u64);
    fn gpu_address(&self) -> u64;
}

/// GPU image resource.
pub trait GpuImage: Sized {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn format(&self) -> ImageFormat;
    fn mip_levels(&self) -> u32;
}

/// GPU image view.
pub trait GpuImageView: Sized {
    fn image(&self) -> &impl GpuImage;
}

/// GPU graphics pipeline.
pub trait GpuGraphicsPipeline: Clone {}

/// GPU compute pipeline.
pub trait GpuComputePipeline: Clone {
    fn workgroup_size(&self) -> [u32; 3];
}

/// GPU sampler.
pub trait GpuSampler: Clone {}

/// GPU fence for CPU-GPU synchronization.
pub trait GpuFence {
    fn is_signaled(&self) -> bool;
}

/// GPU event for GPU-GPU synchronization.
pub trait GpuEvent {}
```

---

## 5. Metal Context Implementation

### MetalContext

```rust
// metal/context.rs

use objc2::runtime::AnyObject;
use objc2_metal::{
    MTLDevice, MTLCommandQueue, MTLCommandBuffer,
    MTLCreateSystemDefaultDevice,
};
use objc2_quartz_core::CAMetalLayer;

pub struct MetalContext {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    surface: MetalSurface,
    frame_index: u32,
    frames_in_flight: u32,
}

pub struct MetalBackend;

impl GpuBackend for MetalBackend {
    type Context = MetalContext;
    type CommandBuffer = MetalCommandBuffer;
    type RenderEncoder = MetalRenderEncoder;
    type ComputeEncoder = MetalComputeEncoder;
    type BlitEncoder = MetalBlitEncoder;
    type Image = MetalTexture;
    type ImageView = MetalTextureView;
    type Buffer = MetalBuffer;
    type GraphicsPipeline = MetalGraphicsPipeline;
    type ComputePipeline = MetalComputePipeline;
    type Sampler = MetalSamplerState;
    type Fence = MetalFence;
    type Event = MetalEvent;
    type NativeFormat = MTLPixelFormat;
    type NativeImageLayout = ();  // Metal has no image layouts

    fn name() -> &'static str { "Metal" }
}

impl GpuContext<MetalBackend> for MetalContext {
    fn init(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        validation: ValidationMode,
        app_name: &CString,
        engine_name: &CString,
    ) -> Result<Self, RendererError> {
        let device = unsafe { MTLCreateSystemDefaultDevice() }
            .ok_or(RendererError::InitializationFailed("No Metal device".into()))?;

        let command_queue = device
            .newCommandQueue()
            .ok_or(RendererError::InitializationFailed("Failed to create command queue".into()))?;

        let surface = MetalSurface::new(display, window, &device)?;

        Ok(Self {
            device,
            command_queue,
            surface,
            frame_index: 0,
            frames_in_flight: 2,
        })
    }

    fn init_headless(
        validation: ValidationMode,
        app_name: &CString,
        engine_name: &CString,
    ) -> Result<Self, RendererError> {
        let device = unsafe { MTLCreateSystemDefaultDevice() }
            .ok_or(RendererError::InitializationFailed("No Metal device".into()))?;

        let command_queue = device
            .newCommandQueue()
            .ok_or(RendererError::InitializationFailed("Failed to create command queue".into()))?;

        Ok(Self {
            device,
            command_queue,
            surface: MetalSurface::headless(),
            frame_index: 0,
            frames_in_flight: 2,
        })
    }

    fn create_command_buffer(&self) -> MetalCommandBuffer {
        let cmd_buffer = self.command_queue
            .commandBuffer()
            .expect("Failed to allocate command buffer");
        MetalCommandBuffer::new(cmd_buffer)
    }

    fn create_buffer(
        &self,
        size: u64,
        usage: BufferUsage,
        memory_location: MemoryLocation,
    ) -> Result<MetalBuffer, RendererError> {
        let options = match memory_location {
            MemoryLocation::GpuOnly => MTLResourceOptions::StorageModePrivate,
            MemoryLocation::CpuToGpu | MemoryLocation::GpuToCpu => {
                MTLResourceOptions::StorageModeShared
            }
        };
        let buffer = self.device
            .newBufferWithLength_options(size, options)
            .ok_or(RendererError::ResourceCreationFailed("Buffer".into()))?;
        Ok(MetalBuffer { inner: buffer, size })
    }

    fn supports_buffer_device_address(&self) -> bool {
        // buffer.gpuAddress requires Metal 3 (macOS 13+)
        // Check at runtime via available!()
        unsafe { available!(macos = 13.0) }
    }

    // ... remaining trait methods
}
```

---

## 6. Command Recording

### MetalCommandBuffer

```rust
// metal/command_buffer.rs

pub struct MetalCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    active_encoder: Option<EncoderKind>,
}

enum EncoderKind {
    Render(MetalRenderEncoder),
    Compute(MetalComputeEncoder),
    Blit(MetalBlitEncoder),
}

impl GpuCommandBuffer<MetalBackend> for MetalCommandBuffer {
    fn begin(&mut self) {
        // Metal command buffers are created ready-to-record.
        // No explicit begin needed.
    }

    fn end(&mut self) {
        // End any active encoder first.
        self.active_encoder.take();
    }

    fn submit(&self, _context: &MetalContext) {
        self.inner.commit();
    }

    fn begin_render_pass(&mut self, desc: &RenderPassDescriptor<MetalBackend>) -> MetalRenderEncoder {
        // End current encoder if any.
        self.active_encoder.take();

        let pass_desc = unsafe { MTLRenderPassDescriptor::new() };

        // Configure color attachments.
        for (i, attachment) in desc.color_attachments.iter().enumerate() {
            let color_desc = pass_desc.colorAttachments().objectAtIndexedSubscript(i);
            color_desc.setTexture(Some(&attachment.texture.inner));
            color_desc.setLoadAction(to_mtl_load_action(attachment.load_op));
            color_desc.setStoreAction(to_mtl_store_action(attachment.store_op));
            if attachment.load_op == LoadOp::Clear {
                color_desc.setClearColor(MTLClearColor::new(
                    attachment.clear_value.r,
                    attachment.clear_value.g,
                    attachment.clear_value.b,
                    attachment.clear_value.a,
                ));
            }
        }

        // Configure depth attachment.
        if let Some(ref depth) = desc.depth_attachment {
            let depth_desc = pass_desc.depthAttachment();
            depth_desc.setTexture(Some(&depth.texture.inner));
            depth_desc.setLoadAction(to_mtl_load_action(depth.load_op));
            depth_desc.setStoreAction(to_mtl_store_action(depth.store_op));
            if depth.load_op == LoadOp::Clear {
                depth_desc.setClearDepth(depth.clear_value.depth);
            }
        }

        let encoder = self.inner
            .renderCommandEncoderWithDescriptor(&pass_desc)
            .expect("Failed to create render encoder");

        let render_encoder = MetalRenderEncoder::new(encoder);
        self.active_encoder = Some(EncoderKind::Render(render_encoder.clone()));
        render_encoder
    }

    fn begin_compute_pass(&mut self) -> MetalComputeEncoder {
        self.active_encoder.take();
        let encoder = self.inner
            .computeCommandEncoder()
            .expect("Failed to create compute encoder");
        let compute_encoder = MetalComputeEncoder::new(encoder);
        self.active_encoder = Some(EncoderKind::Compute(compute_encoder.clone()));
        compute_encoder
    }

    fn begin_blit_pass(&mut self) -> MetalBlitEncoder {
        self.active_encoder.take();
        let encoder = self.inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder");
        let blit_encoder = MetalBlitEncoder::new(encoder);
        self.active_encoder = Some(EncoderKind::Blit(blit_encoder.clone()));
        blit_encoder
    }

    fn pipeline_barrier(&mut self, barriers: &[BarrierInfo<MetalBackend>]) {
        // Metal tracks resource state implicitly between encoders.
        // Within an encoder, use MTLFence for ordering.
        // Most Vulkan barriers become no-ops on Metal.
    }
}
```

### MetalRenderEncoder

```rust
// metal/render_encoder.rs

pub struct MetalRenderEncoder {
    inner: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
}

impl GpuRenderEncoder for MetalRenderEncoder {
    fn end_encoding(self) {
        self.inner.endEncoding();
    }

    fn bind_graphics_pipeline(&mut self, pipeline: &impl GpuGraphicsPipeline) {
        let pipeline = pipeline.as_metal();
        self.inner.setRenderPipelineState(&pipeline.pipeline_state);

        if let Some(ref depth_stencil) = pipeline.depth_stencil_state {
            self.inner.setDepthStencilState(depth_stencil);
        }

        // Set fixed-function state.
        self.inner.setCullMode(pipeline.cull_mode);
        self.inner.setFrontFacingVertexWinding(pipeline.front_face);
        if let Some(fill_mode) = pipeline.fill_mode {
            self.inner.setTriangleFillMode(fill_mode);
        }
    }

    fn bind_vertex_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32) {
        self.inner.setVertexBuffer_atIndex(offset, buffer.as_metal().inner, index);
    }

    fn bind_index_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index_type: IndexType) {
        // Index buffer is bound at draw time in Metal.
        // Store for later use in draw_indexed.
    }

    fn bind_storage_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32, stages: ShaderStages) {
        if stages.contains(ShaderStages::VERTEX) {
            self.inner.setVertexBuffer_atIndex(offset, buffer.as_metal().inner, index);
        }
        if stages.contains(ShaderStages::FRAGMENT) {
            self.inner.setFragmentBuffer_atIndex(offset, buffer.as_metal().inner, index);
        }
    }

    fn bind_texture(&mut self, view: &impl GpuImageView, index: u32, stages: ShaderStages) {
        if stages.contains(ShaderStages::VERTEX) {
            self.inner.setVertexTexture_atIndex(view.as_metal().inner, index);
        }
        if stages.contains(ShaderStages::FRAGMENT) {
            self.inner.setFragmentTexture_atIndex(view.as_metal().inner, index);
        }
    }

    fn bind_sampler(&mut self, sampler: &impl GpuSampler, index: u32, stages: ShaderStages) {
        if stages.contains(ShaderStages::VERTEX) {
            self.inner.setVertexSamplerState_atIndex(sampler.as_metal().inner, index);
        }
        if stages.contains(ShaderStages::FRAGMENT) {
            self.inner.setFragmentSamplerState_atIndex(sampler.as_metal().inner, index);
        }
    }

    fn bind_argument_buffer(&mut self, buffer: &impl GpuBuffer, offset: u64, index: u32, stages: ShaderStages) {
        // Same as bind_storage_buffer — argument buffers are just MTLBuffers.
        self.bind_storage_buffer(buffer, offset, index, stages);
    }

    fn set_push_constants(&mut self, data: &[u8], index: u32, stages: ShaderStages) {
        // Metal has no push constants. Use setBytes instead.
        if stages.contains(ShaderStages::VERTEX) {
            self.inner.setVertexBytes_length_atIndex(data, data.len(), index);
        }
        if stages.contains(ShaderStages::FRAGMENT) {
            self.inner.setFragmentBytes_length_atIndex(data, data.len(), index);
        }
    }

    fn set_viewport(&mut self, viewport: &Viewport) {
        let mtl_viewport = MTLViewport {
            originX: viewport.x as f64,
            originY: viewport.y as f64,
            width: viewport.width as f64,
            height: viewport.height as f64,
            znear: viewport.min_depth as f64,
            zfar: viewport.max_depth as f64,
        };
        self.inner.setViewport(mtl_viewport);
    }

    fn set_scissor(&mut self, scissor: &Rect2D) {
        let mtl_scissor = MTLScissorRect {
            x: scissor.x,
            y: scissor.y,
            width: scissor.width,
            height: scissor.height,
        };
        self.inner.setScissorRect(mtl_scissor);
    }

    fn set_depth_bias(&mut self, bias: f32, slope: f32, clamp: f32) {
        self.inner.setDepthBias_slopeScale_clamp(bias, slope, clamp);
    }

    fn draw_indexed(&mut self, index_count: u32, instance_count: u32, first_index: u32, vertex_offset: i32, first_instance: u32) {
        self.inner.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset_instanceCount(
            MTLPrimitiveType::Triangle,
            index_count,
            self.index_type,       // stored from bind_index_buffer
            &self.index_buffer,    // stored from bind_index_buffer
            self.index_offset,
            instance_count,
        );
    }

    fn draw(&mut self, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32) {
        self.inner.drawPrimitives_vertexStart_vertexCount_instanceCount(
            MTLPrimitiveType::Triangle,
            first_vertex,
            vertex_count,
            instance_count,
        );
    }
}
```

---

## 7. Resource Management

### MetalBuffer

```rust
// metal/buffer.rs

pub struct MetalBuffer {
    inner: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: u64,
}

impl GpuBuffer for MetalBuffer {
    fn size(&self) -> u64 { self.size }

    fn map(&self) -> *mut u8 {
        // StorageModeShared buffers are always mapped.
        unsafe { self.inner.contents() as *mut u8 }
    }

    fn unmap(&self) {
        // No-op for StorageModeShared.
        // For StorageModeManaged, would need didModifyRange.
    }

    fn flush(&self, offset: u64, size: u64) {
        // Only needed for StorageModeManaged (non-UMA discrete GPUs).
        #[cfg(target_arch = "x86_64")]
        {
            if self.inner.resourceOptions() & MTLResourceOptions::StorageModeManaged != 0 {
                unsafe { self.inner.didModifyRange(NSRange::new(offset, size)) };
            }
        }
    }

    fn gpu_address(&self) -> u64 {
        // Requires Metal 3 (macOS 13+).
        // Falls back to 0 if unavailable.
        unsafe {
            if available!(macos = 13.0) {
                self.inner.gpuAddress()
            } else {
                0
            }
        }
    }
}
```

### MetalTexture

```rust
// metal/texture.rs

pub struct MetalTexture {
    inner: Retained<ProtocolObject<dyn MTLTexture>>,
    descriptor: TextureDescriptor,
}

pub struct MetalTextureView {
    inner: Retained<ProtocolObject<dyn MTLTexture>>,
    parent: MetalTexture,
}

impl GpuImage for MetalTexture {
    fn width(&self) -> u32 { self.inner.width() }
    fn height(&self) -> u32 { self.inner.height() }
    fn format(&self) -> ImageFormat { self.descriptor.format }
    fn mip_levels(&self) -> u32 { self.inner.mipmapLevelCount() }
}

impl GpuImageView for MetalTextureView {
    fn image(&self) -> &impl GpuImage { &self.parent }
}
```

### Texture Creation

```rust
impl MetalContext {
    pub fn create_texture(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(MetalTexture, MetalTextureView), RendererError> {
        let tex_desc = unsafe { MTLTextureDescriptor::new() };

        tex_desc.setTextureType(to_mtl_texture_type(descriptor.dim));
        tex_desc.setPixelFormats(to_mtl_pixel_format(descriptor.format));
        tex_desc.setWidth(descriptor.width);
        tex_desc.setHeight(descriptor.height);
        tex_desc.setMipmapLevelCount(descriptor.mip_levels);

        let usage = to_mtl_texture_usage(descriptor.usage);
        tex_desc.setUsage(usage);

        // Storage mode: Private for GPU-only (render targets, sampled textures).
        // Shared for CPU-uploaded data.
        let storage_mode = if descriptor.usage.intersects(TextureUsage::CPU_UPLOAD) {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Private
        };
        tex_desc.setStorageMode(storage_mode);

        let texture = self.device
            .newTextureWithDescriptor(&tex_desc)
            .ok_or(RendererError::ResourceCreationFailed("Texture".into()))?;

        let metal_texture = MetalTexture {
            inner: texture.clone(),
            descriptor: descriptor.clone(),
        };

        let view = MetalTextureView {
            inner: texture,  // In Metal, the texture IS the view by default.
            parent: metal_texture.clone(),
        };

        Ok((metal_texture, view))
    }
}
```

---

## 8. Shader Compilation Pipeline

### Current Pipeline (Vulkan)

```
WGSL source
  → naga (front::wgsl::parse_str)
    → naga::Module (IR)
      → naga::back::spv::write_vec (SPIR-V)
        → vk::ShaderModule
          → vk::PipelineShaderStageCreateInfo
```

### New Pipeline (Metal)

```
WGSL source
  → naga (front::wgsl::parse_str)
    → naga::Module (IR)
      → naga::back::msl::write_string (MSL source)
        → MTLDevice::newLibraryWithSource:options:error:
          → MTLLibrary
            → MTLLibrary::newFunctionWithName:
              → MTLFunction
                → Set on pipeline descriptor
```

### Implementation

```rust
// metal/shader.rs

use naga::back::msl::{self, Options, PipelineOptions, TranslationInfo};
use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

pub struct MetalShaderModule {
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    entry_points: HashMap<String, Retained<ProtocolObject<dyn MTLFunction>>>,
}

pub struct CompiledMetalShader {
    pub module: MetalShaderModule,
    pub naga_info: TranslationInfo,
}

/// Compile WGSL source to Metal shader library.
pub fn compile_wgsl_to_metal(
    device: &ProtocolObject<dyn MTLDevice>,
    wgsl_source: &str,
    entry_points: &[&str],
) -> Result<CompiledMetalShader, RendererError> {
    // 1. Parse WGSL to naga IR.
    let module = wgsl::parse_str(wgsl_source)
        .map_err(|e| RendererError::ShaderCompilationFailed(format!("WGSL parse: {:?}", e)))?;

    // 2. Validate the naga module.
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator.validate(&module)
        .map_err(|e| RendererError::ShaderCompilationFailed(format!("Validation: {:?}", e)))?;

    // 3. Generate MSL source.
    let msl_options = Options::default();
    let pipeline_options = PipelineOptions::default();

    let (msl_source, translation_info) = msl::write_string(&module, &info, &msl_options, &pipeline_options)
        .map_err(|e| RendererError::ShaderCompilationFailed(format!("MSL generation: {:?}", e)))?;

    // 4. Compile MSL to MTLLibrary.
    let source = NSString::from_str(&msl_source);
    let compile_options = unsafe { MTLCompileOptions::new() };
    compile_options.setLanguageVersion(MTLLanguageVersion::Version3_0);

    let library = unsafe {
        device.newLibraryWithSource_options_error(
            &source,
            &compile_options,
        )
    }
    .map_err(|err| {
        let msg = unsafe { err.localizedDescription() }.to_string();
        RendererError::ShaderCompilationFailed(format!("Metal compile: {}", msg))
    })?;

    // 5. Extract entry point functions.
    let mut functions = HashMap::new();
    for name in entry_points {
        let ns_name = NSString::from_str(name);
        let function = library
            .newFunctionWithName(&ns_name)
            .ok_or_else(|| RendererError::ShaderCompilationFailed(
                format!("Entry point '{}' not found in compiled library", name)
            ))?;
        functions.insert(name.to_string(), function);
    }

    Ok(CompiledMetalShader {
        module: MetalShaderModule {
            library,
            entry_points: functions,
        },
        naga_info: translation_info,
    })
}
```

### MSL Resource Binding Layout

When naga compiles WGSL to MSL, resource bindings are remapped. naga's `TranslationInfo` provides the mapping:

```
WGSL @group(0) @binding(0) → MSL [[buffer(0)]]
WGSL @group(0) @binding(1) → MSL [[buffer(1)]]
WGSL @group(1) @binding(0) → MSL [[texture(0)]]   (bindless array)
WGSL @group(1) @binding(1) → MSL [[sampler(0)]]
WGSL @group(2) @binding(0) → MSL [[buffer(2)]]
```

The `msl::Options` struct controls this mapping via `per_entry_point_map`. Configure it to match the encoder binding indices used in the render/compute encoder code.

---

## 9. Pipeline State

### MetalGraphicsPipeline

```rust
// metal/pipeline.rs

pub struct MetalGraphicsPipeline {
    pub pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    pub depth_stencil_state: Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
    pub cull_mode: MTLCullMode,
    pub front_face: MTLWinding,
    pub fill_mode: Option<MTLTriangleFillMode>,
    pub depth_bias: Option<(f32, f32, f32)>,  // (bias, slope_scale, clamp)
}

pub struct MetalComputePipeline {
    pub pipeline_state: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    pub workgroup_size: MTLSize,
}
```

### Graphics Pipeline Creation

```rust
impl MetalContext {
    pub fn create_graphics_pipeline(
        &self,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
        vertex_descriptor: Option<&MetalVertexDescriptor>,
        color_formats: &[MTLPixelFormat],
        depth_format: Option<MTLPixelFormat>,
        blend_state: Option<&BlendState>,
        sample_count: u32,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        let descriptor = unsafe { MTLRenderPipelineDescriptor::new() };

        // Shader functions.
        descriptor.setVertexFunction(Some(vertex_function));
        if let Some(fragment) = fragment_function {
            descriptor.setFragmentFunction(Some(fragment));
        }

        // Color attachments.
        for (i, format) in color_formats.iter().enumerate() {
            let color_desc = descriptor.colorAttachments().objectAtIndexedSubscript(i);
            color_desc.setPixelFormats(*format);
            color_desc.setBlendingEnabled(blend_state.is_some());

            if let Some(blend) = blend_state {
                color_desc.setSourceRGBBlendFactor(blend.src_factor);
                color_desc.setDestinationRGBBlendFactor(blend.dst_factor);
                color_desc.setRgbBlendOperation(blend.operation);
            }
        }

        // Vertex descriptor.
        if let Some(vd) = vertex_descriptor {
            descriptor.setVertexDescriptor(&vd.inner);
        }

        descriptor.setSampleCount(sample_count);

        // Depth attachment format.
        if let Some(depth_fmt) = depth_format {
            descriptor.setDepthAttachmentPixelFormats(depth_fmt);
        }

        // Create pipeline state.
        let pipeline_state = unsafe {
            self.device
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
        }
        .map_err(|err| {
            let msg = unsafe { err.localizedDescription() }.to_string();
            RendererError::PipelineCreationFailed(format!("Metal: {}", msg))
        })?;

        // Depth-stencil state (separate in Metal).
        let depth_stencil_state = depth_format.map(|_| {
            self.create_depth_stencil_state(
                true,                         // depth_write
                MTLCompareFunction::GreaterEqual,  // reverse-Z
            )
        });

        Ok(MetalGraphicsPipeline {
            pipeline_state,
            depth_stencil_state,
            cull_mode: MTLCullMode::Back,
            front_face: MTLWinding::CounterClockwise,
            fill_mode: None,
            depth_bias: None,
        })
    }

    fn create_depth_stencil_state(
        &self,
        depth_write: bool,
        compare_func: MTLCompareFunction,
    ) -> Retained<ProtocolObject<dyn MTLDepthStencilState>> {
        let descriptor = unsafe { MTLDepthStencilDescriptor::new() };
        descriptor.setDepthWriteEnabled(depth_write);
        descriptor.setDepthCompareFunction(compare_func);
        unsafe { self.device.newDepthStencilStateWithDescriptor(&descriptor) }
            .expect("Failed to create depth stencil state")
    }
}
```

### Compute Pipeline Creation

```rust
impl MetalContext {
    pub fn create_compute_pipeline(
        &self,
        function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<MetalComputePipeline, RendererError> {
        let pipeline_state = unsafe {
            self.device
                .newComputePipelineStateWithFunction_error(function)
        }
        .map_err(|err| {
            let msg = unsafe { err.localizedDescription() }.to_string();
            RendererError::PipelineCreationFailed(format!("Metal compute: {}", msg))
        })?;

        let workgroup_size = MTLSize::new(
            pipeline_state.threadExecutionWidth(),
            1,
            1,
        );

        Ok(MetalComputePipeline {
            pipeline_state,
            workgroup_size,
        })
    }
}
```

---

## 10. Bindless Textures via Argument Buffers

### Architecture

Metal's argument buffers are the native equivalent of Vulkan's descriptor indexing. They allow packing an array of textures, samplers, and buffers into a single `MTLBuffer` that can be bound in one call.

```
Vulkan:
  DescriptorSet[1] = { binding_array<texture2d, 4096> + sampler }

Metal:
  MTLArgumentEncoder encodes into MTLBuffer:
    [texture0, texture1, ..., texture4095, sampler]
  Bind argument buffer once → shader indexes into array
```

### Implementation

```rust
// metal/argument_buffer.rs

pub struct MetalBindlessTextureManager {
    argument_encoder: Retained<ProtocolObject<dyn MTLArgumentEncoder>>,
    argument_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    textures: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
    capacity: u32,
}

impl MetalBindlessTextureManager {
    pub fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        capacity: u32,
    ) -> Result<Self, RendererError> {
        // Create argument encoder for the bindless layout.
        let encoder_desc = unsafe { MTLArgumentEncoderDescriptor::new() };
        encoder_desc.setLabel(NSString::from_str("BindlessTextures"));

        let argument_encoder = unsafe {
            device.newArgumentEncoderWithDescriptor(&encoder_desc)
        }.ok_or(RendererError::ResourceCreationFailed("Argument encoder".into()))?;

        // Allocate argument buffer.
        let buffer_size = argument_encoder.encodedLength();
        let argument_buffer = device
            .newBufferWithLength_options(
                buffer_size as usize,
                MTLResourceOptions::StorageModeShared,
            )
            .ok_or(RendererError::ResourceCreationFailed("Argument buffer".into()))?;

        Ok(Self {
            argument_encoder,
            argument_buffer,
            textures: vec![None; capacity as usize],
            sampler: Self::create_default_sampler(device),
            capacity,
        })
    }

    /// Register a texture at a given slot index.
    pub fn set_texture(
        &mut self,
        texture: &ProtocolObject<dyn MTLTexture>,
        index: u32,
    ) {
        assert!(index < self.capacity);

        unsafe {
            self.argument_encoder.setArgumentBuffer(
                &self.argument_buffer,
                0,
            );
            self.argument_encoder.setTexture_atIndex(
                texture,
                index as usize,
            );
        }

        self.textures[index as usize] = Some(texture.retain());
    }

    /// Bind the argument buffer to a render encoder.
    pub fn bind(
        &self,
        encoder: &MetalRenderEncoder,
        index: u32,
        stages: ShaderStages,
    ) {
        if stages.contains(ShaderStages::VERTEX) {
            encoder.inner.setVertexBuffer_atIndex(
                0,
                &self.argument_buffer,
                index,
            );
        }
        if stages.contains(ShaderStages::FRAGMENT) {
            encoder.inner.setFragmentBuffer_atIndex(
                0,
                &self.argument_buffer,
                index,
            );
        }
    }
}
```

### MSL Shader Bindless Declaration

```metal
// Generated by naga from WGSL bindless declaration.
// The argument buffer layout must match the encoder.

struct BindlessResources {
    array<texture2d<float, access::sample>, 4096> textures;
    sampler shared_sampler;
};

fragment float4 fs_main(
    VertexOutput in [[stage_in]],
    constant BindlessResources& bindless [[buffer(10)]]
) {
    uint tex_idx = in.texture_index;
    return bindless.textures[tex_idx].sample(bindless.shared_sampler, in.uv);
}
```

### naga MSL Bindless Configuration

```rust
// When configuring naga's MSL output for bindless:
let mut msl_options = msl::Options::default();
msl_options.per_entry_point_map.insert(
    "fs_main".to_string(),
    msl::EntryPointResources {
        // Map bindless array to argument buffer at binding 10.
        push_constant_buffer: None,
        sizes_buffer: None,
    },
);
```

---

## 11. Render Graph Integration

### What Stays Unchanged

- **Compiler** (`compiler.rs`) — pure dependency analysis, no GPU code.
- **Pass descriptors** (`pass.rs`) — abstract pass declarations.
- **Resource declarations** (`resource.rs`) — `GraphResourceDesc`, `ResourceState`.
- **Pass templates** (`passes/`) — `GeometryPass`, `FullscreenPass`, etc.
- **Builder** (`builder.rs`) — `PassBuilder` trait.

### What Gets Generic

The execution layer (`frame_graph.rs`, `transient_texture.rs`, `frame/`) needs to become generic over `B: GpuBackend`.

### TransientTexture (Generic)

```rust
// render_graph/transient_texture.rs

pub struct TransientTexture<B: GpuBackend> {
    pub texture: B::Image,
    pub view: B::ImageView,
    pub format: ImageFormat,
    pub extent: Extent2D,
    pub frames_in_flight: u32,
    pub current_frame: u32,
}

// Vulkan specialization (existing code, unchanged):
pub type VkTransientTexture = TransientTexture<VulkanBackend>;

// Metal specialization (zero new code):
pub type MetalTransientTexture = TransientTexture<MetalBackend>;
```

### Frame Execution (Generic)

```rust
// render_graph/frame/frame_executor.rs

pub struct FrameExecutor<B: GpuBackend> {
    context: B::Context,
    command_buffer: B::CommandBuffer,
    transient_textures: HashMap<ResourceId, TransientTexture<B>>,
}

impl<B: GpuBackend> FrameExecutor<B> {
    pub fn execute_pass(
        &mut self,
        pass: &CompiledPass,
        resources: &ResourceStorage<B>,
    ) {
        let render_pass_desc = self.build_render_pass_descriptor(pass);

        let mut encoder = self.command_buffer.begin_render_pass(&render_pass_desc);

        // Bind pipeline.
        if let Some(pipeline) = pass.graphics_pipeline {
            encoder.bind_graphics_pipeline(&resources.pipelines[pipeline]);
        }

        // Bind descriptor sets (translated to individual bind calls).
        self.bind_resources(&mut encoder, pass, resources);

        // Execute draw calls.
        for draw in &pass.draws {
            encoder.draw_indexed(
                draw.index_count,
                draw.instance_count,
                draw.first_index,
                draw.vertex_offset,
                draw.first_instance,
            );
        }

        encoder.end_encoding();
    }
}
```

---

## 12. Swapchain and Presentation

### MetalSurface

```rust
// metal/surface.rs

use objc2_quartz_core::CAMetalLayer;
use objc2_app_kit::NSView;

pub struct MetalSurface {
    layer: Retained<CAMetalLayer>,
    current_drawable: Option<Retained<CAMetalDrawableProtocol>>,
    size: Size2D,
}

impl MetalSurface {
    pub fn new(
        display: &dyn HasDisplayHandle,
        window: &dyn HasWindowHandle,
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Self, RendererError> {
        // Create CAMetalLayer.
        let layer = unsafe { CAMetalLayer::new() };
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);  // Match current swapchain format
        layer.setMaximumDrawableCount(3);  // Triple buffering
        layer.setDisplaySyncEnabled(true);  // vsync (FIFO equivalent)
        layer.setFramebufferOnly(true);     // Optimize for presentation-only

        // Attach to the native window.
        // On macOS, get the NSView from the raw window handle and set the layer.
        attach_layer_to_window(&layer, window)?;

        Ok(Self {
            layer,
            current_drawable: None,
            size: Size2D::new(0, 0),
        })
    }

    pub fn acquire_next_drawable(
        &mut self,
    ) -> Result<Retained<ProtocolObject<dyn MTLTexture>>, RendererError> {
        let drawable = self.layer
            .nextDrawable()
            .ok_or(RendererError::OutOfDate("No drawable available".into()))?;

        let texture = drawable.texture().retain();
        self.current_drawable = Some(drawable);
        Ok(texture)
    }

    pub fn present(&mut self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
        if let Some(drawable) = self.current_drawable.take() {
            unsafe { command_buffer.presentDrawable(&drawable) };
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Size2D::new(width, height);
        let size = CGSize::new(width as f64, height as f64);
        self.layer.setDrawableSize(size);
    }
}

unsafe fn attach_layer_to_window(
    layer: &CAMetalLayer,
    window: &dyn HasWindowHandle,
) -> Result<(), RendererError> {
    // Get NSView from raw window handle.
    let raw = window.window_handle()
        .map_err(|e| RendererError::InitializationFailed(format!("Window handle: {:?}", e)))?;

    let ns_view: &NSView = unsafe { &*(raw.ptr.ptr as *const NSView) };
    ns_view.setWantsLayer(true);
    ns_view.setLayer(Some(layer));

    Ok(())
}
```

---

## 13. Synchronization Model

### Key Differences from Vulkan

| Concept | Vulkan | Metal |
|---------|--------|-------|
| Image layout transitions | Explicit `vkCmdPipelineBarrier2` | None — implicit between encoders |
| GPU-GPU sync within CB | `vkCmdPipelineBarrier2` | `MTLFence` between encoders |
| GPU-GPU sync across CBs | `vk::Semaphore` | `MTLSharedEvent` signal/wait |
| CPU-GPU sync | `vk::Fence` | `addCompletedHandler:` callback or `waitUntilCompleted` |
| Memory barriers | Explicit `vk::MemoryBarrier2` | Implicit on Apple Silicon (UMA) |

### Simplification on Apple Silicon

On Apple Silicon with unified memory, most Vulkan pipeline barriers are no-ops:
- No discrete VRAM to manage.
- No image layout transitions.
- Command buffers execute in submission order within a queue.

The render graph compiler's barrier generation can be kept for correctness on Intel Macs with discrete GPUs, but becomes largely decorative on Apple Silicon.

### Implementation

```rust
// metal/sync.rs

use dispatch_semaphore_t;

pub struct MetalFence {
    value: AtomicBool,
}

impl MetalFence {
    pub fn new(signaled: bool) -> Self {
        Self { value: AtomicBool::new(signaled) }
    }

    pub fn signal(&self) {
        self.value.store(true, Ordering::Release);
    }

    pub fn wait(&self) {
        while !self.value.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
    }
}

impl GpuFence for MetalFence {
    fn is_signaled(&self) -> bool {
        self.value.load(Ordering::Acquire)
    }
}

pub struct MetalEvent {
    event: Retained<ProtocolObject<dyn MTLSharedEvent>>,
    value: u64,
}

impl MetalEvent {
    pub fn signal(&self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
        unsafe {
            command_buffer.encodeSignalEvent(&self.event, self.value);
        }
    }

    pub fn wait(&self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
        unsafe {
            command_buffer.encodeWaitForEvent(&self.event, self.value - 1);
        }
    }
}

impl GpuEvent for MetalEvent {}
```

### Barrier Translation Strategy

```rust
/// Convert Vulkan-style barrier info to Metal synchronization.
///
/// On Apple Silicon: most barriers are no-ops.
/// On Intel Mac with discrete GPU: use MTLFence between encoders.
pub fn translate_barrier<B: GpuBackend>(barrier: &BarrierInfo<B>, encoder: &mut B::RenderEncoder) {
    // Metal doesn't have image layouts or explicit access/stage masks.
    // Between encoders within a command buffer, ordering is guaranteed.
    // Between command buffers, use MTLSharedEvent.
    //
    // For Katla's render graph, the compiler already inserts barriers
    // at pass boundaries, which map to encoder boundaries in Metal.
    // No explicit barrier calls needed.
}
```

---

## 14. Format and Enum Conversion Tables

### ImageFormat → MTLPixelFormat

```rust
// metal/format.rs

pub fn to_mtl_pixel_format(format: ImageFormat) -> MTLPixelFormat {
    match format {
        // Color formats
        ImageFormat::R8Unorm           => MTLPixelFormat::R8Unorm,
        ImageFormat::R8G8Unorm         => MTLPixelFormat::RG8Unorm,
        ImageFormat::R8G8B8A8Unorm     => MTLPixelFormat::RGBA8Unorm,
        ImageFormat::R8G8B8A8Srgb      => MTLPixelFormat::RGBA8Unorm_sRGB,
        ImageFormat::B8G8R8A8Unorm     => MTLPixelFormat::BGRA8Unorm,
        ImageFormat::B8G8R8A8Srgb      => MTLPixelFormat::BGRA8Unorm_sRGB,
        ImageFormat::R16Float          => MTLPixelFormat::R16Float,
        ImageFormat::R16G16Float       => MTLPixelFormat::RG16Float,
        ImageFormat::R16G16B16A16Float => MTLPixelFormat::RGBA16Float,
        ImageFormat::R32Float          => MTLPixelFormat::R32Float,
        ImageFormat::R32G32Float       => MTLPixelFormat::RG32Float,
        ImageFormat::R32G32B32A32Float => MTLPixelFormat::RGBA32Float,
        ImageFormat::R32Uint           => MTLPixelFormat::R32Uint,

        // Depth/stencil formats
        ImageFormat::D32Float          => MTLPixelFormat::Depth32Float,
        ImageFormat::D32FloatS8Uint    => MTLPixelFormat::Depth32Float_Stencil8,
        ImageFormat::D24UnormS8Uint    => MTLPixelFormat::Depth24Unorm_Stencil8,

        // Compressed formats
        ImageFormat::Bc1RgbUnormBlock  => MTLPixelFormat::BC1_RGBA_Unorm,   // Note: Metal BC1 is RGBA
        ImageFormat::Bc1RgbSrgbBlock   => MTLPixelFormat::BC1_RGBA_Unorm_sRGB,
        ImageFormat::Bc2UnormBlock     => MTLPixelFormat::BC2_RGBA_Unorm,
        ImageFormat::Bc3UnormBlock     => MTLPixelFormat::BC3_RGBA_Unorm,
        ImageFormat::Bc3SrgbBlock      => MTLPixelFormat::BC3_RGBA_Unorm_sRGB,
        ImageFormat::Bc4UnormBlock     => MTLPixelFormat::BC4_RUnorm,
        ImageFormat::Bc5UnormBlock     => MTLPixelFormat::BC5_RGunorm,
        ImageFormat::Bc7UnormBlock     => MTLPixelFormat::BC7_RGBAUnorm,
        ImageFormat::Bc7SrgbBlock      => MTLPixelFormat::BC7_RGBAUnorm_sRGB,

        // ASTC formats (iOS, Apple Silicon)
        ImageFormat::Astc4x4UnormBlock => MTLPixelFormat::ASTC_4x4_LDR,
        ImageFormat::Astc4x4SrgbBlock  => MTLPixelFormat::ASTC_4x4_sRGB,

        _ => MTLPixelFormat::Invalid,
    }
}
```

### CompareOp → MTLCompareFunction

```rust
pub fn to_mtl_compare_func(op: CompareOp) -> MTLCompareFunction {
    match op {
        CompareOp::Never          => MTLCompareFunction::Never,
        CompareOp::Less           => MTLCompareFunction::Less,
        CompareOp::Equal          => MTLCompareFunction::Equal,
        CompareOp::LessOrEqual    => MTLCompareFunction::LessEqual,
        CompareOp::Greater        => MTLCompareFunction::Greater,
        CompareOp::NotEqual       => MTLCompareFunction::NotEqual,
        CompareOp::GreaterOrEqual => MTLCompareFunction::GreaterEqual,
        CompareOp::Always         => MTLCompareFunction::Always,
    }
}
```

### CullMode → MTLCullMode

```rust
pub fn to_mtl_cull_mode(mode: CullMode) -> MTLCullMode {
    match mode {
        CullMode::None  => MTLCullMode::None,
        CullMode::Front => MTLCullMode::Front,
        CullMode::Back  => MTLCullMode::Back,
    }
}
```

### FrontFace → MTLWinding

```rust
pub fn to_mtl_winding(face: FrontFace) -> MTLWinding {
    match face {
        FrontFace::CounterClockwise => MTLWinding::CounterClockwise,
        FrontFace::Clockwise        => MTLWinding::Clockwise,
    }
}
```

### LoadOp/StoreOp → Metal

```rust
pub fn to_mtl_load_action(op: LoadOp) -> MTLLoadAction {
    match op {
        LoadOp::Load     => MTLLoadAction::Load,
        LoadOp::Clear    => MTLLoadAction::Clear,
        LoadOp::DontCare => MTLLoadAction::DontCare,
    }
}

pub fn to_mtl_store_action(op: StoreOp) -> MTLStoreAction {
    match op {
        StoreOp::Store    => MTLStoreAction::Store,
        StoreStore::DontCare => MTLStoreAction::DontCare,
    }
}
```

---

## 15. Platform Considerations

### macOS Version Requirements

| Feature | Minimum macOS |
|---------|---------------|
| Basic Metal | 10.11 |
| Metal 2 (argument buffers tier 1) | 10.13 |
| Metal 3 (argument buffers tier 2, BDA) | 13.0 |
| Metal 4 (mesh shaders, ray tracing) | 26.0 (upcoming) |

**Recommended minimum**: macOS 13.0 (Ventura) for full argument buffer support and `gpuAddress`.

### Apple Silicon vs Intel Mac

| Concern | Apple Silicon (M1+) | Intel Mac (discrete GPU) |
|---------|---------------------|--------------------------|
| Memory | Unified (UMA) | Discrete VRAM |
| Storage mode | `Shared` for everything | `Private` for GPU, `Shared` for CPU |
| Barriers | Mostly no-ops | Need `MTLFence` |
| `didModifyRange` | Not needed | Required for `Managed` storage |
| BDA | Always available | Requires Metal 3 |
| Argument buffer tier | Tier 2 | Tier 1 or 2 |

### Runtime Feature Detection

```rust
impl MetalContext {
    fn detect_features(&self) -> MetalFeatures {
        MetalFeatures {
            is_apple_silicon: self.device.isLowPower(),  // Apple GPUs report low power
            supports_argument_buffers_tier2: unsafe {
                self.device.argumentBuffersSupport() == MTLArgumentBuffersTier::Tier2
            },
            supports_gpu_address: unsafe { available!(macos = 13.0) },
            max_texture_size: self.device.maximum2DTextureWidth() as u32,
            max_bindless_textures: if self.supports_argument_buffers_tier2 {
                4096  // Katla's current max
            } else {
                128   // Tier 1 limit
            },
        }
    }
}
```

---

## 16. Testing Strategy

### Unit Tests (No GPU Required)

- Format conversion tables (`ImageFormat` ↔ `MTLPixelFormat`).
- Enum conversions (`CompareOp`, `CullMode`, `FrontFace`, `LoadOp`, `StoreOp`).
- naga MSL output correctness (compile WGSL, verify MSL string).

### Integration Tests (Requires Metal Device)

```rust
#[cfg(feature = "metal")]
#[test]
fn test_metal_buffer_creation() {
    let context = MetalContext::init_headless(
        ValidationMode::None,
        CString::new("test").unwrap(),
        CString::new("test").unwrap(),
    ).expect("Metal context");

    let buffer = context.create_buffer(256, BufferUsage::STORAGE, MemoryLocation::CpuToGpu)
        .expect("buffer creation");
    assert_eq!(buffer.size(), 256);

    let ptr = buffer.map();
    assert!(!ptr.is_null());
    unsafe { ptr::write_volatile(ptr as *mut u32, 0xDEADBEEF) };
    buffer.unmap();
}

#[cfg(feature = "metal")]
#[test]
fn test_shader_compilation() {
    let context = MetalContext::init_headless(/*...*/);
    let shader = compile_wgsl_to_metal(
        &context.device,
        r#"
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    return vec4f(0.0, 0.0, 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4f {
    return vec4f(1.0, 0.0, 0.0, 1.0);
}
"#,
        &["vs_main", "fs_main"],
    ).expect("shader compilation");

    assert!(shader.module.entry_points.contains_key("vs_main"));
    assert!(shader.module.entry_points.contains_key("fs_main"));
}

#[cfg(feature = "metal")]
#[test]
fn test_bindless_textures() {
    let context = MetalContext::init_headless(/*...*/);
    let manager = MetalBindlessTextureManager::new(&context.device, 4096)
        .expect("bindless manager");

    // Create and register 64 textures.
    for i in 0..64 {
        let (texture, _) = context.create_texture(&TextureDescriptor {
            format: ImageFormat::R8G8B8A8Srgb,
            width: 256,
            height: 256,
            mip_levels: 1,
            usage: TextureUsage::SAMPLED,
            ..Default::default()
        }).expect("texture");
        manager.set_texture(&texture.inner, i);
    }
}
```

### Rendering Test

Use the existing `-s` (single-frame) validation mode:

```bash
# Build with Metal backend
cargo build --features metal
cargo run --features metal -- -s
```

Should produce 25 frames without Metal validation errors.

---

## 17. Migration Checklist

### Phase 1: Backend Abstraction (2-4 weeks) — DONE

- [x] Create `backend/` module with trait definitions
- [x] Make `Renderer` generic over `GpuBackend` (GpuRenderer trait with 38 methods)
- [x] Make `TransientTexture` generic over `GpuBackend` (MetalTransientTexture exists)
- [x] Make render graph execution generic over `GpuBackend` (MetalFrameGraph exists)
- [x] Port existing Vulkan code to implement the traits (VulkanRenderer impl of GpuRenderer)
- [x] Verify Vulkan path still works identically (350 tests pass)
- [ ] Add `cargo test --workspace` to CI

### Phase 2: Metal Backend Core (3-4 weeks) — DONE

- [x] Add `objc2-metal`, `objc2-quartz-core` dependencies
- [x] Implement `MetalContext` (device, queue, surface)
- [x] Implement `MetalBuffer`, `MetalTexture`, `MetalTextureView`
- [x] Implement `MetalSampler`
- [x] Implement shader compilation (naga WGSL → MSL → MTLLibrary)
- [x] Implement `MetalGraphicsPipeline` (including separate depth-stencil)
- [x] Implement `MetalComputePipeline`
- [x] Implement `MetalCommandBuffer` + encoders
- [x] Implement `MetalFence`, `MetalEvent`
- [x] Implement format conversion tables (12 functions, 15 tests)

### Phase 3: Metal Bindless (1-2 weeks) — DONE

- [x] Implement `MetalBindlessTextureManager` with free-list slot allocator
- [x] Configure naga MSL output for argument buffer binding layout (katla_msl_options)
- [ ] Test with full 4096 texture array
- [ ] Verify shader indexing works correctly

### Phase 4: Render Graph Integration (2-3 weeks) — IN PROGRESS

- [x] Implement MetalRenderer with GpuRenderer trait (38 methods, all stubs filled)
- [x] Wire compile_material through WGSL→MSL→Metal pipeline
- [x] Wire execute_draw_calls with storage buffer upload
- [x] Wire render_frame with draw call submission
- [x] Wire begin_frame/end_frame with drawable lifecycle
- [x] Wire create_skeleton/update_skeleton
- [x] Cfg-gate katla_app for multi-backend (Renderer type alias)
- [x] Game binary builds with metal backend
- [ ] Port full frame loop (camera, frustum cull, draw call collection)
- [ ] Port shadow pass, depth prepass, outline pass
- [ ] Port UI rendering pass
- [ ] Port particle system compute passes
- [ ] Port animation compute passes
- [ ] Port light culling compute pass
- [ ] Test with `-s` single-frame validation mode

### Phase 5: Polish and CI (1-2 weeks)

- [ ] Metal validation layer testing
- [ ] Performance benchmarks vs MoltenVK
- [ ] Update `game/build.rs` for Metal backend (remove Vulkan SDK requirement)
- [ ] Update CI to test both backends
- [ ] Update AGENTS.md with Metal backend documentation
