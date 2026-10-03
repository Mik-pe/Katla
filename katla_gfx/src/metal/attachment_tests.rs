//! GPU regression coverage for the compiled-to-native attachment contract.

use super::attachments::ResolvedMetalAttachments;
use super::command_buffer::MetalCommandBuffer;
use super::context::MetalContext;
use super::execution_plan::MetalExecutionPlan;
use super::metal_renderer::MetalRenderer;
use super::texture::MetalTextureView;
use crate::GpuRenderer;
use crate::backend::command::{GpuBlitEncoder, GpuCommandBuffer};
use crate::backend::resource::{GpuBuffer, GpuImageView};
use crate::render_graph::{
    FrameGraph, FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassBuilder, PassKind,
    PassType, SimplePass,
};
use crate::render_pass::{AttachmentOps, ClearValue, StoreOp};
use crate::renderer::frame_scope::FrameAcquisition;
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use objc2_metal::{MTLLoadAction, MTLStoreAction};
use std::collections::HashMap;

fn renderer() -> MetalRenderer {
    MetalRenderer::new(MetalContext::init_headless_with_size(16, 16).unwrap()).unwrap()
}

fn color(name: &str, format: ImageFormat) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.into(),
        resource_type: GraphResourceType::ColorAttachment { clear_value: None },
        format,
        width: 16,
        height: 16,
        tracks_swapchain_size: false,
    }
}

fn clear(name: &str, target: &str, value: ClearValue) -> SimplePass {
    SimplePass::new(name, PassType::Graphics)
        .without_depth()
        .write(target)
        .attachment(target, AttachmentOps::clear(value))
        .with_kind(PassKind::Geometry)
}

fn drawable(renderer: &MetalRenderer) -> MetalTextureView {
    let descriptor = TextureDescriptor::new(16, 16, ImageFormat::B8G8R8A8Srgb)
        .with_usage(TextureUsage::COLOR_ATTACHMENT | TextureUsage::SAMPLED);
    renderer
        .context
        .create_texture_shared(&descriptor)
        .unwrap()
        .1
}

fn pixel(renderer: &MetalRenderer, view: &MetalTextureView) -> [u8; 4] {
    let buffer = renderer.context.create_buffer(4, true).unwrap();
    let mut cmd = renderer.context.create_command_buffer();
    cmd.begin();
    let mut blit = cmd.begin_blit_pass_with_label("attachment_readback");
    blit.copy_texture_pixel_to_buffer(view.image(), 8, 8, &buffer);
    blit.end_encoding();
    cmd.end();
    cmd.submit(&renderer.context);
    cmd.wait_until_completed().unwrap();
    let value = unsafe { std::ptr::read(buffer.map() as *const [u8; 4]) };
    buffer.unmap();
    value
}

fn execute(
    renderer: &mut MetalRenderer,
    graph: &FrameGraph<MetalRenderer>,
) -> crate::render_graph::ResourceExecutionTrace {
    let view = drawable(renderer);
    renderer.set_headless_drawable(view.inner.clone());
    let frame = match GpuRenderer::acquire_frame(renderer).unwrap() {
        FrameAcquisition::Ready(frame) => frame,
        _ => panic!("headless frame unavailable"),
    };
    let plan =
        MetalExecutionPlan::compile(graph, ImageFormat::B8G8R8A8Srgb, Some(renderer)).unwrap();
    let trace = renderer
        .render_frame(&frame, &plan, HashMap::new(), graph, true)
        .unwrap();
    GpuRenderer::present(renderer, frame).unwrap();
    renderer.wait_for_frame_impl().unwrap();
    trace
}

