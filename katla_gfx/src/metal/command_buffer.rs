use std::ptr::NonNull;

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

        let error = match cmd_buffer.error() {
            Some(e) => e,
            None => {
                log::error!(
                    "Metal command buffer '{}' failed with Error status but no NSError",
                    label
                );
                return;
            }
        };

        let code = error.code() as u64;
        let domain = error.domain().to_string();
        let description = error.localizedDescription().to_string();

        let error_kind = match code {
            1 => "Internal",
            2 => "Timeout",
            3 => "PageFault",
            4 => "AccessRevoked",
            7 => "NotPermitted",
            8 => "OutOfMemory",
            9 => "InvalidResource",
            10 => "Memoryless",
            11 => "DeviceRemoved",
            12 => "StackOverflow",
            _ => "Unknown",
        };

        log::error!(
            "Metal GPU error [{}]: {} (code={} domain={}):\n  {}",
            label,
            error_kind,
            code,
            domain,
            description,
        );
    }
}

impl GpuCommandBuffer<MetalBackend> for MetalCommandBuffer {
    fn begin(&mut self) {}

    fn end(&mut self) {}

    fn submit(&self, _context: &<MetalBackend as GpuBackend>::Context) {
        #[allow(clippy::type_complexity)]
        let block: RcBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)> = RcBlock::new(
            |cmd_buffer: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                let cmd_buffer = unsafe { cmd_buffer.as_ref() };
                Self::log_gpu_error(cmd_buffer);
            },
        );
        unsafe {
            self.inner.addCompletedHandler(RcBlock::into_raw(block));
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
        MetalRenderEncoder::new(encoder)
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
    use crate::backend::command::*;
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
