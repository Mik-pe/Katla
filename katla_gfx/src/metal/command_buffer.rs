use std::ptr::NonNull;
use std::sync::OnceLock;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLRenderPassDescriptor,
};

use crate::backend::command::*;
use crate::backend::traits::GpuBackend;
use crate::render_pass::{ClearValue, LoadOp};

use super::MetalBackend;
use super::blit_encoder::MetalBlitEncoder;
use super::buffer::MetalBuffer;
use super::compute_encoder::MetalComputeEncoder;
use super::format::{to_mtl_load_action, to_mtl_store_action};
use super::render_encoder::MetalRenderEncoder;
use super::texture::MetalTexture;

pub(crate) struct MetalCommandBuffer {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
}

impl MetalCommandBuffer {
    fn log_gpu_error(cmd_buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
        let status = cmd_buffer.status();
        if status != MTLCommandBufferStatus::Error {
            return;
        }

        let label = cmd_buffer
            .label()
            .map(|l| l.to_string())
            .unwrap_or_default();

        let Some(error) = cmd_buffer.error() else {
            log::error!(
                "Metal command buffer '{}' failed with Error status but no NSError",
                label
            );
            return;
        };

        match super::diagnostics::GpuCommandBufferDiagnostics::from_error(&label, &error) {
            Some(diagnostics) => {
                log::error!("Metal GPU failure: {}", diagnostics.render());
                if let Some(faulted) = diagnostics.faulted_encoder() {
                    log::error!(
                        "First faulted encoder: '{}' (signposts: {})",
                        faulted.label,
                        if faulted.debug_signposts.is_empty() {
                            "none".to_owned()
                        } else {
                            faulted.debug_signposts.join(",")
                        }
                    );
                }
            }
            None => log::error!(
                "Metal command buffer '{}' failed without diagnostics",
                label
            ),
        }
    }
}

impl GpuCommandBuffer<MetalBackend> for MetalCommandBuffer {
    fn begin(&mut self) {}

    fn end(&mut self) {}

    fn submit(&self, _context: &<MetalBackend as GpuBackend>::Context) {
        // The completion block captures nothing, so a single process-lifetime
        // instance is immutable and safe to register from any thread; Metal
        // retains it per command buffer. Allocating a fresh block per submit
        // would hand Metal one unbalanced Rc refcount per frame.
        // Data audit (issue #57): the handler runs on an arbitrary
        // Metal-managed thread. It receives only the ObjC-owned command-buffer
        // reference, reads its status/error, and logs — no captured Rust state,
        // no surface/layer mutation, nothing non-Send crosses the boundary.
        // Readback paths (picking) use synchronous waitUntilCompleted instead
        // of completion handlers.
        type CompletionBlock = RcBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)>;
        #[allow(clippy::type_complexity)]
        type RawCompletionBlock =
            *mut block2::DynBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)>;
        struct SharedBlock(RawCompletionBlock);
        // SAFETY: the wrapped pointer owns a capture-free Block: immutable after
        // creation, invocation thread-safe (Block ABI), never freed (leaked by
        // design so the single instance can be registered on every submit).
        unsafe impl Send for SharedBlock {}
        unsafe impl Sync for SharedBlock {}
        static COMPLETION: OnceLock<SharedBlock> = OnceLock::new();
        let SharedBlock(block_ptr) = COMPLETION.get_or_init(|| {
            #[allow(clippy::type_complexity)]
            let block: CompletionBlock = RcBlock::new(
                |cmd_buffer: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                    let cmd_buffer = unsafe { cmd_buffer.as_ref() };
                    Self::log_gpu_error(cmd_buffer);
                },
            );
            SharedBlock(RcBlock::into_raw(block))
        });
        unsafe {
            self.inner.addCompletedHandler(*block_ptr);
        }
        self.inner.commit();
    }

    fn begin_render_pass(&mut self, desc: RenderPassInfo<MetalBackend>) -> MetalRenderEncoder {
        let pass_desc = MTLRenderPassDescriptor::new();

        for (i, attachment) in desc.color_attachments.iter().enumerate() {
            let color_desc = unsafe { pass_desc.colorAttachments().objectAtIndexedSubscript(i) };
            color_desc.setTexture(Some(&attachment.view.inner));
            color_desc.setLoadAction(to_mtl_load_action(attachment.load_op));
            color_desc.setStoreAction(to_mtl_store_action(attachment.store_op));
            if attachment.load_op == LoadOp::Clear
                && let ClearValue::Color([r, g, b, a]) = attachment.clear_value
            {
                color_desc.setClearColor(objc2_metal::MTLClearColor {
                    red: r as f64,
                    green: g as f64,
                    blue: b as f64,
                    alpha: a as f64,
                });
            }
        }

        if let Some(ref depth) = desc.depth_attachment {
            let depth_desc = pass_desc.depthAttachment();
            depth_desc.setTexture(Some(&depth.view.inner));
            depth_desc.setLoadAction(to_mtl_load_action(depth.load_op));
            depth_desc.setStoreAction(to_mtl_store_action(depth.store_op));
            if depth.load_op == LoadOp::Clear
                && let ClearValue::DepthStencil { depth: d, .. } = depth.clear_value
            {
                depth_desc.setClearDepth(d as f64);
            }

            if matches!(
                depth.format,
                crate::texture::ImageFormat::D32SfloatS8Uint
                    | crate::texture::ImageFormat::D24UnormS8Uint
            ) {
                let stencil_desc = pass_desc.stencilAttachment();
                stencil_desc.setTexture(Some(&depth.view.inner));
                stencil_desc.setLoadAction(to_mtl_load_action(depth.load_op));
                stencil_desc.setStoreAction(to_mtl_store_action(depth.store_op));
                if depth.load_op == LoadOp::Clear
                    && let ClearValue::DepthStencil { stencil: s, .. } = depth.clear_value
                {
                    stencil_desc.setClearStencil(s);
                }
            }
        }

        let encoder = self
            .inner
            .renderCommandEncoderWithDescriptor(&pass_desc)
            .expect("Failed to create render encoder");
        if let Some(label) = desc.debug_label {
            encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        }
        MetalRenderEncoder::new(encoder)
    }

    fn begin_compute_pass_with_label(&mut self, label: &'static str) -> MetalComputeEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create compute encoder");
        encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        MetalComputeEncoder::new(encoder)
    }

    fn begin_blit_pass_with_label(&mut self, label: &'static str) -> MetalBlitEncoder {
        let encoder = self
            .inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder");
        encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        MetalBlitEncoder::new(encoder)
    }

    fn begin_compute_pass(&mut self) -> MetalComputeEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create compute encoder");
        MetalComputeEncoder::new(encoder)
    }

    fn begin_blit_pass(&mut self) -> MetalBlitEncoder {
        let encoder = self
            .inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder");
        MetalBlitEncoder::new(encoder)
    }

    fn copy_buffer_to_texture(
        &mut self,
        src: &MetalBuffer,
        dst: &MetalTexture,
        regions: &[BufferImageCopy],
    ) {
        let encoder = self
            .inner
            .blitCommandEncoder()
            .expect("Failed to create blit encoder for copy");

        let mut blit = super::blit_encoder::MetalBlitEncoder::new(encoder);
        blit.copy_buffer_to_texture(src, dst, regions);
        blit.inner.endEncoding();
    }
}

