#![allow(unused_imports)]

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandQueue, MTLCompareFunction, MTLComputePipelineState,
    MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor, MTLDepthStencilState, MTLDevice,
    MTLFunction, MTLGPUFamily, MTLPixelFormat, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLResourceOptions, MTLStencilDescriptor, MTLStencilOperation, MTLStorageMode,
    MTLTextureDescriptor, MTLVertexDescriptor, MTLVertexFormat, MTLVertexStepFunction,
};

use crate::backend::traits::{GpuBackend, GpuContext};
use crate::error::RendererError;
use crate::pipeline::CompareOp;
use crate::texture::TextureDescriptor;

/// Stencil face operations for depth/stencil state creation.
pub(crate) struct StencilFaceOps {
    pub compare_func: MTLCompareFunction,
    pub stencil_fail_op: MTLStencilOperation,
    pub depth_fail_op: MTLStencilOperation,
    pub depth_stencil_pass_op: MTLStencilOperation,
    pub read_mask: u32,
    pub write_mask: u32,
}

impl Default for StencilFaceOps {
    fn default() -> Self {
        Self {
            compare_func: MTLCompareFunction::Always,
            stencil_fail_op: MTLStencilOperation::Keep,
            depth_fail_op: MTLStencilOperation::Keep,
            depth_stencil_pass_op: MTLStencilOperation::Keep,
            read_mask: 0xFF,
            write_mask: 0xFF,
        }
    }
}

use super::buffer::MetalBuffer;
use super::command_buffer::MetalCommandBuffer;
use super::format::{to_mtl_compare_func, to_mtl_pixel_format, to_mtl_texture_usage};
use super::pipeline::{MetalComputePipeline, MetalGraphicsPipeline};
use super::sampler::MetalSamplerState;
use super::surface::MetalSurface;
use super::sync::{MetalEvent, MetalFence};
use super::texture::{MetalTexture, MetalTextureView};

/// Build the standard PBR vertex descriptor matching `VertexPBR`.
///
/// Layout (48 bytes stride, interleaved in buffer 0):
/// - location 0: position Float3 @ offset 0
/// - location 1: normal Float3 @ offset 12
/// - location 2: tangent Float4 @ offset 24
/// - location 3: uv Float2 @ offset 40
pub(crate) fn default_pbr_vertex_descriptor() -> Retained<MTLVertexDescriptor> {
    let vertex_descriptor = MTLVertexDescriptor::new();

    let layouts = vertex_descriptor.layouts();
    let layout = unsafe { layouts.objectAtIndexedSubscript(10) };
    unsafe {
        layout.setStride(48);
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    let attrs = vertex_descriptor.attributes();

    let pos_attr = unsafe { attrs.objectAtIndexedSubscript(0) };
    pos_attr.setFormat(MTLVertexFormat::Float3);
    unsafe {
        pos_attr.setOffset(0);
        pos_attr.setBufferIndex(10);
    }

    let norm_attr = unsafe { attrs.objectAtIndexedSubscript(1) };
    norm_attr.setFormat(MTLVertexFormat::Float3);
    unsafe {
        norm_attr.setOffset(12);
        norm_attr.setBufferIndex(10);
    }

    let tan_attr = unsafe { attrs.objectAtIndexedSubscript(2) };
    tan_attr.setFormat(MTLVertexFormat::Float4);
    unsafe {
        tan_attr.setOffset(24);
        tan_attr.setBufferIndex(10);
    }

    let uv_attr = unsafe { attrs.objectAtIndexedSubscript(3) };
    uv_attr.setFormat(MTLVertexFormat::Float2);
    unsafe {
        uv_attr.setOffset(40);
        uv_attr.setBufferIndex(10);
    }

    vertex_descriptor
}

pub(crate) struct MetalFeatures {
    pub(crate) max_bindless_textures: u32,
}

pub(crate) struct MetalBackend;

impl GpuBackend for MetalBackend {
    type Context = MetalContext;
    type CommandBuffer = MetalCommandBuffer;
    type RenderEncoder = super::render_encoder::MetalRenderEncoder;
    type ComputeEncoder = super::compute_encoder::MetalComputeEncoder;
    type BlitEncoder = super::blit_encoder::MetalBlitEncoder;
    type Image = MetalTexture;
    type ImageView = MetalTextureView;
    type Buffer = MetalBuffer;
    type GraphicsPipeline = MetalGraphicsPipeline;
    type ComputePipeline = MetalComputePipeline;
    type Sampler = MetalSamplerState;
    type Fence = MetalFence;
    type Event = MetalEvent;

    fn name() -> &'static str {
        "Metal"
    }
}

