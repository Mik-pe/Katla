//! Explicit graph topology for the Katla editor.

use katla_gfx::render_graph::{
    FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassBuilder, PassType, SimplePass,
    UIPass,
};
use katla_gfx::{
    AttachmentOps, ClearValue, GpuRenderer, ImageFormat, LoadOp, PassBindings, PipelineDescriptor,
    SamplerBinding, SamplingMode, ShaderStages,
};

use super::frame_graph_config::{
    ApplicationFrameGraph, FrameGraphBindings, FrameGraphRuntime, KatlaEditorFrameGraphPreset,
};
use crate::{AppResult, FrameGraph, Renderer, resources::ResourceManager};

impl KatlaEditorFrameGraphPreset {
    /// Compose the editor's graph without installing features on the device.
    pub fn build(
        renderer: &mut Renderer,
        resources: &ResourceManager,
    ) -> AppResult<ApplicationFrameGraph> {
        let extent = renderer.swapchain_extent();
        let wallhack_overlay = matches!(renderer, Renderer::Vulkan(_));
        let tonemap_output = if wallhack_overlay {
            "tonemap_color"
        } else {
            "viewport_0"
        };
        let ui_material = renderer.compile_material(&PipelineDescriptor::ui(
            resources
                .shader_path("ui/ui.wgsl")
                .to_string_lossy()
                .into_owned(),
        ))?;
        let mut builder = FrameGraphBuilder::new();
        for (name, format) in [
            ("hdr_color", ImageFormat::R16G16B16A16Sfloat),
            ("object_id", ImageFormat::R32Uint),
            ("viewport_0", ImageFormat::B8G8R8A8Srgb),
        ] {
            builder = builder.create_resource(GraphResourceDesc {
                name: name.into(),
                resource_type: GraphResourceType::ColorAttachment { clear_value: None },
                format,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            });
        }
        if wallhack_overlay {
            builder = builder.create_resource(GraphResourceDesc {
                name: tonemap_output.into(),
                resource_type: GraphResourceType::ColorAttachment { clear_value: None },
                format: ImageFormat::B8G8R8A8Srgb,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            });
        }
        builder = builder
            .create_resource(GraphResourceDesc {
                name: "scene_depth".into(),
                resource_type: GraphResourceType::DepthAttachment {
                    clear_value: 0.0,
                    sampled: true,
                },
                format: ImageFormat::D32SfloatS8Uint,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            })
            .create_resource(GraphResourceDesc {
                name: "shadow_atlas".into(),
                resource_type: GraphResourceType::DepthAttachment {
                    clear_value: 1.0,
                    sampled: true,
                },
                format: ImageFormat::D32Sfloat,
                width: if wallhack_overlay { 4096 } else { 2048 },
                height: if wallhack_overlay { 4096 } else { 2048 },
                tracks_swapchain_size: false,
            });
        if wallhack_overlay {
            builder = builder.create_resource(GraphResourceDesc {
                name: "stencil_indicator".into(),
                resource_type: GraphResourceType::ColorAttachment { clear_value: None },
                format: ImageFormat::R8Unorm,
                width: extent.width,
                height: extent.height,
                tracks_swapchain_size: true,
            });
        }
        let clear_depth = AttachmentOps::clear(ClearValue::DepthStencil {
            depth: 0.0,
            stencil: 0,
        });
        let load_depth = clear_depth.with_load(LoadOp::Load);
        builder = builder
            .export_resource("viewport_0")
            .export_resource("object_id")
            .export_resource("backbuffer")
            .add_pass(
                SimplePass::new("sky", PassType::Graphics)
                    .without_depth()
                    .write("hdr_color")
                    .attachment("hdr_color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            )
            .add_pass(
                SimplePass::new("shadow", PassType::Graphics)
                    .depth_ops(
                        AttachmentOps::clear(ClearValue::DepthStencil {
                            depth: 1.0,
                            stencil: 0,
                        }),
                        AttachmentOps::dont_care(),
                    )
                    .depth_target("shadow_atlas"),
            )
            .add_pass(
                SimplePass::new("depth_prepass", PassType::Graphics)
                    .depth_ops(clear_depth, clear_depth)
                    .depth_target("scene_depth"),
            )
            .add_pass(
                SimplePass::new("geometry", PassType::Graphics)
                    .read("hdr_color")
                    .write("hdr_color")
                    .read("shadow_atlas")
                    .attachment("hdr_color", AttachmentOps::load())
                    .depth_ops(load_depth, clear_depth)
                    .depth_target("scene_depth"),
            )
            .add_pass(
                SimplePass::new("particles", PassType::Graphics)
                    .read("hdr_color")
                    .write("hdr_color")
                    .attachment("hdr_color", AttachmentOps::load())
                    .depth_ops(load_depth, load_depth)
                    .depth_target("scene_depth"),
            )
            .add_pass(
                SimplePass::new("outline", PassType::Graphics)
                    .read("hdr_color")
                    .write("hdr_color")
                    .attachment("hdr_color", AttachmentOps::load())
                    .depth_ops(load_depth, load_depth)
                    .depth_target("scene_depth"),
            )
            .add_pass(
                SimplePass::new("object_id", PassType::Graphics)
                    .write("object_id")
                    .attachment(
                        "object_id",
                        AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                    )
                    .depth_ops(load_depth, load_depth)
                    .depth_target("scene_depth"),
            );
        if wallhack_overlay {
            builder = builder.add_pass(
                SimplePass::new("stencil_indicator", PassType::Graphics)
                    .write("stencil_indicator")
                    .attachment(
                        "stencil_indicator",
                        AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                    )
                    .depth_ops(load_depth, load_depth)
                    .depth_target("scene_depth"),
            );
        }
        builder = builder.add_pass(
            SimplePass::new("tonemap", PassType::Graphics)
                .without_depth()
                .read("hdr_color")
                .write(tonemap_output)
                .attachment(
                    tonemap_output,
                    AttachmentOps::clear(ClearValue::OPAQUE_BLACK),
                ),
        );
        if wallhack_overlay {
            builder = builder.add_pass(
                SimplePass::new("wallhack_overlay", PassType::Graphics)
                    .without_depth()
                    .read(tonemap_output)
                    .read("stencil_indicator")
                    .write("viewport_0")
                    .attachment("viewport_0", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            );
        }
        builder = builder.add_pass(
            UIPass::new("ui")
                .read("viewport_0")
                .write_ops(
                    "backbuffer",
                    AttachmentOps::clear(ClearValue::color(0.022, 0.022, 0.022, 1.0)),
                )
                .material(ui_material),
        );
        let (mut graph, bindings) = match renderer {
            Renderer::Vulkan(_) => (
                FrameGraph::from_vulkan(builder.build::<katla_gfx::VulkanRenderer>()?),
                FrameGraphBindings::katla_editor(),
            ),
            #[cfg(target_os = "macos")]
            Renderer::Metal(_) => (
                FrameGraph::from_metal(builder.build::<katla_gfx::MetalRenderer>()?),
                FrameGraphBindings::katla_editor_metal(),
            ),
        };
        if let Some(ui) = graph.pass_id("ui") {
            graph.set_pass_bindings(
                ui,
                PassBindings {
                    samplers: vec![SamplerBinding {
                        group: 0,
                        binding: 1,
                        stages: ShaderStages::FRAGMENT,
                        sampling: SamplingMode::Linear,
                    }],
                    ..Default::default()
                },
            )?;
        }
        Ok(ApplicationFrameGraph::new(graph)
            .with_bindings(bindings)
            .with_runtime(FrameGraphRuntime::KatlaScene))
    }
}
