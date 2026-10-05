//! Native allocation-range rejection before command submission.
use super::context::MetalContext;
use super::metal_renderer::MetalRenderer;
use crate::GpuRenderer;
use crate::render_graph::*;
use crate::renderer::frame_scope::FrameAcquisition;
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
fn renderer() -> MetalRenderer {
    MetalRenderer::new(MetalContext::init_headless_with_size(16, 16).unwrap()).unwrap()
}
fn acquire(renderer: &mut MetalRenderer) -> crate::renderer::frame_scope::FrameToken {
    let desc = TextureDescriptor::new(16, 16, ImageFormat::B8G8R8A8Srgb)
        .with_usage(TextureUsage::COLOR_ATTACHMENT);
    let (_, view) = renderer.context.create_texture_shared(&desc).unwrap();
    renderer.set_headless_drawable(view.inner);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    frame
}
#[test]
fn test_native_compute_rejects_range_beyond_renderer_owned_allocation() {
    let mut renderer = renderer();
    let input_desc = BufferDesc::new(256, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible);
    let handle = renderer.create_buffer(input_desc).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .create_buffer(GraphBufferDesc::new(
            "output",
            BufferDesc::new(4, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible),
        ))
        .export_resource("output")
        .build::<MetalRenderer>()
        .unwrap();
    let input = graph
        .import_buffer("oversized native allocation", handle, input_desc)
        .unwrap();
    renderer.graph_buffers.get_mut(handle).unwrap().buffer =
        renderer.context.create_buffer(160, true).unwrap();
    let output = graph.resource_id("output").unwrap();
    let command=ComputeDispatch {
        pipeline: ComputePipelineDesc {wgsl:"@group(0) @binding(0) var<storage,read> input:array<u32>; @group(0) @binding(1) var<storage,read_write> output:array<u32>; @compute @workgroup_size(1) fn cs_main(){output[0]=input[0];}".into(),entry:"cs_main".into()},
        bindings:vec![ComputeBinding{group:0,binding:0,resource:input,range:BufferByteRange::new(0,200)},ComputeBinding{group:0,binding:1,resource:output,range:BufferByteRange::new(0,4)}],
        constants:Vec::new(),size:ComputeDispatchSize::Direct([1,1,1]),
    };
    graph
        .add_pass(
            PassDesc::new(
                "invalid live range",
                PassType::Compute,
                Vec::new(),
                Vec::new(),
            )
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command)]),
        )
        .unwrap();
    graph.compile().unwrap();
    graph.initialize_transient_buffers(&renderer).unwrap();
    graph.initialize_compute_pipelines(&mut renderer).unwrap();
    let frame = acquire(&mut renderer);
    let error = renderer.render(&frame, &mut graph, |_| {}).unwrap_err();
    assert!(
        matches!(error, crate::error::RendererError::InvalidOperation(_)),
        "{error:?}"
    );
    assert!(error.to_string().contains("range"), "{error}");
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert!(renderer.present(frame).is_err());
    let recovered = acquire(&mut renderer);
    renderer.abort(recovered).unwrap();
}

#[test]
fn test_native_unprepared_compute_abort_releases_recording_before_slot_reuse() {
    use crate::render_pass::{AttachmentOps, ClearValue};
    let mut renderer = renderer();
    let graph = |color| {
        FrameGraphBuilder::new()
            .export_resource("backbuffer")
            .add_pass(
                SimplePass::new("clear", PassType::Graphics)
                    .without_depth()
                    .write("backbuffer")
                    .attachment("backbuffer", AttachmentOps::clear(color)),
            )
            .build::<MetalRenderer>()
            .unwrap()
    };
    let mut failed = graph(ClearValue::color(1., 0., 0., 1.));
    let dispatch = ComputeDispatch {
        pipeline: ComputePipelineDesc {
            wgsl: "@compute @workgroup_size(1) fn cs_main(){}".into(),
            entry: "cs_main".into(),
        },
        bindings: vec![],
        constants: vec![],
        size: ComputeDispatchSize::Direct([1, 1, 1]),
    };
    failed
        .add_pass(
            PassDesc::new("deliberately unprepared", PassType::Compute, vec![], vec![])
                .with_commands([ComputeCommand::Dispatch(dispatch)])
                .with_side_effect(),
        )
        .unwrap();
    let frame = acquire(&mut renderer);
    let error = renderer.render(&frame, &mut failed, |_| {}).unwrap_err();
    assert!(
        error.to_string().contains("not prepared before encoding"),
        "{error}"
    );
    assert!(renderer.pending_frame.is_none());
    assert!(renderer.frame_slots[frame.slot()].submission.is_none());
    assert!(renderer.last_submission.is_none());
    assert!(
        renderer
            .graph_texture_source(failed.resource_id("backbuffer").unwrap())
            .is_none()
    );
    renderer.abort(frame).unwrap();
    let recovered = acquire(&mut renderer);
    assert_eq!(recovered.slot(), frame.slot());
    let mut good = graph(ClearValue::color(0., 1., 0., 1.));
    renderer.render(&recovered, &mut good, |_| {}).unwrap();
    renderer.present(recovered).unwrap();
    renderer.wait_for_last_submission().unwrap();
    let source = renderer
        .graph_texture_source(good.resource_id("backbuffer").unwrap())
        .unwrap();
    let ticket = renderer
        .queue_texture_readback(
            source,
            crate::renderer::texture_readback::TextureReadbackRegion::pixel(8, 8),
        )
        .unwrap();
    assert_eq!(
        super::test_support::readback(&mut renderer, ticket).bytes,
        [0, 255, 0, 255]
    );
}