#[test]
fn test_native_clear_store_and_load_store_preserve_distinct_geometry_targets() {
    let mut renderer = renderer();
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("red", ImageFormat::B8G8R8A8Srgb))
        .create_resource(color("green", ImageFormat::B8G8R8A8Srgb))
        .export_resource("red")
        .export_resource("green")
        .add_pass(clear(
            "paint_red",
            "red",
            ClearValue::color(1.0, 0.0, 0.0, 1.0),
        ))
        .add_pass(
            SimplePass::new("extend_red", PassType::Graphics)
                .without_depth()
                .read("red")
                .write("red")
                .attachment("red", AttachmentOps::load())
                .with_kind(PassKind::Geometry),
        )
        .add_pass(clear(
            "paint_green",
            "green",
            ClearValue::color(0.0, 1.0, 0.0, 1.0),
        ))
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    let view = drawable(&renderer);
    let plan =
        MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb, Some(&renderer)).unwrap();
    for (index, record) in plan.passes().iter().enumerate() {
        let resolved =
            ResolvedMetalAttachments::resolve(record, &graph, &view, 0, &renderer).unwrap();
        let native = MetalCommandBuffer::render_pass_descriptor(&resolved.info);
        let attachment = unsafe { native.colorAttachments().objectAtIndexedSubscript(0) };
        let expected = graph
            .transient_texture(if index == 2 { "green" } else { "red" }, 0)
            .unwrap();
        assert_eq!(attachment.texture().unwrap(), expected.view.inner);
        assert_eq!(attachment.storeAction(), MTLStoreAction::Store);
        assert_eq!(
            attachment.loadAction(),
            if index == 1 {
                MTLLoadAction::Load
            } else {
                MTLLoadAction::Clear
            }
        );
    }
    let trace = execute(&mut renderer, &graph);
    assert_eq!(
        trace
            .entries()
            .iter()
            .map(|entry| entry.color_targets.clone())
            .collect::<Vec<_>>(),
        vec![vec!["red"], vec!["red"], vec!["green"]]
    );
    assert_eq!(
        pixel(&renderer, &graph.transient_texture("red", 0).unwrap().view),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(
            &renderer,
            &graph.transient_texture("green", 0).unwrap().view
        ),
        [0, 255, 0, 255]
    );
    // The next acquired ownership slot resolves different native images.
    assert_ne!(
        graph.transient_texture("red", 0).unwrap().view.inner,
        graph.transient_texture("red", 1).unwrap().view.inner
    );
}

#[test]
fn test_native_depth_and_stencil_use_independent_operations_and_graph_identity() {
    let renderer = renderer();
    let mut depth = color("depth", ImageFormat::D32SfloatS8Uint);
    depth.resource_type = GraphResourceType::DepthAttachment {
        clear_value: 0.0,
        sampled: false,
    };
    let stencil = AttachmentOps::clear(ClearValue::DepthStencil {
        depth: 0.25,
        stencil: 7,
    })
    .with_store(StoreOp::DontCare);
    let mut graph = FrameGraphBuilder::new()
        .create_resource(depth)
        .export_resource("depth")
        .add_pass(
            SimplePass::new("depth_clear", PassType::Graphics)
                .with_kind(PassKind::DepthPrepass)
                .depth_ops(
                    AttachmentOps::clear(ClearValue::DepthStencil {
                        depth: 0.25,
                        stencil: 0,
                    }),
                    stencil,
                )
                .depth_target("depth"),
        )
        .add_pass(
            SimplePass::new("depth_load", PassType::Graphics)
                .with_kind(PassKind::DepthPrepass)
                .depth_ops(AttachmentOps::load(), stencil)
                .depth_target("depth"),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    let plan =
        MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb, Some(&renderer)).unwrap();
    for (index, record) in plan.passes().iter().enumerate() {
        let resolved =
            ResolvedMetalAttachments::resolve(record, &graph, &drawable(&renderer), 1, &renderer)
                .unwrap();
        assert_eq!(resolved.depth_target.as_deref(), Some("depth"));
        let native = MetalCommandBuffer::render_pass_descriptor(&resolved.info);
        assert_eq!(
            native.depthAttachment().texture().unwrap(),
            graph.transient_texture("depth", 1).unwrap().view.inner
        );
        assert_eq!(
            native.depthAttachment().loadAction(),
            if index == 0 {
                MTLLoadAction::Clear
            } else {
                MTLLoadAction::Load
            }
        );
        assert_eq!(
            native.depthAttachment().storeAction(),
            MTLStoreAction::Store
        );
        assert_eq!(
            native.stencilAttachment().loadAction(),
            MTLLoadAction::Clear
        );
        assert_eq!(
            native.stencilAttachment().storeAction(),
            MTLStoreAction::DontCare
        );
        assert_eq!(native.stencilAttachment().clearStencil(), 7);
        if index == 0 {
            assert_eq!(native.depthAttachment().clearDepth(), 0.25);
        }
    }
}

