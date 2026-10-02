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
    renderer.light_culling =
        Some(super::light_culling::MetalLightCulling::new(&renderer.context, 32, 32).unwrap());
    let mut graph = FrameGraphBuilder::new()
        .create_buffer(GraphBufferDesc::new(
            "output",
            BufferDesc::new(4, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible),
        ))
        .export_resource("output")
        .build::<MetalRenderer>()
        .unwrap();
    let input = graph.import_builtin_buffer(
        "oversized light frame",
        BuiltinBuffer::LightFrame,
        BufferDesc::new(256, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible),
    );
    let output = graph.resource_id("output").unwrap();
    let command=ComputeDispatch {
        kernel: ComputeKernel::Shader(ComputePipelineDesc {wgsl:"@group(0) @binding(0) var<storage,read> input:array<u32>; @group(0) @binding(1) var<storage,read_write> output:array<u32>; @compute @workgroup_size(1) fn cs_main(){output[0]=input[0];}".into(),entry:"cs_main".into()}),
        bindings:vec![ComputeBinding{group:0,binding:0,resource:input,range:BufferByteRange::new(0,200)},ComputeBinding{group:0,binding:1,resource:output,range:BufferByteRange::new(0,4)}],
        constants:Vec::new(),size:ComputeDispatchSize::Direct([1,1,1]),
    };
    graph.add_pass(
        PassDesc::new(
            "invalid live range",
            PassType::Compute,
            Vec::new(),
            Vec::new(),
        )
        .with_buffer_accesses(command.accesses().unwrap())
        .with_commands([ComputeCommand::Dispatch(command)]),
    );
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
    renderer.present(recovered).unwrap();
}
