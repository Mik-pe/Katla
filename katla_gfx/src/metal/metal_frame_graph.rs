use std::collections::HashMap;

use objc2_metal::MTLCommandBuffer;

use crate::backend::command::{
    ColorAttachmentInfo, DepthAttachmentInfo, GpuCommandBuffer, RenderPassInfo,
};
use crate::backend::traits::GpuBackend;
use crate::error::RendererError;
use crate::render_pass::ResourceState;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::{ImageFormat, TextureUsage};

use super::context::MetalBackend;
use super::context::MetalContext;
use super::metal_transient_texture::MetalTransientTexture;

pub(crate) struct MetalPassDesc {
    pub name: String,
    pub color_formats: Vec<ImageFormat>,
    pub depth_format: Option<ImageFormat>,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct MetalFrameGraph {
    passes: Vec<MetalPassDesc>,
    transient_textures: HashMap<String, MetalTransientTexture>,
}

impl MetalFrameGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            transient_textures: HashMap::new(),
        }
    }

    pub fn add_pass(&mut self, pass: MetalPassDesc) {
        self.passes.push(pass);
    }

    pub fn create_resources(&mut self, ctx: &MetalContext) -> Result<(), RendererError> {
        for pass in &self.passes {
            for (i, format) in pass.color_formats.iter().enumerate() {
                let name = format!("{}_color_{}", pass.name, i);
                if !self.transient_textures.contains_key(&name) {
                    let desc =
                        crate::texture::TextureDescriptor::new(pass.width, pass.height, *format)
                            .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
                    let (texture, view) = ctx.create_texture(&desc)?;
                    self.transient_textures.insert(
                        name,
                        MetalTransientTexture::new(texture, view, *format, pass.width, pass.height),
                    );
                }
            }
            if let Some(depth_fmt) = pass.depth_format {
                let name = format!("{}_depth", pass.name);
                if !self.transient_textures.contains_key(&name) {
                    let desc =
                        crate::texture::TextureDescriptor::new(pass.width, pass.height, depth_fmt)
                            .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
                    let (texture, view) = ctx.create_texture(&desc)?;
                    self.transient_textures.insert(
                        name,
                        MetalTransientTexture::new(
                            texture,
                            view,
                            depth_fmt,
                            pass.width,
                            pass.height,
                        ),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn execute_render_pass<F>(
        &self,
        ctx: &MetalContext,
        pass_name: &str,
        f: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(<MetalBackend as GpuBackend>::RenderEncoder),
    {
        let pass = self
            .passes
            .iter()
            .find(|p| p.name == pass_name)
            .ok_or_else(|| RendererError::NotFound(format!("Pass '{}' not found", pass_name)))?;

        let mut color_attachments = Vec::new();
        for i in 0..pass.color_formats.len() {
            let name = format!("{}_color_{}", pass_name, i);
            let transient = self
                .transient_textures
                .get(&name)
                .ok_or_else(|| RendererError::NotFound(format!("Texture '{}' not found", name)))?;
            transient.state.set(ResourceState::ColorAttachment);
            color_attachments.push(ColorAttachmentInfo {
                view: transient.view.clone(),
                load_op: LoadOp::Clear,
                store_op: StoreOp::Store,
                clear_value: ClearValue::color(0.0, 0.0, 0.0, 1.0),
            });
        }

        let depth_attachment = if let Some(_depth_fmt) = pass.depth_format {
            let name = format!("{}_depth", pass_name);
            self.transient_textures.get(&name).map(|transient| {
                transient.state.set(ResourceState::DepthStencilAttachment);
                DepthAttachmentInfo {
                    view: transient.view.clone(),
                    load_op: LoadOp::Clear,
                    store_op: StoreOp::Store,
                    clear_value: ClearValue::depth_stencil(1.0, 0),
                    format: transient.format,
                }
            })
        } else {
            None
        };

        let mut cmd_buffer = ctx.create_command_buffer();
        cmd_buffer.begin();

        let render_pass_info = RenderPassInfo {
            color_attachments,
            depth_attachment,
        };

        let encoder = cmd_buffer.begin_render_pass(render_pass_info);
        f(encoder);

        cmd_buffer.end();
        cmd_buffer.submit(ctx);
        cmd_buffer.inner.waitUntilCompleted();

        Ok(())
    }

    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    pub fn cleanup(&mut self) {
        self.transient_textures.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::GpuRenderEncoder;

    #[test]
    fn test_metal_frame_graph_basic() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut graph = MetalFrameGraph::new();

        graph.add_pass(MetalPassDesc {
            name: "geometry".into(),
            color_formats: vec![ImageFormat::R16G16B16A16Sfloat],
            depth_format: Some(ImageFormat::D32Sfloat),
            width: 256,
            height: 256,
        });

        graph.add_pass(MetalPassDesc {
            name: "tonemap".into(),
            color_formats: vec![ImageFormat::B8G8R8A8Srgb],
            depth_format: None,
            width: 256,
            height: 256,
        });

        graph.create_resources(&ctx).unwrap();
        assert_eq!(graph.pass_count(), 2);

        graph
            .execute_render_pass(&ctx, "geometry", |encoder| {
                encoder.end_encoding();
            })
            .unwrap();

        graph
            .execute_render_pass(&ctx, "tonemap", |encoder| {
                encoder.end_encoding();
            })
            .unwrap();
    }

    #[test]
    fn test_metal_frame_graph_unknown_pass() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut graph = MetalFrameGraph::new();

        graph.add_pass(MetalPassDesc {
            name: "test".into(),
            color_formats: vec![ImageFormat::B8G8R8A8Srgb],
            depth_format: None,
            width: 128,
            height: 128,
        });

        graph.create_resources(&ctx).unwrap();

        let result = graph.execute_render_pass(&ctx, "nonexistent", |_| {});
        assert!(result.is_err());
    }

    #[test]
    fn test_metal_frame_graph_cleanup() {
        let ctx = MetalContext::init_headless().unwrap();
        let mut graph = MetalFrameGraph::new();

        graph.add_pass(MetalPassDesc {
            name: "pass_a".into(),
            color_formats: vec![ImageFormat::R8G8B8A8Srgb],
            depth_format: Some(ImageFormat::D32Sfloat),
            width: 64,
            height: 64,
        });

        graph.create_resources(&ctx).unwrap();
        assert_eq!(graph.pass_count(), 1);

        graph.cleanup();
        assert_eq!(graph.pass_count(), 1);
    }
}