#[test]
fn test_two_fullscreen_passes_sample_their_own_inputs_and_render_to_distinct_targets() {
    let mut renderer = renderer();
    if super::argument_buffer::MetalBindlessTextureManager::unsupported_device_reason(
        &renderer.context.device,
    )
    .is_some()
    {
        return;
    }
    let source = format!(
        "{} @group(0) @binding(0) var image:texture_2d<f32>; @group(0) @binding(1) var image_sampler:sampler; @fragment fn fs_main()->@location(0) vec4<f32>{{return textureSample(image,image_sampler,vec2<f32>(0.5));}}",
        super::test_support::FULLSCREEN_VERTEX
    );
    let material = super::test_support::material(
        &mut renderer,
        &source,
        super::test_support::fullscreen_descriptor(ImageFormat::B8G8R8A8Srgb),
    );
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("hdr_red", ImageFormat::R16G16B16A16Sfloat))
        .create_resource(color("hdr_green", ImageFormat::R16G16B16A16Sfloat))
        .create_resource(color("ldr_red", ImageFormat::B8G8R8A8Srgb))
        .create_resource(color("ldr_green", ImageFormat::B8G8R8A8Srgb))
        .export_resource("ldr_red")
        .export_resource("ldr_green")
        .add_pass(clear(
            "red",
            "hdr_red",
            ClearValue::color(1.0, 0.0, 0.0, 1.0),
        ))
        .add_pass(clear(
            "green",
            "hdr_green",
            ClearValue::color(0.0, 1.0, 0.0, 1.0),
        ))
        .add_pass(
            SimplePass::new("tonemap_red", PassType::Graphics)
                .without_depth()
                .read("hdr_red")
                .write("ldr_red")
                .attachment("ldr_red", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .with_kind(PassKind::Fullscreen),
        )
        .add_pass(
            SimplePass::new("tonemap_green", PassType::Graphics)
                .without_depth()
                .read("hdr_green")
                .write("ldr_green")
                .attachment("ldr_green", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .with_kind(PassKind::Fullscreen),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    for (pass, input) in [("tonemap_red", "hdr_red"), ("tonemap_green", "hdr_green")] {
        let mut packet =
            super::test_support::vertices(material, crate::vertex::VertexLayout::new(vec![]), 3);
        packet
            .images
            .push(crate::renderer::frame_bindings::ImageBinding {
                group: 0,
                binding: 0,
                resource: graph.resource_id(input).unwrap(),
                range: crate::render_graph::ImageSubresourceRange::WHOLE_COLOR,
                stages: crate::backend::command::ShaderStages::FRAGMENT,
            });
        packet
            .samplers
            .push(crate::renderer::frame_bindings::SamplerBinding {
                group: 0,
                binding: 1,
                stages: crate::backend::command::ShaderStages::FRAGMENT,
                sampling: crate::SamplerDescriptor::nearest_clamp(),
            });
        graph
            .set_pass_bindings(graph.pass_id(pass).unwrap(), packet)
            .unwrap();
    }
    graph.collect_draw_lists(&mut renderer, |_| {}).unwrap();
    let trace = execute(&mut renderer, &graph);
    assert_eq!(trace.entries().len(), 4);
    assert_eq!(
        pixel(
            &renderer,
            &graph.transient_texture("ldr_red", 0).unwrap().view
        ),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(
            &renderer,
            &graph.transient_texture("ldr_green", 0).unwrap().view
        ),
        [0, 255, 0, 255]
    );
}

#[test]
fn test_ui_only_clear_without_draws_uses_imported_drawable_contract() {
    let mut renderer = renderer();
    let graph = FrameGraphBuilder::new()
        .add_pass(
            SimplePass::new("ui", PassType::Graphics)
                .without_depth()
                .write("backbuffer")
                .attachment(
                    "backbuffer",
                    AttachmentOps::clear(ClearValue::color(0.0, 1.0, 0.0, 1.0)),
                )
                .with_kind(PassKind::Ui),
        )
        .build::<MetalRenderer>()
        .unwrap();
    let view = drawable(&renderer);
    renderer.set_headless_drawable(view.inner.clone());
    let frame = match GpuRenderer::acquire_frame(&mut renderer).unwrap() {
        FrameAcquisition::Ready(frame) => frame,
        _ => panic!(),
    };
    let plan =
        MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb, Some(&renderer)).unwrap();
    let trace = renderer
        .render_frame(&frame, &plan, HashMap::new(), &graph, true)
        .unwrap();
    GpuRenderer::present(&mut renderer, frame).unwrap();
    renderer.wait_for_frame_impl().unwrap();
    assert_eq!(pixel(&renderer, &view), [0, 255, 0, 255]);
    assert_eq!(trace.entries()[0].color_targets, vec!["backbuffer"]);
    assert_eq!(trace.entries()[0].depth_target, None);
}

#[test]
fn test_empty_graph_emits_no_hidden_canvas_pass() {
    let mut renderer = renderer();
    let graph = FrameGraphBuilder::new().build::<MetalRenderer>().unwrap();
    assert!(execute(&mut renderer, &graph).entries().is_empty());
}

#[test]
fn test_unresolved_and_mismatched_attachment_extents_fail_before_native_encoding() {
    let renderer = renderer();
    let mut depth = color("depth", ImageFormat::D32SfloatS8Uint);
    depth.width = 8;
    depth.resource_type = GraphResourceType::DepthAttachment {
        clear_value: 0.0,
        sampled: false,
    };
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("color", ImageFormat::B8G8R8A8Srgb))
        .create_resource(depth)
        .export_resource("color")
        .add_pass(clear("invalid", "color", ClearValue::OPAQUE_BLACK).depth_target("depth"))
        .build::<MetalRenderer>()
        .unwrap();
    let plan =
        MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb, Some(&renderer)).unwrap();
    let record = &plan.passes()[0];
    let view = drawable(&renderer);
    assert!(
        ResolvedMetalAttachments::resolve(record, &graph, &view, 0, &renderer)
            .err()
            .unwrap()
            .to_string()
            .contains("cannot resolve")
    );
    graph.initialize_transient_textures(&renderer).unwrap();
    assert!(
        ResolvedMetalAttachments::resolve(record, &graph, &view, 0, &renderer)
            .err()
            .unwrap()
            .to_string()
            .contains("extents")
    );
}