pub(crate) struct MetalContext {
    pub(crate) device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub(crate) command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pub(crate) surface: MetalSurface,
}

impl GpuContext<MetalBackend> for MetalContext {}

impl MetalContext {
    pub(crate) fn init(
        window: &dyn raw_window_handle::HasWindowHandle,
        display: &dyn raw_window_handle::HasDisplayHandle,
    ) -> Result<Self, RendererError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| RendererError::InitializationFailed("No Metal device found".into()))?;
        let command_queue = device.newCommandQueue().ok_or_else(|| {
            RendererError::InitializationFailed("Failed to create command queue".into())
        })?;
        let surface = MetalSurface::new(window, display, &device)?;
        Ok(Self {
            device,
            command_queue,
            surface,
        })
    }

    #[cfg(test)]
    pub(crate) fn init_headless() -> Result<Self, RendererError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| RendererError::InitializationFailed("No Metal device found".into()))?;
        let command_queue = device.newCommandQueue().ok_or_else(|| {
            RendererError::InitializationFailed("Failed to create command queue".into())
        })?;
        Ok(Self {
            device,
            command_queue,
            surface: MetalSurface::headless(),
        })
    }

    pub(crate) fn create_buffer(
        &self,
        size: u64,
        cpu_accessible: bool,
    ) -> Result<MetalBuffer, RendererError> {
        let options = if cpu_accessible {
            MTLResourceOptions::StorageModeShared
        } else {
            MTLResourceOptions::StorageModePrivate
        };
        let buffer = self
            .device
            .newBufferWithLength_options(size as usize, options)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Failed to create Metal buffer".into())
            })?;
        Ok(MetalBuffer::new(buffer, size))
    }

    pub(crate) fn create_texture(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(MetalTexture, MetalTextureView), RendererError> {
        let tex_desc = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                to_mtl_pixel_format(descriptor.format),
                descriptor.width as usize,
                descriptor.height as usize,
                false,
            )
        };
        tex_desc.setUsage(to_mtl_texture_usage(descriptor.usage));
        tex_desc.setStorageMode(MTLStorageMode::Private);
        let texture = self
            .device
            .newTextureWithDescriptor(&tex_desc)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Failed to create Metal texture".into())
            })?;
        let metal_texture = MetalTexture::new(texture.clone(), descriptor.format);
        let view = MetalTextureView::new(texture, metal_texture.clone());
        Ok((metal_texture, view))
    }

    pub(crate) fn create_texture_with_data(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(MetalTexture, MetalTextureView), RendererError> {
        let tex_desc = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                to_mtl_pixel_format(descriptor.format),
                descriptor.width as usize,
                descriptor.height as usize,
                false,
            )
        };
        tex_desc.setUsage(to_mtl_texture_usage(descriptor.usage));
        tex_desc.setStorageMode(MTLStorageMode::Shared);
        let texture = self
            .device
            .newTextureWithDescriptor(&tex_desc)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Failed to create Metal texture".into())
            })?;
        let metal_texture = MetalTexture::new(texture.clone(), descriptor.format);
        let view = MetalTextureView::new(texture, metal_texture.clone());
        Ok((metal_texture, view))
    }

    pub(crate) fn create_sampler(&self) -> Result<MetalSamplerState, RendererError> {
        let desc = objc2_metal::MTLSamplerDescriptor::new();
        desc.setMinFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
        desc.setMagFilter(objc2_metal::MTLSamplerMinMagFilter::Linear);
        desc.setMipFilter(objc2_metal::MTLSamplerMipFilter::Linear);
        desc.setSAddressMode(objc2_metal::MTLSamplerAddressMode::Repeat);
        desc.setTAddressMode(objc2_metal::MTLSamplerAddressMode::Repeat);
        let sampler = self
            .device
            .newSamplerStateWithDescriptor(&desc)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Failed to create Metal sampler".into())
            })?;
        Ok(MetalSamplerState { inner: sampler })
    }

    pub(crate) fn create_command_buffer(&self) -> MetalCommandBuffer {
        let cmd_buffer = self
            .command_queue
            .commandBuffer()
            .expect("Failed to allocate command buffer");
        MetalCommandBuffer { inner: cmd_buffer }
    }

    pub(crate) fn create_graphics_pipeline(
        &self,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
        color_formats: &[MTLPixelFormat],
        depth_format: Option<MTLPixelFormat>,
        depth_write_enabled: bool,
        depth_compare: CompareOp,
        cull_mode: objc2_metal::MTLCullMode,
        front_face: objc2_metal::MTLWinding,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        self.create_graphics_pipeline_with_vertex_descriptor(
            vertex_function,
            fragment_function,
            color_formats,
            depth_format,
            depth_write_enabled,
            depth_compare,
            cull_mode,
            front_face,
            None,
        )
    }

    pub(crate) fn create_graphics_pipeline_with_vertex_descriptor(
        &self,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
        color_formats: &[MTLPixelFormat],
        depth_format: Option<MTLPixelFormat>,
        depth_write_enabled: bool,
        depth_compare: CompareOp,
        cull_mode: objc2_metal::MTLCullMode,
        front_face: objc2_metal::MTLWinding,
        vertex_descriptor: Option<&MTLVertexDescriptor>,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(vertex_function));
        descriptor.setFragmentFunction(fragment_function);
        descriptor.setRasterSampleCount(1);

        let color_attachments = descriptor.colorAttachments();
        for (i, &format) in color_formats.iter().enumerate() {
            let attachment = unsafe { color_attachments.objectAtIndexedSubscript(i as usize) };
            attachment.setPixelFormat(format);
        }

        if let Some(depth_fmt) = depth_format {
            descriptor.setDepthAttachmentPixelFormat(depth_fmt);
        }

        let vd = match vertex_descriptor {
            Some(vd) => vd.retain(),
            None => default_pbr_vertex_descriptor(),
        };
        descriptor.setVertexDescriptor(Some(&vd));

        let pipeline_state = self
            .device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|err| {
                let msg = err.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create graphics pipeline: {}",
                    msg
                ))
            })?;

        let depth_stencil_state = if depth_format.is_some() {
            Some(self.create_depth_stencil_state(
                depth_write_enabled,
                to_mtl_compare_func(depth_compare),
            ))
        } else {
            None
        };

        Ok(MetalGraphicsPipeline {
            pipeline_state,
            depth_stencil_state,
            cull_mode,
            front_face,
            depth_bias: None,
        })
    }

    fn create_depth_stencil_state(
        &self,
        depth_write_enabled: bool,
        compare_func: MTLCompareFunction,
    ) -> Retained<ProtocolObject<dyn MTLDepthStencilState>> {
        let descriptor = MTLDepthStencilDescriptor::new();
        descriptor.setDepthWriteEnabled(depth_write_enabled);
        descriptor.setDepthCompareFunction(compare_func);
        self.device
            .newDepthStencilStateWithDescriptor(&descriptor)
            .expect("Failed to create depth-stencil state")
    }

    pub(crate) fn create_depth_stencil_state_with_stencil(
        &self,
        depth_write_enabled: bool,
        depth_compare: MTLCompareFunction,
        stencil_face: StencilFaceOps,
    ) -> Retained<ProtocolObject<dyn MTLDepthStencilState>> {
        let descriptor = MTLDepthStencilDescriptor::new();
        descriptor.setDepthWriteEnabled(depth_write_enabled);
        descriptor.setDepthCompareFunction(depth_compare);

        let stencil_desc = MTLStencilDescriptor::new();
        stencil_desc.setStencilCompareFunction(stencil_face.compare_func);
        stencil_desc.setStencilFailureOperation(stencil_face.stencil_fail_op);
        stencil_desc.setDepthFailureOperation(stencil_face.depth_fail_op);
        stencil_desc.setDepthStencilPassOperation(stencil_face.depth_stencil_pass_op);
        stencil_desc.setReadMask(stencil_face.read_mask);
        stencil_desc.setWriteMask(stencil_face.write_mask);

        descriptor.setFrontFaceStencil(Some(&stencil_desc));
        descriptor.setBackFaceStencil(Some(&stencil_desc));

        self.device
            .newDepthStencilStateWithDescriptor(&descriptor)
            .expect("Failed to create depth-stencil state with stencil")
    }

    pub(crate) fn create_graphics_pipeline_with_stencil(
        &self,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
        color_formats: &[MTLPixelFormat],
        depth_format: Option<MTLPixelFormat>,
        depth_write_enabled: bool,
        depth_compare: CompareOp,
        cull_mode: objc2_metal::MTLCullMode,
        front_face: objc2_metal::MTLWinding,
        stencil_face: StencilFaceOps,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        self.create_graphics_pipeline_with_stencil_and_vertex_descriptor(
            vertex_function,
            fragment_function,
            color_formats,
            depth_format,
            depth_write_enabled,
            depth_compare,
            cull_mode,
            front_face,
            stencil_face,
            None,
        )
    }

    pub(crate) fn create_graphics_pipeline_with_stencil_and_vertex_descriptor(
        &self,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
        fragment_function: Option<&ProtocolObject<dyn MTLFunction>>,
        color_formats: &[MTLPixelFormat],
        depth_format: Option<MTLPixelFormat>,
        depth_write_enabled: bool,
        depth_compare: CompareOp,
        cull_mode: objc2_metal::MTLCullMode,
        front_face: objc2_metal::MTLWinding,
        stencil_face: StencilFaceOps,
        vertex_descriptor: Option<&MTLVertexDescriptor>,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(vertex_function));
        descriptor.setFragmentFunction(fragment_function);
        descriptor.setRasterSampleCount(1);

        let color_attachments = descriptor.colorAttachments();
        for (i, &format) in color_formats.iter().enumerate() {
            let attachment = unsafe { color_attachments.objectAtIndexedSubscript(i as usize) };
            attachment.setPixelFormat(format);
        }

        if let Some(depth_fmt) = depth_format {
            descriptor.setDepthAttachmentPixelFormat(depth_fmt);
        }

        let vd = match vertex_descriptor {
            Some(vd) => vd.retain(),
            None => default_pbr_vertex_descriptor(),
        };
        descriptor.setVertexDescriptor(Some(&vd));

        let pipeline_state = self
            .device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|err| {
                let msg = err.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create graphics pipeline: {}",
                    msg
                ))
            })?;

        let depth_stencil_state = if depth_format.is_some() {
            Some(self.create_depth_stencil_state_with_stencil(
                depth_write_enabled,
                to_mtl_compare_func(depth_compare),
                stencil_face,
            ))
        } else {
            None
        };

        Ok(MetalGraphicsPipeline {
            pipeline_state,
            depth_stencil_state,
            cull_mode,
            front_face,
            depth_bias: None,
        })
    }

    pub(crate) fn create_compute_pipeline(
        &self,
        function: &ProtocolObject<dyn MTLFunction>,
        workgroup: [u32; 3],
    ) -> Result<MetalComputePipeline, RendererError> {
        let pipeline_state = self
            .device
            .newComputePipelineStateWithFunction_error(function)
            .map_err(|err| {
                let msg = err.localizedDescription().to_string();
                RendererError::ResourceCreationFailed(format!(
                    "Failed to create compute pipeline: {}",
                    msg
                ))
            })?;
        Ok(MetalComputePipeline {
            pipeline_state,
            workgroup,
        })
    }

    pub(crate) fn detect_features(&self) -> MetalFeatures {
        let is_apple_silicon = self.device.supportsFamily(MTLGPUFamily::Apple7);

        let max_bindless_textures: u32 = if is_apple_silicon { 4096 } else { 2048 };

        MetalFeatures {
            max_bindless_textures,
        }
    }
}

