use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTL4CommandAllocator, MTL4CommandAllocatorDescriptor, MTL4CommandQueue,
    MTL4CommandQueueDescriptor, MTLCompareFunction, MTLCreateSystemDefaultDevice,
    MTLDepthStencilDescriptor, MTLDepthStencilState, MTLDevice, MTLFunction, MTLGPUFamily,
    MTLPixelFormat, MTLRenderPipelineDescriptor, MTLResourceOptions, MTLStencilDescriptor,
    MTLStorageMode, MTLTextureDescriptor, MTLVertexDescriptor, MTLVertexFormat,
    MTLVertexStepFunction,
};

use crate::backend::traits::{GpuBackend, GpuContext};
use crate::error::RendererError;
use crate::pipeline::CompareOp;
use crate::texture::TextureDescriptor;

use super::buffer::MetalBuffer;
use super::command_buffer::MetalCommandBuffer;
use super::format::{to_mtl_compare_func, to_mtl_pixel_format, to_mtl_texture_usage};
use super::pipeline::{MetalComputePipeline, MetalGraphicsPipeline};
use super::sampler::MetalSamplerState;
use super::surface::MetalSurface;
use super::texture::{MetalTexture, MetalTextureView};

/// Build the instanced UI vertex descriptor for unit quad input.
///
/// Layout (8 bytes stride in buffer 10, PerVertex):
/// - location 0: local_pos Float2 @ offset 0
///
/// Instance data is read from a storage buffer (not vertex attributes).
pub(crate) fn ui_instanced_vertex_descriptor() -> Retained<MTLVertexDescriptor> {
    let vertex_descriptor = MTLVertexDescriptor::new();

    let layouts = vertex_descriptor.layouts();
    let layout = unsafe { layouts.objectAtIndexedSubscript(10) };
    unsafe {
        layout.setStride(8);
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    let attrs = vertex_descriptor.attributes();

    let pos_attr = unsafe { attrs.objectAtIndexedSubscript(0) };
    pos_attr.setFormat(MTLVertexFormat::Float2);
    unsafe {
        pos_attr.setOffset(0);
        pos_attr.setBufferIndex(10);
    }

    vertex_descriptor
}

pub(crate) struct GraphicsPipelineConfig<'a> {
    pub(crate) vertex_function: &'a ProtocolObject<dyn MTLFunction>,
    pub(crate) fragment_function: Option<&'a ProtocolObject<dyn MTLFunction>>,
    pub(crate) color_formats: &'a [MTLPixelFormat],
    pub(crate) depth_format: Option<MTLPixelFormat>,
    pub(crate) depth_write_enabled: bool,
    pub(crate) depth_compare: CompareOp,
    pub(crate) cull_mode: objc2_metal::MTLCullMode,
    pub(crate) front_face: objc2_metal::MTLWinding,
    pub(crate) vertex_descriptor: &'a MTLVertexDescriptor,
    pub(crate) alpha_blended: bool,
    pub(crate) portable: Option<&'a crate::renderer::pipeline_descriptor::PipelineDescriptor>,
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
}

pub(crate) struct MetalContext {
    pub(crate) device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub(crate) command_queue: Retained<ProtocolObject<dyn MTL4CommandQueue>>,
    pub(crate) surface: MetalSurface,
    pub(crate) pipeline_archive:
        Option<std::sync::Arc<super::pipeline_archive::MetalPipelineArchive>>,
}

