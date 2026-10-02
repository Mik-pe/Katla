use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTL4CommandAllocator, MTL4CommandBuffer, MTL4CommandEncoder, MTL4CommandQueue,
    MTL4CommitFeedback, MTL4CommitOptions, MTL4RenderPassDescriptor,
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
    pub(crate) inner: Retained<ProtocolObject<dyn MTL4CommandBuffer>>,
    pub(crate) allocator: Retained<ProtocolObject<dyn MTL4CommandAllocator>>,
    pub(crate) completion: super::submission::SubmissionCompletion,
    pub(crate) resources: std::rc::Rc<super::encoding_resources::EncodingResources>,
}

impl MetalCommandBuffer {
    pub(crate) fn render_pass_descriptor(
        desc: &RenderPassInfo<MetalBackend>,
    ) -> Retained<MTL4RenderPassDescriptor> {
        let pass_desc = MTL4RenderPassDescriptor::new();

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
                stencil_desc.setLoadAction(to_mtl_load_action(depth.stencil_ops.load));
                stencil_desc.setStoreAction(to_mtl_store_action(depth.stencil_ops.store));
                if depth.stencil_ops.load == LoadOp::Clear
                    && let ClearValue::DepthStencil { stencil: s, .. } =
                        depth.stencil_ops.clear_value
                {
                    stencil_desc.setClearStencil(s);
                }
            }
        }

        pass_desc
    }
}

impl GpuCommandBuffer<MetalBackend> for MetalCommandBuffer {
    fn begin(&mut self) {
        self.inner.beginCommandBufferWithAllocator(&self.allocator);
        self.inner
            .useResidencySet(self.resources.residency.native());
    }

    fn end(&mut self) {
        self.inner.endCommandBuffer();
    }

    fn submit(&self, context: &<MetalBackend as GpuBackend>::Context) {
        if let Err(error) = self.resources.check() {
            log::error!("Metal encoding rejected: {error}");
            return;
        }
        assert!(
            self.completion.mark_submitted(),
            "command buffer may only be submitted once"
        );
        self.resources.residency.commit();
        let completion = self.completion.clone();
        let feedback = RcBlock::new(
            move |native: NonNull<ProtocolObject<dyn MTL4CommitFeedback>>| {
                let native = unsafe { native.as_ref() };
                let error = native
                    .error()
                    .as_ref()
                    .map(|error| super::submission::CommitError::from_native(error));
                if let Some(error) = &error {
                    log::error!("Metal4 submission failed: {error:?}");
                }
                completion.finish(super::submission::CommitFeedback {
                    gpu_start: native.GPUStartTime(),
                    gpu_end: native.GPUEndTime(),
                    error,
                });
            },
        );
        let options = MTL4CommitOptions::new();
        let mut buffers = [NonNull::from(&*self.inner)];
        unsafe {
            options.addFeedbackHandler(RcBlock::as_ptr(&feedback));
            context
                .command_queue
                .commit_count_options(NonNull::from(&mut buffers[0]), 1, &options);
        }
    }

    fn begin_render_pass(&mut self, desc: RenderPassInfo<MetalBackend>) -> MetalRenderEncoder {
        for attachment in &desc.color_attachments {
            self.resources
                .residency
                .add_texture(&attachment.view.inner)
                .expect("Metal attachment residency");
        }
        if let Some(attachment) = &desc.depth_attachment {
            self.resources
                .residency
                .add_texture(&attachment.view.inner)
                .expect("Metal depth residency");
        }
        let pass_desc = Self::render_pass_descriptor(&desc);

        let encoder = self
            .inner
            .renderCommandEncoderWithDescriptor(&pass_desc)
            .expect("Failed to create render encoder");
        if let Some(label) = desc.debug_label {
            encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        }
        MetalRenderEncoder::new(encoder, self.resources.clone())
    }

    fn begin_compute_pass_with_label(&mut self, label: &'static str) -> MetalComputeEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create compute encoder");
        encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        MetalComputeEncoder::new(encoder, self.resources.clone())
    }

    fn begin_blit_pass_with_label(&mut self, label: &'static str) -> MetalBlitEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create blit encoder");
        encoder.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        MetalBlitEncoder::new(encoder, self.resources.clone())
    }

    fn begin_compute_pass(&mut self) -> MetalComputeEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create compute encoder");
        MetalComputeEncoder::new(encoder, self.resources.clone())
    }

    fn begin_blit_pass(&mut self) -> MetalBlitEncoder {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create blit encoder");
        MetalBlitEncoder::new(encoder, self.resources.clone())
    }

    fn copy_buffer_to_texture(
        &mut self,
        src: &MetalBuffer,
        dst: &MetalTexture,
        regions: &[BufferImageCopy],
    ) {
        let encoder = self
            .inner
            .computeCommandEncoder()
            .expect("Failed to create blit encoder for copy");

        let mut blit = super::blit_encoder::MetalBlitEncoder::new(encoder, self.resources.clone());
        blit.copy_buffer_to_texture(src, dst, regions);
        blit.inner.endEncoding();
    }
}

impl MetalCommandBuffer {
    pub(crate) fn wait_until_completed(
        &self,
    ) -> Result<super::submission::CommitFeedback, crate::error::RendererError> {
        let label = self
            .inner
            .label()
            .map(|v| v.to_string())
            .unwrap_or_default();
        self.resources.check()?;
        self.completion.result(&label)
    }
}

impl Drop for MetalCommandBuffer {
    fn drop(&mut self) {
        self.completion.wait();
    }
}

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
        cmd_buffer.wait_until_completed().unwrap();
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
        cmd_buffer.wait_until_completed().unwrap();
    }

    #[test]
    fn test_compute_dispatch() {
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
        let pipeline = ctx.create_compute_pipeline(cs, [64, 1, 1]).unwrap();

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let mut encoder = cmd_buffer.begin_compute_pass();
        encoder.bind_compute_pipeline(&pipeline);
        encoder.bind_storage_buffer(&buffer, 0, 0);
        encoder.dispatch(1, 1, 1);
        encoder.end_encoding();

        cmd_buffer.end();
        cmd_buffer.submit(&ctx);
        cmd_buffer.wait_until_completed().unwrap();
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
        cmd_buffer.wait_until_completed().unwrap();
    }
}