unsafe impl Send for MetalContext {}
unsafe impl Sync for MetalContext {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::GpuCommandBuffer;
    use crate::backend::command::GpuRenderEncoder;
    use crate::backend::resource::GpuBuffer;
    use crate::backend::resource::GpuComputePipeline;
    use crate::backend::resource::GpuImage;
    use crate::metal::shader;
    use crate::texture::TextureUsage;

    #[test]
    fn test_metal_context_headless() {
        let ctx = MetalContext::init_headless();
        assert!(
            ctx.is_ok(),
            "Failed to create headless Metal context: {:?}",
            ctx.err()
        );
    }

    #[test]
    fn test_metal_buffer_creation_cpu_accessible() {
        let ctx = MetalContext::init_headless().unwrap();
        let buffer = ctx.create_buffer(256, true);
        assert!(
            buffer.is_ok(),
            "Failed to create CPU-accessible buffer: {:?}",
            buffer.err()
        );
        assert_eq!(buffer.unwrap().size(), 256);
    }

    #[test]
    fn test_metal_buffer_creation_gpu_only() {
        let ctx = MetalContext::init_headless().unwrap();
        let buffer = ctx.create_buffer(1024, false);
        assert!(
            buffer.is_ok(),
            "Failed to create GPU-only buffer: {:?}",
            buffer.err()
        );
        assert_eq!(buffer.unwrap().size(), 1024);
    }