unsafe impl Send for MetalCommandBuffer {}
unsafe impl Sync for MetalCommandBuffer {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

    fn headless_context() -> super::super::context::MetalContext {
        super::super::context::MetalContext::init_headless().unwrap()
    }

    #[test]
    fn test_command_buffer_lifecycle() {
        let ctx = headless_context();
        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();
        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();
    }

    #[test]
    fn test_render_pass_clear() {
        let ctx = headless_context();

        let desc = TextureDescriptor::new(256, 256, ImageFormat::R8G8B8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
        let (_texture, view) = ctx.create_texture(&desc).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments: vec![ColorAttachmentInfo {
                view,
                load_op: LoadOp::Clear,
                store_op: crate::render_pass::StoreOp::Store,
                clear_value: ClearValue::color(1.0, 0.0, 0.0, 1.0),
            }],
            depth_attachment: None,
            debug_label: Some("test_pass"),
        };

        let encoder = cmd_buffer.begin_render_pass(render_pass_info);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();
    }

    #[test]
    fn test_compute_dispatch() {
        use objc2_metal::MTLDevice;

        let ctx = headless_context();

        let buffer = ctx.create_buffer(256, true).unwrap();

        let shader = super::super::shader::compile_wgsl_to_metal(
            &ctx.device,
            r#"
@group(0) @binding(0) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    if (gid.x < 64u) {
        output[gid.x] = f32(gid.x);
    }
}
"#,
            &["cs_main"],
            super::super::shader::ShaderProfile::Graphics,
        )
        .unwrap();

        let cs = shader.module.entry_points.get("cs_main").unwrap();
        let pipeline_state = ctx
            .device
            .newComputePipelineStateWithFunction_error(cs)
            .expect("Failed to create compute pipeline state");
        let pipeline = super::super::pipeline::MetalComputePipeline {
            pipeline_state,
            workgroup: [64, 1, 1],
        };

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let mut encoder = cmd_buffer.begin_compute_pass();
        encoder.bind_compute_pipeline(&pipeline);
        encoder.bind_storage_buffer(&buffer, 0, 0);
        encoder.dispatch(1, 1, 1);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();
    }

    #[test]
    fn test_blit_copy() {
        let ctx = headless_context();

        let src = ctx.create_buffer(1024, true).unwrap();
        let dst = ctx.create_buffer(1024, false).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let mut blit = cmd_buffer.begin_blit_pass();
        blit.copy_buffer_to_buffer(&src, 0, &dst, 0, 1024);
        blit.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.inner.waitUntilCompleted();
    }
}