#[test]
fn test_depth_kind_without_declared_resource_has_no_native_depth_attachment() {
    let graph = FrameGraphBuilder::new()
        .add_side_effect_pass(
            SimplePass::new("implicit_depth", PassType::Graphics).with_kind(PassKind::DepthPrepass),
        )
        .build::<MetalRenderer>()
        .unwrap();
    let plan = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb, None).unwrap();
    assert!(!plan.passes()[0].uses_depth);
    assert!(plan.passes()[0].depth_attachment.is_none());
}

#[test]
fn test_culled_geometry_has_no_native_encoder() {
    let mut renderer = renderer();
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("unused", ImageFormat::B8G8R8A8Srgb))
        .add_pass(clear(
            "culled",
            "unused",
            ClearValue::color(1.0, 0.0, 0.0, 1.0),
        ))
        .add_pass(
            SimplePass::new("visible", PassType::Graphics)
                .without_depth()
                .write("backbuffer")
                .attachment("backbuffer", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .with_kind(PassKind::Ui),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    graph.set_execution_trace(true);
    let trace = execute(&mut renderer, &graph);
    assert_eq!(trace.entries().len(), 1);
    assert_eq!(trace.entries()[0].name, "visible");
    graph.store_last_execution_trace(trace);
    assert!(graph.compare_execution_trace().is_empty());
}

#[test]
fn test_hdr_geometry_without_later_fullscreen_keeps_its_declared_target() {
    let mut renderer = renderer();
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("hdr", ImageFormat::R16G16B16A16Sfloat))
        .export_resource("hdr")
        .add_pass(clear(
            "hdr_only",
            "hdr",
            ClearValue::color(1.0, 0.0, 0.0, 1.0),
        ))
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    let trace = execute(&mut renderer, &graph);
    assert_eq!(trace.entries()[0].color_targets, vec!["hdr"]);
    let texture = &graph.transient_texture("hdr", 0).unwrap().view;
    let buffer = renderer.context.create_buffer(8, true).unwrap();
    let mut cmd = renderer.context.create_command_buffer();
    cmd.begin();
    let mut blit = cmd.begin_blit_pass_with_label("hdr_attachment_readback");
    blit.copy_texture_pixel_to_buffer(texture.image(), 8, 8, &buffer);
    blit.end_encoding();
    cmd.end();
    cmd.submit(&renderer.context);
    cmd.wait_until_completed().unwrap();
    let components = unsafe { std::ptr::read(buffer.map() as *const [u16; 4]) };
    buffer.unmap();
    assert_eq!(components, [0x3c00, 0, 0, 0x3c00]);
}

