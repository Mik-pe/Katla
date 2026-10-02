//! Capture observes the ordinary native workload without changing its execution.
use super::{context::MetalContext, metal_renderer::MetalRenderer, test_support};
use crate::GpuRenderer;
use crate::backend::command::ShaderStages;
use crate::render_graph::*;
use crate::render_pass::{AttachmentOps, ClearValue};
use crate::renderer::frame_bindings::BufferBinding;
use crate::texture::ImageFormat;

#[test]
fn test_native_capture_on_off_preserves_graph_workload_and_submission() {
    let mut measurements = Vec::new();
    let mut plans = Vec::new();
    let mut observed = None;
    for enabled in [false, true] {
        let mut renderer =
            MetalRenderer::new(MetalContext::init_headless_with_size(16, 16).unwrap()).unwrap();
        let desc = BufferDesc::new(
            4,
            BufferUsages::STORAGE
                | BufferUsages::TRANSFER_DESTINATION
                | BufferUsages::TRANSFER_SOURCE,
            BufferMemoryPolicy::DeviceLocal,
        );
        let handle = renderer.create_buffer(desc).unwrap();
        let readback_desc = BufferDesc::new(
            4,
            BufferUsages::TRANSFER_DESTINATION | BufferUsages::READBACK,
            BufferMemoryPolicy::Readback,
        );
        let readback = renderer.create_buffer(readback_desc).unwrap();
        let source = format!(
            "{} @group(2) @binding(0) var<storage,read> value:array<u32>; @fragment fn fs_main()->@location(0) vec4<f32>{{return vec4<f32>(f32(value[0])/12.,0.,0.,1.);}}",
            test_support::FULLSCREEN_VERTEX
        );
        let material = test_support::material(
            &mut renderer,
            &source,
            test_support::fullscreen_descriptor(ImageFormat::B8G8R8A8Srgb),
        );
        let mut graph = FrameGraphBuilder::new()
            .export_resource("backbuffer")
            .build::<MetalRenderer>()
            .unwrap();
        let value = graph.import_buffer("value", handle, desc).unwrap();
        let result = graph
            .import_buffer("readback", readback, readback_desc)
            .unwrap();
        let fill = ComputeCommand::FillBuffer {
            resource: value,
            range: BufferByteRange::new(0, 4),
            value: 7,
        };
        graph
            .add_pass(
                PassDesc::new("fill", PassType::Transfer, vec![], vec![])
                    .with_buffer_accesses([
                        BufferAccess::transfer_write(value).with_range(BufferByteRange::new(0, 4))
                    ])
                    .with_commands([fill]),
            )
            .unwrap();
        let dispatch=ComputeDispatch{pipeline:ComputePipelineDesc{wgsl:"@group(0) @binding(0) var<storage,read_write> value:array<u32>; @compute @workgroup_size(1) fn cs_main(){value[0]+=5u;}".into(),entry:"cs_main".into()},bindings:vec![ComputeBinding{group:0,binding:0,resource:value,range:BufferByteRange::new(0,4)}],constants:vec![],size:ComputeDispatchSize::Direct([1,1,1])};
        graph
            .add_pass(
                PassDesc::new("compute", PassType::Compute, vec![], vec![])
                    .with_buffer_accesses(dispatch.accesses().unwrap())
                    .with_commands([ComputeCommand::Dispatch(dispatch)]),
            )
            .unwrap();
        graph
            .add_pass(
                PassDesc::new("copy", PassType::Transfer, vec![], vec![])
                    .with_buffer_accesses([
                        BufferAccess::transfer_read(value).with_range(BufferByteRange::new(0, 4)),
                        BufferAccess::transfer_write(result).with_range(BufferByteRange::new(0, 4)),
                    ])
                    .with_commands([ComputeCommand::CopyBuffer {
                        source: value,
                        destination: result,
                        source_offset: 0,
                        destination_offset: 0,
                        size: 4,
                    }]),
            )
            .unwrap();
        graph
            .add_pass(
                PassDesc::new("host", PassType::Transfer, vec![], vec![])
                    .with_buffer_accesses([
                        BufferAccess::readback_read(result).with_range(BufferByteRange::new(0, 4))
                    ])
                    .with_side_effect(),
            )
            .unwrap();
        let backbuffer = graph.resource_id("backbuffer").unwrap();
        let mut packet =
            test_support::vertices(material, crate::vertex::VertexLayout::new(vec![]), 3);
        packet.buffers.push(BufferBinding {
            group: 2,
            binding: 0,
            resource: value,
            range: BufferByteRange::new(0, 4),
            stages: ShaderStages::FRAGMENT,
        });
        graph
            .add_pass(
                {
                    let mut pass =
                        PassDesc::new("render", PassType::Graphics, vec![], vec![backbuffer]);
                    pass.kind = Some(PassKind::Fullscreen);
                    pass.color_attachments
                        .push((backbuffer, AttachmentOps::clear(ClearValue::OPAQUE_BLACK)));
                    pass
                }
                .with_buffer_accesses([BufferAccess::storage_read(value)
                    .with_stage(ResourceAccessStage::FragmentShader)
                    .with_range(BufferByteRange::new(0, 4))])
                .with_bindings(packet),
            )
            .unwrap();
        graph.compile().unwrap();
        graph.initialize_compute_pipelines(&mut renderer).unwrap();
        graph.set_execution_trace(enabled);
        let frame = test_support::acquire(&mut renderer, 16);
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        let resources = &renderer.pending_frame.as_ref().unwrap().command.resources;
        let diagnostics = resources.diagnostics();
        measurements.push((
            resources.native_counts(),
            diagnostics.argument_table_count,
            diagnostics.submission.allocation_count,
        ));
        plans.push(serde_json::to_value(graph.capture().unwrap().planned_synchronization).unwrap());
        renderer.present(frame).unwrap();
        renderer.wait_for_last_submission().unwrap();
        assert_eq!(
            renderer
                .read_buffer_completed(readback, BufferByteRange::new(0, 4))
                .unwrap()
                .unwrap(),
            12u32.to_ne_bytes()
        );
        let source = renderer.graph_texture_source(backbuffer).unwrap();
        let ticket = renderer
            .queue_texture_readback(
                source,
                crate::renderer::texture_readback::TextureReadbackRegion::pixel(8, 8),
            )
            .unwrap();
        assert_eq!(
            test_support::readback(&mut renderer, ticket).bytes,
            [0, 0, 255, 255]
        );
        assert_eq!(renderer.last_submission.as_ref().unwrap().1, 1);
        if enabled {
            let mut capture = graph.capture().unwrap();
            capture.backend_execution.frame = renderer.capture_submission_snapshot();
            observed = Some(capture);
        }
    }
    assert_eq!(measurements[0], measurements[1]);
    assert_eq!(plans[0], plans[1]);
    let capture = observed.unwrap();
    if !capture.comparison.is_empty() {
        capture
            .write_artifacts(
                std::path::Path::new("target/render-graph-diagnostics"),
                "metal_capture_on_off",
            )
            .unwrap();
    }
    assert!(capture.comparison.is_empty(), "{:?}", capture.comparison);
    assert_eq!(
        capture
            .backend_execution
            .encoders
            .iter()
            .filter(|encoder| encoder.pass_index.is_some())
            .count(),
        4
    );
    assert!(
        capture
            .backend_execution
            .bindings
            .iter()
            .any(|binding| !binding.residency_members.is_empty())
    );
    assert!(
        capture
            .backend_execution
            .synchronization
            .iter()
            .any(|operation| operation.emitted && !operation.native_scope.is_empty())
    );
    assert!(
        capture
            .executed_passes
            .iter()
            .any(|pass| pass.label == "host" && pass.outcome == "skipped_no_work")
    );
}