impl MetalContext {
    fn open_pipeline_archive(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Option<std::sync::Arc<super::pipeline_archive::MetalPipelineArchive>> {
        match super::pipeline_archive::MetalPipelineArchive::open_or_create(device) {
            Ok(archive) => {
                let stats = archive.stats();
                log::info!(
                    "Pipeline cache ready: opened_from_disk={}, rejection={:?}, open_ms={}",
                    stats.opened_from_disk,
                    stats.rejection,
                    stats.open_duration.as_millis(),
                );
                Some(std::sync::Arc::new(archive))
            }
            Err(err) => {
                log::warn!("Pipeline cache disabled: {err}");
                None
            }
        }
    }
}

impl GpuContext<MetalBackend> for MetalContext {}

impl MetalContext {
    pub(crate) fn init(
        window: &dyn raw_window_handle::HasWindowHandle,
        display: &dyn raw_window_handle::HasDisplayHandle,
    ) -> Result<Self, RendererError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| RendererError::InitializationFailed("No Metal device found".into()))?;
        let command_queue = Self::create_command_queue(&device)?;
        let surface = MetalSurface::new(window, display, &device)?;
        let pipeline_archive = Self::open_pipeline_archive(&device);
        Ok(Self {
            device,
            command_queue,
            surface,
            pipeline_archive,
        })
    }

    #[cfg(test)]
    pub(crate) fn init_headless() -> Result<Self, RendererError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| RendererError::InitializationFailed("No Metal device found".into()))?;
        let command_queue = Self::create_command_queue(&device)?;
        let pipeline_archive = Self::open_pipeline_archive(&device);
        Ok(Self {
            device,
            command_queue,
            surface: MetalSurface::headless(),
            pipeline_archive,
        })
    }

    pub(crate) fn init_headless_with_size(width: u32, height: u32) -> Result<Self, RendererError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| RendererError::InitializationFailed("No Metal device found".into()))?;
        let command_queue = Self::create_command_queue(&device)?;
        let surface = MetalSurface::headless_with_device(&device, width, height);
        let pipeline_archive = Self::open_pipeline_archive(&device);
        Ok(Self {
            device,
            command_queue,
            surface,
            pipeline_archive,
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

    /// Shared-storage texture for the documented CPU-readback contract
    /// (headless screenshot and offscreen readback paths).
    pub(crate) fn create_texture_shared(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(MetalTexture, MetalTextureView), RendererError> {
        descriptor.validate_data(0)?;
        let tex_desc = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                to_mtl_pixel_format(descriptor.format),
                descriptor.width as usize,
                descriptor.height as usize,
                false,
            )
        };
        unsafe {
            tex_desc.setMipmapLevelCount(descriptor.mip_levels as usize);
            tex_desc.setDepth(descriptor.depth as usize);
            tex_desc.setArrayLength(descriptor.array_layers as usize);
        }
        tex_desc.setTextureType(if descriptor.depth > 1 {
            objc2_metal::MTLTextureType::Type3D
        } else if descriptor.array_layers > 1 {
            objc2_metal::MTLTextureType::Type2DArray
        } else {
            objc2_metal::MTLTextureType::Type2D
        });
        if descriptor.format.block_extent() != [1, 1] && !self.device.supportsBCTextureCompression()
        {
            return Err(RendererError::UnsupportedFeature(format!(
                "{:?} block compression is unsupported by this Metal device",
                descriptor.format
            )));
        }
        tex_desc.setUsage(to_mtl_texture_usage(descriptor.usage));
        tex_desc.setStorageMode(MTLStorageMode::Shared);
        let texture = self
            .device
            .newTextureWithDescriptor(&tex_desc)
            .ok_or_else(|| RendererError::AllocationFailed {
                resource: "metal texture".to_string(),
                reason: format!(
                    "{}x{}x{} {:?}, layers {}, mips {}: device refused the texture descriptor",
                    descriptor.width,
                    descriptor.height,
                    descriptor.depth,
                    descriptor.format,
                    descriptor.array_layers,
                    descriptor.mip_levels
                ),
            })?;
        let metal_texture = MetalTexture::new(texture.clone(), descriptor.format)
            .with_upload_policy(descriptor.generate_mips, descriptor.label);
        let view = MetalTextureView::new(texture, metal_texture.clone());
        Ok((metal_texture, view))
    }

    pub(crate) fn create_texture(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<(MetalTexture, MetalTextureView), RendererError> {
        descriptor.validate_data(0)?;
        let tex_desc = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                to_mtl_pixel_format(descriptor.format),
                descriptor.width as usize,
                descriptor.height as usize,
                false,
            )
        };
        unsafe {
            tex_desc.setMipmapLevelCount(descriptor.mip_levels as usize);
            tex_desc.setDepth(descriptor.depth as usize);
            tex_desc.setArrayLength(descriptor.array_layers as usize);
        }
        tex_desc.setTextureType(if descriptor.depth > 1 {
            objc2_metal::MTLTextureType::Type3D
        } else if descriptor.array_layers > 1 {
            objc2_metal::MTLTextureType::Type2DArray
        } else {
            objc2_metal::MTLTextureType::Type2D
        });
        if descriptor.format.block_extent() != [1, 1] && !self.device.supportsBCTextureCompression()
        {
            return Err(RendererError::UnsupportedFeature(format!(
                "{:?} block compression is unsupported by this Metal device",
                descriptor.format
            )));
        }
        tex_desc.setUsage(to_mtl_texture_usage(descriptor.usage));
        tex_desc.setStorageMode(MTLStorageMode::Private);

        let texture = self
            .device
            .newTextureWithDescriptor(&tex_desc)
            .ok_or_else(|| RendererError::AllocationFailed {
                resource: "metal texture".to_string(),
                reason: format!(
                    "{}x{}x{} {:?}, layers {}, mips {}: device refused the texture descriptor",
                    descriptor.width,
                    descriptor.height,
                    descriptor.depth,
                    descriptor.format,
                    descriptor.array_layers,
                    descriptor.mip_levels
                ),
            })?;
        let metal_texture = MetalTexture::new(texture.clone(), descriptor.format)
            .with_upload_policy(descriptor.generate_mips, descriptor.label);
        let view = MetalTextureView::new(texture, metal_texture.clone());
        Ok((metal_texture, view))
    }

    pub(crate) fn create_sampler(&self) -> Result<MetalSamplerState, RendererError> {
        let desc = objc2_metal::MTLSamplerDescriptor::new();
        desc.setSupportArgumentBuffers(true);
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

    pub(crate) fn create_sampler_with_descriptor(
        &self,
        desc: &objc2_metal::MTLSamplerDescriptor,
    ) -> Result<MetalSamplerState, RendererError> {
        desc.setSupportArgumentBuffers(true);
        let sampler = self
            .device
            .newSamplerStateWithDescriptor(desc)
            .ok_or_else(|| {
                RendererError::InvalidOperation("Failed to create Metal sampler".into())
            })?;
        Ok(MetalSamplerState { inner: sampler })
    }

    fn create_command_queue(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Retained<ProtocolObject<dyn MTL4CommandQueue>>, RendererError> {
        if !device.supportsFamily(MTLGPUFamily::Metal4) {
            return Err(RendererError::UnsupportedFeature(format!(
                "Metal 4 is required; GPU '{}' does not support MTLGPUFamilyMetal4",
                device.name(),
            )));
        }
        let descriptor = MTL4CommandQueueDescriptor::new();
        descriptor.setLabel(Some(&objc2_foundation::NSString::from_str(
            "Katla Metal4 graphics queue",
        )));
        device
            .newMTL4CommandQueueWithDescriptor_error(&descriptor)
            .map_err(|error| {
                RendererError::InitializationFailed(error.localizedDescription().to_string())
            })
    }

    pub(crate) fn create_command_allocator(
        &self,
        slot: usize,
    ) -> Result<Retained<ProtocolObject<dyn MTL4CommandAllocator>>, RendererError> {
        let descriptor = MTL4CommandAllocatorDescriptor::new();
        descriptor.setLabel(Some(&objc2_foundation::NSString::from_str(&format!(
            "frame_slot.{slot}.allocator"
        ))));
        self.device
            .newCommandAllocatorWithDescriptor_error(&descriptor)
            .map_err(|error| RendererError::AllocationFailed {
                resource: "Metal4 command allocator".into(),
                reason: error.localizedDescription().to_string(),
            })
    }

    pub(crate) fn create_command_buffer(&self) -> MetalCommandBuffer {
        let allocator = self
            .create_command_allocator(usize::MAX)
            .expect("Metal4 command allocator");
        self.create_command_buffer_for_allocator(allocator, "standalone")
    }

    pub(crate) fn create_command_buffer_for_allocator(
        &self,
        allocator: Retained<ProtocolObject<dyn MTL4CommandAllocator>>,
        label: &str,
    ) -> MetalCommandBuffer {
        let inner = self
            .device
            .newCommandBuffer()
            .expect("Metal4 command buffer");
        MetalCommandBuffer {
            inner,
            allocator,
            completion: Default::default(),
            resources: super::encoding_resources::EncodingResources::new(&self.device, label),
            recording: std::cell::Cell::new(false),
        }
    }

    pub(crate) fn pipeline_compiler(&self) -> Result<MetalPipelineCompiler, RendererError> {
        Ok(MetalPipelineCompiler {
            device: self.device.clone(),
            archive: self.pipeline_archive.clone().ok_or_else(|| {
                RendererError::InitializationFailed(
                    "Metal pipeline compiler service unavailable".into(),
                )
            })?,
        })
    }

    #[cfg(test)]
    pub(crate) fn create_graphics_pipeline(
        &self,
        config: GraphicsPipelineConfig<'_>,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        self.pipeline_compiler()?.create_graphics_pipeline(config)
    }

    pub(crate) fn create_compute_pipeline(
        &self,
        function: &ProtocolObject<dyn MTLFunction>,
        workgroup: [u32; 3],
    ) -> Result<MetalComputePipeline, RendererError> {
        let pipeline_state = self
            .pipeline_archive
            .as_ref()
            .ok_or_else(|| {
                RendererError::InitializationFailed(
                    "Metal pipeline compiler service unavailable".into(),
                )
            })?
            .create_compute_pipeline(function, workgroup)?;
        Ok(MetalComputePipeline {
            uniform_bindings: Vec::new(),
            table_layout: super::shader::function_layout(function)?,
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

// SAFETY: `MetalContext` owns `MTLDevice` and `MTL4CommandQueue` plus immutable
// feature/capability state. Apple's Metal documentation guarantees both
// `MTLDevice` ("A GPU ... you can access ... from multiple threads") and
// `MTL4CommandQueue` ("MTL4CommandQueue is thread-safe") for concurrent use; the
// context holds no encoder, drawable, or layer state. Command *buffers* allocated
// from the queue are NOT thread-safe and are confined to the encoding thread by
// the `!Send`/`!Sync` command-buffer and encoder types in this module.
unsafe impl Send for MetalContext {}
unsafe impl Sync for MetalContext {}

fn portable_stencil_op(
    operation: crate::renderer::pipeline_descriptor::StencilOperation,
) -> objc2_metal::MTLStencilOperation {
    use crate::renderer::pipeline_descriptor::StencilOperation::*;
    match operation {
        Keep => objc2_metal::MTLStencilOperation::Keep,
        Zero => objc2_metal::MTLStencilOperation::Zero,
        Replace => objc2_metal::MTLStencilOperation::Replace,
        IncrementClamp => objc2_metal::MTLStencilOperation::IncrementClamp,
        DecrementClamp => objc2_metal::MTLStencilOperation::DecrementClamp,
        Invert => objc2_metal::MTLStencilOperation::Invert,
        IncrementWrap => objc2_metal::MTLStencilOperation::IncrementWrap,
        DecrementWrap => objc2_metal::MTLStencilOperation::DecrementWrap,
    }
}

/// Same-device pipeline preparation without frame, queue or surface ownership.
pub(crate) struct MetalPipelineCompiler {
    pub(crate) device: Retained<ProtocolObject<dyn MTLDevice>>,
    archive: std::sync::Arc<super::pipeline_archive::MetalPipelineArchive>,
}
// SAFETY: Metal permits concurrent MTLDevice use. The archive synchronizes its
// mutable state; pipeline descriptors are created and owned by the receiving worker.
unsafe impl Send for MetalPipelineCompiler {}

impl MetalPipelineCompiler {
    pub(crate) fn create_graphics_pipeline(
        &self,
        config: GraphicsPipelineConfig<'_>,
    ) -> Result<MetalGraphicsPipeline, RendererError> {
        let GraphicsPipelineConfig {
            vertex_function,
            fragment_function,
            color_formats,
            depth_format,
            depth_write_enabled,
            depth_compare,
            cull_mode,
            front_face,
            vertex_descriptor,
            alpha_blended,
            portable,
        } = config;
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(vertex_function));
        descriptor.setFragmentFunction(fragment_function);
        descriptor.setRasterSampleCount(1);

        let color_attachments = descriptor.colorAttachments();
        for (i, &format) in color_formats.iter().enumerate() {
            let attachment = unsafe { color_attachments.objectAtIndexedSubscript(i) };
            attachment.setPixelFormat(format);
            if let Some(portable) = portable {
                let mask = portable.color_write_mask.0;
                let mut native = objc2_metal::MTLColorWriteMask::None;
                if mask & 1 != 0 {
                    native |= objc2_metal::MTLColorWriteMask::Red;
                }
                if mask & 2 != 0 {
                    native |= objc2_metal::MTLColorWriteMask::Green;
                }
                if mask & 4 != 0 {
                    native |= objc2_metal::MTLColorWriteMask::Blue;
                }
                if mask & 8 != 0 {
                    native |= objc2_metal::MTLColorWriteMask::Alpha;
                }
                attachment.setWriteMask(native);
            }

            if alpha_blended {
                attachment.setBlendingEnabled(true);
                attachment.setSourceRGBBlendFactor(objc2_metal::MTLBlendFactor::SourceAlpha);
                attachment
                    .setDestinationRGBBlendFactor(objc2_metal::MTLBlendFactor::OneMinusSourceAlpha);
                attachment.setRgbBlendOperation(objc2_metal::MTLBlendOperation::Add);
                attachment.setSourceAlphaBlendFactor(objc2_metal::MTLBlendFactor::One);
                attachment.setDestinationAlphaBlendFactor(objc2_metal::MTLBlendFactor::Zero);
                attachment.setAlphaBlendOperation(objc2_metal::MTLBlendOperation::Add);
            }
        }

        if let Some(depth_fmt) = depth_format {
            descriptor.setDepthAttachmentPixelFormat(depth_fmt);
            if depth_fmt == MTLPixelFormat::Depth32Float_Stencil8
                || depth_fmt == MTLPixelFormat::Depth24Unorm_Stencil8
            {
                descriptor.setStencilAttachmentPixelFormat(depth_fmt);
            }
        }

        descriptor.setVertexDescriptor(Some(vertex_descriptor));

        let pipeline_state = self.archive.create_render_pipeline(&descriptor, &format!("depth_write={depth_write_enabled};depth_compare={depth_compare:?};cull={cull_mode:?};front={front_face:?};portable={portable:?}"))?;

        let mut depth_stencil_state = if depth_format.is_some() {
            Some(self.create_depth_stencil_state(
                depth_write_enabled,
                to_mtl_compare_func(depth_compare),
            )?)
        } else {
            None
        };

        if let Some(stencil) = portable.and_then(|descriptor| descriptor.stencil) {
            let descriptor = MTLDepthStencilDescriptor::new();
            descriptor.setDepthCompareFunction(to_mtl_compare_func(depth_compare));
            descriptor.setDepthWriteEnabled(depth_write_enabled);
            for (front, face) in [(true, stencil.front), (false, stencil.back)] {
                let native = MTLStencilDescriptor::new();
                native.setStencilCompareFunction(to_mtl_compare_func(face.compare));
                native.setStencilFailureOperation(portable_stencil_op(face.fail));
                native.setDepthFailureOperation(portable_stencil_op(face.depth_fail));
                native.setDepthStencilPassOperation(portable_stencil_op(face.pass));
                native.setReadMask(stencil.read_mask);
                native.setWriteMask(stencil.write_mask);
                if front {
                    descriptor.setFrontFaceStencil(Some(&native));
                } else {
                    descriptor.setBackFaceStencil(Some(&native));
                }
            }
            depth_stencil_state = Some(
                self.device
                    .newDepthStencilStateWithDescriptor(&descriptor)
                    .ok_or_else(|| {
                        RendererError::InitializationFailed(
                            "Depth/stencil state creation failed".into(),
                        )
                    })?,
            );
        }
        Ok(MetalGraphicsPipeline {
            vertex_layout: super::shader::function_layout(vertex_function)?,
            fragment_layout: fragment_function
                .map(super::shader::function_layout)
                .transpose()?,
            pipeline_state,
            depth_stencil_state,
            cull_mode,
            front_face,
            depth_bias: portable.map(|descriptor| {
                (
                    descriptor.depth_bias.constant,
                    descriptor.depth_bias.slope_factor,
                    descriptor.depth_bias.clamp,
                )
            }),
            stencil_reference: portable
                .and_then(|descriptor| descriptor.stencil.map(|state| state.reference)),
            wireframe: portable.is_some_and(|descriptor| descriptor.wireframe),
        })
    }

    fn create_depth_stencil_state(
        &self,
        depth_write_enabled: bool,
        compare_func: MTLCompareFunction,
    ) -> Result<Retained<ProtocolObject<dyn MTLDepthStencilState>>, RendererError> {
        let descriptor = MTLDepthStencilDescriptor::new();
        descriptor.setDepthWriteEnabled(depth_write_enabled);
        descriptor.setDepthCompareFunction(compare_func);
        self.device
            .newDepthStencilStateWithDescriptor(&descriptor)
            .ok_or_else(|| {
                RendererError::ResourceCreationFailed("Depth-stencil state creation failed".into())
            })
    }
}

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
    fn test_metal_context_capability_precedes_native_creation() {
        let Some(device) = MTLCreateSystemDefaultDevice() else {
            return;
        };
        let supports_metal4 = device.supportsFamily(MTLGPUFamily::Metal4);
        match MetalContext::create_command_queue(&device) {
            Ok(_) => assert!(supports_metal4),
            Err(RendererError::UnsupportedFeature(reason)) => {
                assert!(!supports_metal4);
                assert!(reason.contains("MTLGPUFamilyMetal4"));
                assert!(reason.contains(&device.name().to_string()));
            }
            Err(error) => panic!("unexpected Metal queue creation failure: {error}"),
        }
    }

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
            shader::ShaderProfile::Graphics,
        )
        .unwrap();

        let vs = shader.module.entry_points.get("vs_main").unwrap();
        let fs = shader.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx.create_graphics_pipeline(GraphicsPipelineConfig {
            vertex_function: vs,
            fragment_function: Some(fs),
            color_formats: &[MTLPixelFormat::BGRA8Unorm_sRGB],
            depth_format: Some(MTLPixelFormat::Depth32Float),
            depth_write_enabled: true,
            depth_compare: CompareOp::LessOrEqual,
            cull_mode: objc2_metal::MTLCullMode::Back,
            front_face: objc2_metal::MTLWinding::Clockwise,
            vertex_descriptor: &objc2_metal::MTLVertexDescriptor::new(),
            alpha_blended: false,
            portable: None,
        });
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
            shader::ShaderProfile::Graphics,
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
            shader::ShaderProfile::Graphics,
        )
        .unwrap();

        let vs = shader.module.entry_points.get("vs_main").unwrap();
        let fs = shader.module.entry_points.get("fs_main").unwrap();

        let pipeline = ctx
            .create_graphics_pipeline(GraphicsPipelineConfig {
                vertex_function: vs,
                fragment_function: Some(fs),
                color_formats: &[MTLPixelFormat::BGRA8Unorm_sRGB],
                depth_format: None,
                depth_write_enabled: false,
                depth_compare: CompareOp::Always,
                cull_mode: objc2_metal::MTLCullMode::None,
                front_face: objc2_metal::MTLWinding::Clockwise,
                vertex_descriptor: &MTLVertexDescriptor::new(),
                alpha_blended: false,
                portable: None,
            })
            .unwrap();

        let desc = TextureDescriptor::new(256, 256, crate::texture::ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT);
        let (_texture, view) = ctx.create_texture(&desc).unwrap();

        let dummy_vb = ctx.create_buffer(4, true).unwrap();

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
            debug_label: Some("test_pass"),
        };

        let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.bind_graphics_pipeline(&pipeline);
        encoder.bind_vertex_buffer(&dummy_vb, 0, 10);
        encoder.draw(3, 1, 0, 0);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.wait_until_completed().unwrap();
    }
}