#[test]
fn test_native_fresh_backbuffer_load_requires_stored_contents_across_frames() {
    let mut renderer = renderer();
    let view = drawable(&renderer);
    let graph = |ops| {
        FrameGraphBuilder::new()
            .add_pass(
                SimplePass::new("backbuffer contents", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", ops)
                    .with_kind(PassKind::Geometry),
            )
            .build::<MetalRenderer>()
            .unwrap()
    };
    let mut load = graph(AttachmentOps::load());
    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    let error = renderer.render(&frame, &mut load, |_| {}).unwrap_err();
    assert!(
        matches!(error, crate::error::RendererError::InvalidOperation(_)),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("before its contents are defined"),
        "{error}"
    );
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert!(renderer.present(frame).is_err());

    let mut clear = graph(AttachmentOps::clear(ClearValue::color(1.0, 0.0, 0.0, 1.0)));
    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&frame, &mut clear, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    renderer.wait_for_slot(frame.slot()).unwrap();
    assert_eq!(pixel(&renderer, &view), [0, 0, 255, 255]);

    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&frame, &mut load, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    renderer.wait_for_slot(frame.slot()).unwrap();
    assert_eq!(pixel(&renderer, &view), [0, 0, 255, 255]);

    let mut discard = graph(AttachmentOps::load().with_store(StoreOp::DontCare));
    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&frame, &mut discard, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    renderer.wait_for_slot(frame.slot()).unwrap();
    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    assert!(renderer.render(&frame, &mut load, |_| {}).is_err());
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert!(renderer.present(frame).is_err());
}

#[test]
fn test_native_fresh_imported_depth_load_is_rejected_before_submission() {
    use crate::render_graph::{ImportedImageContract, ResourceState};
    let mut renderer = renderer();
    let descriptor = TextureDescriptor::new(16, 16, ImageFormat::D32SfloatS8Uint)
        .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT);
    let depth = renderer.create_texture_impl(&descriptor, &[]).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .import_resource(
            "imported depth",
            depth,
            ImportedImageContract::arrives_in(ResourceState::DepthStencilAttachment),
        )
        .export_resource("imported depth")
        .add_pass(
            SimplePass::new("load fresh depth", PassType::Graphics)
                .with_kind(PassKind::DepthPrepass)
                .depth_ops(AttachmentOps::load(), AttachmentOps::load())
                .depth_target("imported depth"),
        )
        .build::<MetalRenderer>()
        .unwrap();
    let view = drawable(&renderer);
    renderer.set_headless_drawable(view.inner);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    let error = renderer.render(&frame, &mut graph, |_| {}).unwrap_err();
    assert!(
        matches!(error, crate::error::RendererError::InvalidOperation(_)),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("before they are defined"),
        "{error}"
    );
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert!(renderer.present(frame).is_err());
}