    #[test]
    fn test_metal_buffer_creation_large() {
        let ctx = MetalContext::init_headless().unwrap();
        let buffer = ctx.create_buffer(16 * 1024 * 1024, true);
        assert!(
            buffer.is_ok(),
            "Failed to create large buffer: {:?}",
            buffer.err()
        );
    }

    #[test]
    fn test_metal_texture_creation_rgba8_srgb() {
        let ctx = MetalContext::init_headless().unwrap();
        let desc = TextureDescriptor::new(256, 256, crate::texture::ImageFormat::R8G8B8A8Srgb);
        let result = ctx.create_texture(&desc);
        assert!(
            result.is_ok(),
            "Failed to create RGBA8 SRGB texture: {:?}",
            result.err()
        );
        let (texture, _view) = result.unwrap();
        assert_eq!(texture.width(), 256);
        assert_eq!(texture.height(), 256);
        assert_eq!(texture.format(), crate::texture::ImageFormat::R8G8B8A8Srgb);
    }

    #[test]
    fn test_metal_texture_creation_depth() {
        let ctx = MetalContext::init_headless().unwrap();
        let desc = TextureDescriptor::new(256, 256, crate::texture::ImageFormat::D32Sfloat)
            .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
        let result = ctx.create_texture(&desc);
        assert!(
            result.is_ok(),
            "Failed to create depth texture: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_metal_texture_creation_rgba16_float() {
        let ctx = MetalContext::init_headless().unwrap();
        let desc =
            TextureDescriptor::new(128, 128, crate::texture::ImageFormat::R16G16B16A16Sfloat);
        let result = ctx.create_texture(&desc);
        assert!(
            result.is_ok(),
            "Failed to create RGBA16 float texture: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_metal_command_buffer_creation() {
        let ctx = MetalContext::init_headless().unwrap();
        let _cmd_buffer = ctx.create_command_buffer();
    }

    #[test]
    fn test_metal_sampler_creation() {
        let ctx = MetalContext::init_headless().unwrap();
        let sampler = ctx.create_sampler();
        assert!(
            sampler.is_ok(),
            "Failed to create sampler: {:?}",
            sampler.err()
        );
    }

    #[test]
    fn test_metal_graphics_pipeline_creation() {
        let ctx = MetalContext::init_headless().unwrap();
        let shader = shader::compile_wgsl_to_metal(
            &ctx.device,
            r#"
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    return vec4f(0.0, 0.0, 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4f {
    return vec4f(1.0, 0.0, 0.0, 1.0);
}
"#,
            &["vs_main", "fs_main"],
        )
        .unwrap();

        let vs = shader.module.entry_points.get("vs_main").unwrap();
        let fs = shader.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx.create_graphics_pipeline(
            vs,
            Some(fs),
            &[MTLPixelFormat::BGRA8Unorm_sRGB],
            Some(MTLPixelFormat::Depth32Float),
            true,
            CompareOp::LessOrEqual,
            objc2_metal::MTLCullMode::Back,
            objc2_metal::MTLWinding::CounterClockwise,
        );
        assert!(
            pipeline.is_ok(),
            "Failed to create graphics pipeline: {:?}",
            pipeline.err()
        );
    }

    #[test]
    fn test_metal_compute_pipeline_creation() {
        let ctx = MetalContext::init_headless().unwrap();
        let shader = shader::compile_wgsl_to_metal(
            &ctx.device,
            r#"
@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {}
"#,
            &["cs_main"],
        )
        .unwrap();

        let cs = shader.module.entry_points.get("cs_main").unwrap();
        let pipeline = ctx.create_compute_pipeline(cs, [64, 1, 1]);
        assert!(
            pipeline.is_ok(),
            "Failed to create compute pipeline: {:?}",
            pipeline.err()
        );
        assert_eq!(pipeline.unwrap().workgroup_size()[0], 64);
    }

    #[test]
    fn test_metal_feature_detection() {
        let ctx = MetalContext::init_headless().unwrap();
        let features = ctx.detect_features();
        assert!(features.max_bindless_textures > 0);
    }

    #[test]
    fn test_metal_buffer_write_read() {
        let ctx = MetalContext::init_headless().unwrap();

        let buffer = ctx.create_buffer(256, true).unwrap();
        assert_eq!(buffer.size(), 256);

        let ptr = buffer.map();
        assert!(!ptr.is_null());

        let data = ptr as *mut [u32; 64];
        unsafe {
            for i in 0..64 {
                (*data)[i] = i as u32;
            }
        }
        buffer.unmap();

        let ptr = buffer.map();
        let data = ptr as *const [u32; 64];
        unsafe {
            for i in 0..64 {
                assert_eq!((*data)[i], i as u32, "Mismatch at index {}", i);
            }
        }
        buffer.unmap();
    }

    #[test]
    fn test_metal_buffer_gpu_address() {
        let ctx = MetalContext::init_headless().unwrap();
        let buffer = ctx.create_buffer(256, false).unwrap();
        let addr = buffer.gpu_address();
        println!("GPU address: {:#x}", addr);
    }

    #[test]
    fn test_metal_full_rendering_smoke() {
        let ctx = MetalContext::init_headless().unwrap();

        let shader = shader::compile_wgsl_to_metal(
            &ctx.device,
            r#"
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
}

@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(1.0, -1.0),
        vec2f(0.0, 1.0),
    );
    var colors = array<vec4f, 3>(
        vec4f(1.0, 0.0, 0.0, 1.0),
        vec4f(0.0, 1.0, 0.0, 1.0),
        vec4f(0.0, 0.0, 1.0, 1.0),
    );
    var output: VertexOutput;
    output.position = vec4f(positions[vi], 0.0, 1.0);
    output.color = colors[vi];
    return output;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
"#,
            &["vs_main", "fs_main"],
        )
        .unwrap();

        let vs = shader.module.entry_points.get("vs_main").unwrap();
        let fs = shader.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline(
                vs,
                Some(fs),
                &[MTLPixelFormat::BGRA8Unorm_sRGB],
                None,
                false,
                CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::CounterClockwise,
            )
            .unwrap();

        let desc = TextureDescriptor::new(256, 256, crate::texture::ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT);
        let (_texture, view) = ctx.create_texture(&desc).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = crate::backend::command::RenderPassInfo {
            color_attachments: vec![crate::backend::command::ColorAttachmentInfo {
                view,
                load_op: crate::render_pass::LoadOp::Clear,
                store_op: crate::render_pass::StoreOp::Store,
                clear_value: crate::render_pass::ClearValue::color(0.1, 0.1, 0.1, 1.0),
            }],
            depth_attachment: None,
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.bind_graphics_pipeline(&pipeline);
        encoder.draw(3, 1, 0, 0);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();
    }
}
