//! GPU regression coverage for the compiled-to-native attachment contract.

use super::attachments::ResolvedMetalAttachments;
use super::command_buffer::MetalCommandBuffer;
use super::context::MetalContext;
use super::execution_plan::MetalExecutionPlan;
use super::metal_renderer::MetalRenderer;
use super::texture::MetalTextureView;
use crate::GpuRenderer;
use crate::backend::resource::GpuBuffer;
use crate::render_graph::{
    FrameGraph, FrameGraphBuilder, GraphResourceDesc, GraphResourceType, PassBuilder, PassKind,
    PassType, SimplePass, TonemapOperator, TonemapParams,
};
use crate::render_pass::{AttachmentOps, ClearValue, StoreOp};
use crate::renderer::frame_scope::FrameAcquisition;
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLLoadAction, MTLOrigin, MTLSize,
    MTLStoreAction,
};
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
    let cmd = renderer.context.create_command_buffer();
    let blit = cmd.inner.blitCommandEncoder().unwrap();
    unsafe {
        blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
            &view.inner, 0, 0, MTLOrigin {x: 8, y: 8, z: 0}, MTLSize {width: 1, height: 1, depth: 1}, &buffer.inner, 0, 4, 4);
    }
    blit.endEncoding();
    cmd.inner.commit();
    cmd.inner.waitUntilCompleted();
    assert_eq!(
        cmd.inner.status(),
        objc2_metal::MTLCommandBufferStatus::Completed
    );
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
    let plan = MetalExecutionPlan::compile(graph, ImageFormat::B8G8R8A8Srgb).unwrap();
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
    let plan = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb).unwrap();
    for (index, record) in plan.passes().iter().enumerate() {
        let resolved = ResolvedMetalAttachments::resolve(record, &graph, &view, 0).unwrap();
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
    let plan = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb).unwrap();
    for (index, record) in plan.passes().iter().enumerate() {
        let resolved =
            ResolvedMetalAttachments::resolve(record, &graph, &drawable(&renderer), 1).unwrap();
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
    renderer
        .init_tonemap_pipeline(std::path::Path::new("tonemapping.wgsl"))
        .unwrap();
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
                .with_kind(PassKind::Fullscreen)
                .tonemap(TonemapParams {
                    exposure: 1.0,
                    gamma: 1.0,
                    mode: TonemapOperator::Linear,
                    hdr_texture_index: None,
                }),
        )
        .add_pass(
            SimplePass::new("tonemap_green", PassType::Graphics)
                .without_depth()
                .read("hdr_green")
                .write("ldr_green")
                .attachment("ldr_green", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .with_kind(PassKind::Fullscreen)
                .tonemap(TonemapParams {
                    exposure: 1.0,
                    gamma: 1.0,
                    mode: TonemapOperator::Linear,
                    hdr_texture_index: None,
                }),
        )
        .build::<MetalRenderer>()
        .unwrap();
    graph.initialize_transient_textures(&renderer).unwrap();
    graph
        .register_transient_texture_bindless(&mut renderer, "hdr_red")
        .unwrap();
    graph
        .register_transient_texture_bindless(&mut renderer, "hdr_green")
        .unwrap();
    renderer.bindless_manager.flush_argument_buffer();
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
    let plan = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb).unwrap();
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
    let plan = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb).unwrap();
    let record = &plan.passes()[0];
    let view = drawable(&renderer);
    assert!(
        ResolvedMetalAttachments::resolve(record, &graph, &view, 0)
            .err()
            .unwrap()
            .to_string()
            .contains("cannot resolve")
    );
    graph.initialize_transient_textures(&renderer).unwrap();
    assert!(
        ResolvedMetalAttachments::resolve(record, &graph, &view, 0)
            .err()
            .unwrap()
            .to_string()
            .contains("extents")
    );
}

#[test]
fn test_depth_without_declared_resource_is_rejected_by_metal_compilation() {
    let graph = FrameGraphBuilder::new()
        .add_side_effect_pass(
            SimplePass::new("implicit_depth", PassType::Graphics).with_kind(PassKind::DepthPrepass),
        )
        .build::<MetalRenderer>()
        .unwrap();
    let error = MetalExecutionPlan::compile(&graph, ImageFormat::B8G8R8A8Srgb)
        .unwrap_err()
        .to_string();
    assert!(error.contains("declared graph depth target"));
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
    let cmd = renderer.context.create_command_buffer();
    let blit = cmd.inner.blitCommandEncoder().unwrap();
    unsafe {
        blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
            &texture.inner, 0, 0, MTLOrigin {x: 8, y: 8, z: 0}, MTLSize {width: 1, height: 1, depth: 1}, &buffer.inner, 0, 8, 8);
    }
    blit.endEncoding();
    cmd.inner.commit();
    cmd.inner.waitUntilCompleted();
    let components = unsafe { std::ptr::read(buffer.map() as *const [u16; 4]) };
    buffer.unmap();
    assert_eq!(components, [0x3c00, 0, 0, 0x3c00]);
}