#[test]
fn test_native_two_ui_passes_preserve_distinct_uploaded_vertex_colors() {
    use crate::renderer::pipeline_descriptor::PipelineDescriptor;
    use crate::renderer::types::{UIDrawList, UiDrawCommand};
    use crate::vertex::VertexUI;
    let mut renderer = renderer();
    let material = renderer
        .compile_material_impl(&PipelineDescriptor::ui("ui/ui.wgsl"))
        .unwrap();
    let ui_pass = |name, target| {
        crate::render_graph::UIPass::new(name)
            .write_ops(
                target,
                AttachmentOps::clear(ClearValue::color(0.0, 0.0, 0.0, 1.0)),
            )
            .material(material)
    };
    let mut graph = FrameGraphBuilder::new()
        .create_resource(color("red UI", ImageFormat::B8G8R8A8Srgb))
        .create_resource(color("green UI", ImageFormat::B8G8R8A8Srgb))
        .export_resource("red UI")
        .export_resource("green UI")
        .add_pass(ui_pass("paint red UI", "red UI"))
        .add_pass(ui_pass("paint green UI", "green UI"))
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    let draw_list = |color| UIDrawList {
        vertices: [[0.0, 0.0], [16.0, 0.0], [16.0, 16.0], [0.0, 16.0]]
            .map(|position| VertexUI::new(position, [0.0, 0.0], color, 0))
            .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        commands: vec![UiDrawCommand::vertex(0, 6, None)],
        screen_size: [16.0, 16.0],
        scale_factor: 1.0,
        ..Default::default()
    };
    let red = draw_list([255, 0, 0, 255]);
    let green = draw_list([0, 255, 0, 255]);
    let red_pass = graph.pass_id("paint red UI").unwrap();
    let green_pass = graph.pass_id("paint green UI").unwrap();
    for pass in [red_pass, green_pass] {
        graph
            .set_pass_bindings(
                pass,
                crate::renderer::frame_bindings::PassBindings {
                    samplers: vec![crate::renderer::frame_bindings::SamplerBinding {
                        group: 0,
                        binding: 1,
                        stages: crate::backend::command::ShaderStages::FRAGMENT,
                        sampling: crate::SamplerDescriptor::linear_clamp(),
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
    }

    let view = drawable(&renderer);
    renderer.set_headless_drawable(view.inner);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer
        .render(&frame, &mut graph, |frame| {
            frame.submit_ui(red_pass, &red);
            frame.submit_ui(green_pass, &green);
        })
        .unwrap();
    renderer.present(frame).unwrap();
    renderer.wait_for_slot(frame.slot()).unwrap();
    assert_eq!(
        pixel(
            &renderer,
            &graph
                .transient_texture("red UI", frame.slot())
                .unwrap()
                .view
        ),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(
            &renderer,
            &graph
                .transient_texture("green UI", frame.slot())
                .unwrap()
                .view
        ),
        [0, 255, 0, 255]
    );
}

#[test]
fn test_native_abort_discards_recorded_frame_and_uploads() {
    use objc2_metal::{MTLOrigin, MTLRegion, MTLSize, MTLTexture};
    let mut renderer = renderer();
    let view = drawable(&renderer);
    let blue = [255u8, 0, 0, 255].repeat(16 * 16);
    unsafe {
        view.inner.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: 16,
                    height: 16,
                    depth: 1,
                },
            },
            0,
            std::ptr::NonNull::new(blue.as_ptr() as *mut std::ffi::c_void).unwrap(),
            64,
        );
    }
    let upload = renderer
        .create_texture_impl(
            &TextureDescriptor::new(1, 1, ImageFormat::R8G8B8A8Unorm)
                .with_usage(TextureUsage::SAMPLED),
            &[17, 31, 47, 255],
        )
        .unwrap();
    let queued = renderer.texture_uploads.metrics().queued_bytes;
    let graph = |ops| {
        FrameGraphBuilder::new()
            .add_pass(
                SimplePass::new("backbuffer", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", ops)
                    .with_kind(PassKind::Geometry),
            )
            .build::<MetalRenderer>()
            .unwrap()
    };
    let mut clear = graph(AttachmentOps::clear(ClearValue::color(1.0, 0.0, 0.0, 1.0)));
    renderer.set_headless_drawable(view.inner.clone());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.begin_timestamp("aborted");
    renderer.render(&frame, &mut clear, |_| {}).unwrap();
    renderer.end_timestamp("aborted");
    assert!(renderer.pending_frame.is_some());
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert_eq!(renderer.texture_uploads.metrics().submitted_bytes, 0);
    assert!(renderer.defined_output_contents.is_empty());
    assert!(
        renderer
            .render(&frame, &mut clear, |_| panic!("second render callback"))
            .is_err()
    );
    assert!(
        GpuRenderer::execute_draw_calls(
            &mut renderer,
            &frame,
            &crate::renderer::types::DrawList::new()
        )
        .is_err()
    );
    assert_eq!(pixel(&renderer, &view), [255, 0, 0, 255]);
    renderer.abort(frame).unwrap();
    assert!(renderer.pending_frame.is_none());
    assert!(renderer.last_submitted_slot.is_none());
    assert!(renderer.read_timestamps().is_empty());
    assert_eq!(renderer.texture_uploads.metrics().queued_bytes, queued);
    let FrameAcquisition::Ready(next) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    assert_eq!(next.slot(), frame.slot());
    let mut load = graph(AttachmentOps::load());
    assert!(renderer.render(&next, &mut load, |_| {}).is_err());
    renderer.abort(next).unwrap();
    let FrameAcquisition::Ready(abandoned) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&abandoned, &mut clear, |_| {}).unwrap();
    let FrameAcquisition::Ready(final_frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    assert!(renderer.pending_frame.is_none());
    assert!(renderer.last_submitted_slot.is_none());
    assert_eq!(renderer.texture_uploads.metrics().queued_bytes, queued);
    renderer.begin_timestamp("committed");
    renderer.render(&final_frame, &mut clear, |_| {}).unwrap();
    renderer.end_timestamp("committed");
    renderer.present(final_frame).unwrap();
    renderer.wait_for_slot(final_frame.slot()).unwrap();
    assert_eq!(pixel(&renderer, &view), [0, 0, 255, 255]);
    let uploaded = &renderer.textures.get(upload).unwrap()._view;
    let probe = renderer.context.create_buffer(4, true).unwrap();
    let mut command = renderer.context.create_command_buffer();
    command.begin();
    let mut blit = command.begin_blit_pass();
    blit.copy_texture_pixel_to_buffer(uploaded.image(), 0, 0, &probe);
    blit.end_encoding();
    command.end();
    command.submit(&renderer.context);
    command.wait_until_completed().unwrap();
    assert_eq!(
        unsafe { std::ptr::read(probe.map().cast::<[u8; 4]>()) },
        [17, 31, 47, 255]
    );
    probe.unmap();
    assert_eq!(
        renderer.texture_uploads.metrics().submitted_bytes,
        queued as u64
    );
    let timestamps = renderer.read_timestamps();
    assert_eq!(timestamps.len(), 1);
    assert_eq!(timestamps[0].label, "committed");
    assert!(timestamps[0].duration_ms > 0.0);
}

#[test]
fn test_native_picking_retains_submitted_graph_source_and_global_instance_ids() {
    use crate::PipelineDescriptor;
    use crate::renderer::types::{DrawCall, DrawList, InstanceData};
    use std::rc::Rc;
    let mut renderer = renderer();
    renderer.resize(64, 64).unwrap();
    let source = "struct Object{model:mat4x4<f32>,color:vec4<f32>,textures:vec4<u32>,params:vec4<f32>}; @group(0) @binding(1) var<storage,read> objects:array<Object>; struct Varying{@builtin(position) position:vec4<f32>, @location(0) @interpolate(flat) id:u32}; @vertex fn vs_main(@location(0) p:vec3<f32>,@builtin(instance_index) i:u32)->Varying{var v:Varying;v.position=objects[i].model*vec4<f32>(p,1.);v.id=i+1u;return v;} @fragment fn fs_main(v:Varying)->@location(0) vec4<u32>{return vec4<u32>(v.id,0u,0u,0u);}";
    let material = super::test_support::material(
        &mut renderer,
        source,
        PipelineDescriptor::pbr("")
            .with_color_format(ImageFormat::R32Uint)
            .with_cull(crate::CullMode::None),
    );
    let mesh = crate::primitives::create_cube(&mut renderer, [0.5, 0.5, 0.5]).unwrap();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let instances = [-0.5f32, 0.5].map(|x| {
        let mut transform = identity;
        transform[12] = x;
        transform[14] = 0.5;
        InstanceData {
            model_matrix: transform,
            ..Default::default()
        }
    });
    let mut draw = DrawCall::instanced(mesh, material, instances.to_vec());
    draw.instance_index = 7;
    let list = Rc::new(DrawList::from_draws(vec![draw]));
    let desc = |name: &str, format, resource_type| GraphResourceDesc {
        name: name.into(),
        format,
        resource_type,
        width: 64,
        height: 64,
        tracks_swapchain_size: false,
    };
    let mut graph = FrameGraphBuilder::new()
        .create_resource(desc(
            "ids",
            ImageFormat::R32Uint,
            GraphResourceType::ColorAttachment { clear_value: None },
        ))
        .create_resource(desc(
            "depth",
            ImageFormat::D32SfloatS8Uint,
            GraphResourceType::DepthAttachment {
                clear_value: 0.0,
                sampled: false,
            },
        ))
        .export_resource("ids")
        .add_pass(
            SimplePass::new("pick objects", PassType::Graphics)
                .write("ids")
                .attachment("ids", AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK))
                .with_kind(PassKind::ObjectId)
                .depth_ops(
                    AttachmentOps::clear(ClearValue::DepthStencil {
                        depth: 0.0,
                        stencil: 0,
                    }),
                    AttachmentOps::dont_care(),
                )
                .depth_target("depth"),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    let pass = graph.pass_id("pick objects").unwrap();
    let drawable_desc = TextureDescriptor::new(64, 64, ImageFormat::B8G8R8A8Srgb)
        .with_usage(TextureUsage::COLOR_ATTACHMENT);
    let (_, drawable) = renderer
        .context
        .create_texture_shared(&drawable_desc)
        .unwrap();
    renderer.set_headless_drawable(drawable.inner);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    graph
        .set_pass_bindings(
            pass,
            crate::renderer::frame_bindings::PassBindings {
                pipelines: vec![crate::renderer::frame_bindings::PassPipeline {
                    vertex_layout: crate::vertex::VertexLayout::pbr(),
                    material,
                }],
                ..Default::default()
            },
        )
        .unwrap();
    renderer
        .render(&frame, &mut graph, |frame| {
            frame.submit(pass, list.clone());
        })
        .unwrap();
    let ids = graph.resource_id("ids").unwrap();
    assert!(renderer.graph_texture_source(ids).is_none());
    renderer.present(frame).unwrap();
    let source = renderer.graph_texture_source(ids).unwrap();
    assert_eq!(source.frame_slot, frame.slot());
    let left = renderer
        .queue_texture_readback(
            source,
            crate::renderer::texture_readback::TextureReadbackRegion::pixel(16, 32),
        )
        .unwrap();
    assert_eq!(
        super::test_support::readback(&mut renderer, left).single_u32(),
        Some(8)
    );
    let right = renderer
        .queue_texture_readback(
            source,
            crate::renderer::texture_readback::TextureReadbackRegion::pixel(48, 32),
        )
        .unwrap();
    let mut other =
        MetalRenderer::new(MetalContext::init_headless_with_size(16, 16).unwrap()).unwrap();
    assert!(
        other
            .queue_texture_readback(
                source,
                crate::renderer::texture_readback::TextureReadbackRegion::pixel(0, 0)
            )
            .is_err()
    );
    let mut forged = source;
    forged.generation += 1;
    assert!(
        renderer
            .queue_texture_readback(
                forged,
                crate::renderer::texture_readback::TextureReadbackRegion::pixel(0, 0)
            )
            .is_err()
    );
    let FrameAcquisition::Ready(aborted) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&aborted, &mut graph, |_| {}).unwrap();
    assert_eq!(renderer.graph_texture_source(ids), Some(source));
    renderer.abort(aborted).unwrap();
    renderer.resize(80, 80).unwrap();
    graph
        .recreate_transient_textures(&mut renderer, 80, 80)
        .unwrap();
    assert_eq!(
        super::test_support::readback(&mut renderer, right).single_u32(),
        Some(9)
    );
    let FrameAcquisition::Ready(recycled) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    renderer.render(&recycled, &mut graph, |_| {}).unwrap();
    renderer.present(recycled).unwrap();
    let latest = renderer.graph_texture_source(ids).unwrap();
    assert_ne!(latest.id, source.id);
    assert!(
        renderer
            .queue_texture_readback(
                source,
                crate::renderer::texture_readback::TextureReadbackRegion::pixel(0, 0)
            )
            .is_err()
    );
    let empty = renderer
        .queue_texture_readback(
            latest,
            crate::renderer::texture_readback::TextureReadbackRegion::pixel(32, 32),
        )
        .unwrap();
    assert_eq!(
        super::test_support::readback(&mut renderer, empty).single_u32(),
        Some(0)
    );
}

#[test]
fn test_native_colorless_pipeline_warmup_resolves_auto_before_depth_encoding() {
    use crate::pipeline::{CompareOp, CullMode};
    use crate::renderer::pipeline_descriptor::{DepthState, PipelineDescriptor, PipelineStages};
    let mut renderer = renderer();
    let mut descriptor = PipelineDescriptor::pbr("")
        .with_vertex_layout(crate::vertex::VertexLayout::empty())
        .with_color_attachment(false)
        .with_depth_format(Some(ImageFormat::D32Sfloat))
        .with_depth(DepthState {
            test: true,
            write: true,
            compare: CompareOp::LessOrEqual,
        })
        .with_cull(CullMode::None);
    descriptor.stages = PipelineStages::Graphics {
        vertex_entry: "vs_main".into(),
        fragment_entry: None,
    };
    let material = super::test_support::material(
        &mut renderer,
        super::test_support::FULLSCREEN_VERTEX,
        descriptor,
    );
    assert_eq!(renderer.materials.get(material).unwrap().variants.len(), 1);
    for format in [
        ImageFormat::Auto,
        ImageFormat::B8G8R8A8Srgb,
        ImageFormat::R16G16B16A16Sfloat,
        ImageFormat::R32Uint,
    ] {
        renderer
            .ensure_material_variant_impl(material, format)
            .unwrap();
        renderer.material_pipeline(material, format).unwrap();
    }
    let mut depth = color("depth", ImageFormat::D32Sfloat);
    depth.resource_type = GraphResourceType::DepthAttachment {
        clear_value: 1.,
        sampled: false,
    };
    let mut graph = FrameGraphBuilder::new()
        .create_resource(depth)
        .export_resource("depth")
        .add_pass(
            SimplePass::new("depth only", PassType::Graphics)
                .depth_ops(
                    AttachmentOps::clear(ClearValue::DepthStencil {
                        depth: 1.,
                        stencil: 0,
                    }),
                    AttachmentOps::dont_care(),
                )
                .depth_target("depth"),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph
        .set_pass_bindings(
            graph.pass_id("depth only").unwrap(),
            super::test_support::vertices(material, crate::vertex::VertexLayout::empty(), 3),
        )
        .unwrap();
    let frame = super::test_support::acquire(&mut renderer, 16);
    renderer.render(&frame, &mut graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    renderer.wait_for_last_submission().unwrap();
    assert_eq!(renderer.materials.get(material).unwrap().variants.len(), 1);
}
