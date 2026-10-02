//! Capture observes the ordinary native workload without changing its execution.
use super::{ValidationMode, VulkanRenderer};
use crate::GpuRenderer;
use crate::backend::command::ShaderStages;
use crate::render_graph::*;
use crate::render_pass::{AttachmentOps, ClearValue};
use crate::renderer::frame_bindings::BufferBinding;
use crate::renderer::frame_scope::FrameAcquisition;
use crate::texture::ImageFormat;

#[test]
#[ignore = "requires a Vulkan device"]
fn test_vulkan_native_capture_on_off_preserves_graph_workload_and_submission() {
    let mut measurements = Vec::new();
    let mut plans = Vec::new();
    let mut observed = None;
    for enabled in [false, true] {
        let mut renderer = VulkanRenderer::init_headless(
            16,
            16,
            ValidationMode::Enabled,
            c"capture-proof".into(),
            c"Katla".into(),
        )
        .unwrap();
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
            FULLSCREEN_VERTEX
        );
        let material = material(
            &mut renderer,
            &source,
            fullscreen_descriptor(ImageFormat::B8G8R8A8Srgb),
        );
        let mut graph = FrameGraphBuilder::new()
            .export_resource("backbuffer")
            .build::<VulkanRenderer>()
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
        graph.add_pass(
            PassDesc::new("fill", PassType::Transfer, vec![], vec![])
                .with_buffer_accesses([
                    BufferAccess::transfer_write(value).with_range(BufferByteRange::new(0, 4))
                ])
                .with_commands([fill]),
        );
        let dispatch=ComputeDispatch{pipeline:ComputePipelineDesc{wgsl:"@group(0) @binding(0) var<storage,read_write> value:array<u32>; @compute @workgroup_size(1) fn cs_main(){value[0]+=5u;}".into(),entry:"cs_main".into()},bindings:vec![ComputeBinding{group:0,binding:0,resource:value,range:BufferByteRange::new(0,4)}],constants:vec![],size:ComputeDispatchSize::Direct([1,1,1])};
        graph.add_pass(
            PassDesc::new("compute", PassType::Compute, vec![], vec![])
                .with_buffer_accesses(dispatch.accesses().unwrap())
                .with_commands([ComputeCommand::Dispatch(dispatch)]),
        );
        graph.add_pass(
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
        );
        graph.add_pass(
            PassDesc::new("host", PassType::Transfer, vec![], vec![])
                .with_buffer_accesses([
                    BufferAccess::readback_read(result).with_range(BufferByteRange::new(0, 4))
                ])
                .with_side_effect(),
        );
        let backbuffer = graph.resource_id("backbuffer").unwrap();
        let mut packet = vertices(material, crate::vertex::VertexLayout::new(vec![]), 3);
        packet.buffers.push(BufferBinding {
            group: 2,
            binding: 0,
            resource: value,
            range: BufferByteRange::new(0, 4),
            stages: ShaderStages::FRAGMENT,
        });
        graph.add_pass(
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
        );
        graph.compile().unwrap();
        graph.initialize_compute_pipelines(&mut renderer).unwrap();
        graph.set_execution_trace(enabled);
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless acquisition");
        };
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        measurements.push((
            renderer.graphics_descriptor_sets[frame.slot()].len(),
            renderer.graphics_constants[frame.slot()].len(),
            renderer.pending_graph_buffers.len(),
            renderer.asset_registry.material_variant_count(),
        ));
        plans.push(serde_json::to_value(graph.capture().unwrap().planned_synchronization).unwrap());
        renderer.present(frame).unwrap();
        renderer.wait_for_device();
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
        renderer.wait_for_device();
        assert_eq!(
            renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .unwrap()
                .bytes,
            [0, 0, 255, 255]
        );
        assert_eq!(renderer.swap_data.frame_counter(), 1);
        if enabled {
            let mut capture = graph.capture().unwrap();
            capture.backend_execution.frame = renderer.capture_submission_snapshot();
            observed = Some(capture);
        }
        renderer.destroy();
    }
    assert_eq!(measurements[0], measurements[1]);
    assert_eq!(plans[0], plans[1]);
    let capture = observed.unwrap();
    if !capture.comparison.is_empty() {
        capture
            .write_artifacts(
                std::path::Path::new("target/render-graph-diagnostics"),
                "vulkan_capture_on_off",
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
    assert!(!capture.backend_execution.bindings.is_empty());
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

const FULLSCREEN_VERTEX: &str = "@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let p=array<vec2<f32>,3>(vec2<f32>(-1.,-1.),vec2<f32>(3.,-1.),vec2<f32>(-1.,3.));return vec4<f32>(p[i],0.,1.);}";
fn material(
    renderer: &mut VulkanRenderer,
    source: &str,
    mut descriptor: crate::PipelineDescriptor,
) -> crate::MaterialHandle {
    let path = std::env::temp_dir().join(format!(
        "katla-vulkan-capture-{}.wgsl",
        super::texture_readback::fresh_readback_id()
    ));
    std::fs::write(&path, source).unwrap();
    descriptor.shader_path = path.to_string_lossy().into_owned();
    let result = renderer.compile_material(&descriptor);
    std::fs::remove_file(path).unwrap();
    result.unwrap()
}
fn fullscreen_descriptor(format: ImageFormat) -> crate::PipelineDescriptor {
    crate::PipelineDescriptor::pbr("")
        .with_vertex_layout(crate::vertex::VertexLayout::new(vec![]))
        .with_depth_format(None)
        .with_depth(crate::DepthState::disabled())
        .with_cull(crate::CullMode::None)
        .with_color_format(format)
}
fn vertices(
    material: crate::MaterialHandle,
    vertex_layout: crate::vertex::VertexLayout,
    count: u32,
) -> crate::renderer::frame_bindings::PassBindings {
    use crate::renderer::frame_bindings::*;
    PassBindings {
        phases: vec![PassDrawPhase {
            pipelines: vec![PassPipeline {
                material,
                vertex_layout,
            }],
            constants: Vec::new(),
            viewport: None,
            draw: PassDraw::Vertices {
                count,
                instances: 1,
            },
        }],
        ..Default::default()
    }
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_headless_resize_retires_sources_and_preserves_queued_readback() {
    let mut renderer = VulkanRenderer::init_headless(
        16,
        16,
        ValidationMode::Enabled,
        c"resize-proof".into(),
        c"Katla".into(),
    )
    .unwrap();
    let desc = BufferDesc::new(
        4,
        BufferUsages::STORAGE | BufferUsages::TRANSFER_DESTINATION,
        BufferMemoryPolicy::CpuVisible,
    );
    let buffer = renderer.create_buffer(desc).unwrap();
    let mut graph = FrameGraphBuilder::new()
        .export_resource("backbuffer")
        .build::<VulkanRenderer>()
        .unwrap();
    let value = graph.import_buffer("value", buffer, desc).unwrap();
    graph.add_pass(
        PassDesc::new("fill", PassType::Transfer, vec![], vec![])
            .with_buffer_accesses([
                BufferAccess::transfer_write(value).with_range(BufferByteRange::new(0, 4))
            ])
            .with_commands([ComputeCommand::FillBuffer {
                resource: value,
                range: BufferByteRange::new(0, 4),
                value: 7,
            }])
            .with_side_effect(),
    );
    let backbuffer = graph.resource_id("backbuffer").unwrap();
    let mut clear = PassDesc::new("clear", PassType::Graphics, vec![], vec![backbuffer]);
    clear
        .color_attachments
        .push((backbuffer, AttachmentOps::clear(ClearValue::OPAQUE_BLACK)));
    graph.add_pass(clear);
    graph.compile().unwrap();
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless acquisition");
    };
    renderer
        .write_buffer(&frame, buffer, 0, &3u32.to_ne_bytes())
        .unwrap();
    renderer.render(&frame, &mut graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    let source = renderer.graph_texture_source(backbuffer).unwrap();
    let ticket = renderer
        .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(8, 8))
        .unwrap();
    renderer.resize(32, 24).unwrap();
    assert_eq!(renderer.swapchain_extent(), crate::Size2D::new(32, 24));
    assert!(renderer.graph_texture_source(backbuffer).is_none());
    assert!(
        renderer
            .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(8, 8))
            .is_err()
    );
    assert_eq!(
        renderer
            .poll_texture_readback(ticket)
            .unwrap()
            .unwrap()
            .bytes,
        [0, 0, 0, 255]
    );
    assert!(renderer.capture_submission_snapshot().is_none());
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("resized headless acquisition");
    };
    renderer
        .write_buffer(&frame, buffer, 0, &9u32.to_ne_bytes())
        .unwrap();
    renderer.abort(frame).unwrap();
    renderer.destroy();
}

fn submission_graph(
    buffer: crate::BufferHandle,
    desc: BufferDesc,
    value: u32,
) -> FrameGraph<VulkanRenderer> {
    let mut graph = FrameGraphBuilder::new()
        .export_resource("backbuffer")
        .build()
        .unwrap();
    let resource = graph.import_buffer("counter", buffer, desc).unwrap();
    graph.add_pass(
        PassDesc::new("counter", PassType::Transfer, vec![], vec![])
            .with_buffer_accesses([
                BufferAccess::transfer_write(resource).with_range(BufferByteRange::new(0, 4))
            ])
            .with_commands([ComputeCommand::FillBuffer {
                resource,
                range: BufferByteRange::new(0, 4),
                value,
            }])
            .with_side_effect(),
    );
    graph.add_pass(
        PassDesc::new("host", PassType::Transfer, vec![], vec![])
            .with_buffer_accesses([
                BufferAccess::readback_read(resource).with_range(BufferByteRange::new(0, 4))
            ])
            .with_side_effect(),
    );
    let backbuffer = graph.resource_id("backbuffer").unwrap();
    let mut clear = PassDesc::new("clear", PassType::Graphics, vec![], vec![backbuffer]);
    clear
        .color_attachments
        .push((backbuffer, AttachmentOps::clear(ClearValue::OPAQUE_BLACK)));
    graph.add_pass(clear);
    graph.compile().unwrap();
    graph
}

fn readback_buffer(renderer: &mut VulkanRenderer) -> (crate::BufferHandle, BufferDesc) {
    let desc = BufferDesc::new(
        4,
        BufferUsages::TRANSFER_DESTINATION | BufferUsages::READBACK,
        BufferMemoryPolicy::Readback,
    );
    (renderer.create_buffer(desc).unwrap(), desc)
}

fn submission_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        8,
        8,
        ValidationMode::Enabled,
        c"submission-proof".into(),
        c"Katla".into(),
    )
    .unwrap()
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_submitted_frame_commits_resources_before_surface_outcome() {
    use crate::renderer::frame_scope::SurfaceStatus;
    for surface_result in [
        Ok(true),
        Err(ash::vk::Result::ERROR_OUT_OF_DATE_KHR),
        Err(ash::vk::Result::ERROR_DEVICE_LOST),
    ] {
        let mut renderer = submission_renderer();
        let (buffer, desc) = readback_buffer(&mut renderer);
        let mut graph = submission_graph(buffer, desc, 7);
        let resource = graph.resource_id("backbuffer").unwrap();
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless acquisition")
        };
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        let outcome = renderer
            .present_frame_injected(frame, None, Some(surface_result))
            .unwrap();
        if surface_result == Err(ash::vk::Result::ERROR_DEVICE_LOST) {
            assert!(matches!(
                outcome.surface,
                Err(crate::RendererError::VulkanError(
                    _,
                    ash::vk::Result::ERROR_DEVICE_LOST
                ))
            ));
        } else {
            assert_eq!(outcome.surface.unwrap(), SurfaceStatus::RecreateRequired);
        }
        assert_eq!(renderer.swap_data.frame_counter(), 1);
        assert_eq!(
            renderer
                .read_buffer_completed(buffer, BufferByteRange::new(0, 4))
                .unwrap()
                .unwrap(),
            7u32.to_ne_bytes()
        );
        let source = renderer.graph_texture_source(resource).unwrap();
        assert_eq!(source.frame_slot, frame.slot());
        assert_eq!(source.submission, 1);
        let ticket = renderer
            .queue_texture_readback(source, crate::TextureReadbackRegion::pixel(4, 4))
            .unwrap();
        renderer.resize(16, 8).unwrap();
        assert_eq!(
            renderer
                .poll_texture_readback(ticket)
                .unwrap()
                .unwrap()
                .bytes,
            [0, 0, 0, 255]
        );
        renderer.destroy();
    }
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_rejected_submission_preserves_previous_commit_and_restores_frame_slot() {
    let mut renderer = submission_renderer();
    let (buffer, desc) = readback_buffer(&mut renderer);
    let mut first = submission_graph(buffer, desc, 7);
    let resource = first.resource_id("backbuffer").unwrap();
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless acquisition")
    };
    renderer.render(&frame, &mut first, |_| {}).unwrap();
    renderer.present(frame).unwrap().surface.unwrap();
    let source = renderer.graph_texture_source(resource).unwrap();
    let mut next = submission_graph(buffer, desc, 9);
    let FrameAcquisition::Ready(rejected) = renderer.acquire_frame().unwrap() else {
        panic!("headless acquisition")
    };
    renderer.render(&rejected, &mut next, |_| {}).unwrap();
    let error = renderer
        .present_frame_injected(
            rejected,
            Some(ash::vk::Result::ERROR_OUT_OF_HOST_MEMORY),
            None,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        crate::RendererError::VulkanError(_, ash::vk::Result::ERROR_OUT_OF_HOST_MEMORY)
    ));
    assert_eq!(renderer.swap_data.frame_counter(), 1);
    assert_eq!(renderer.graph_texture_source(resource), Some(source));
    assert_eq!(
        renderer
            .read_buffer_completed(buffer, BufferByteRange::new(0, 4))
            .unwrap()
            .unwrap(),
        7u32.to_ne_bytes()
    );
    let FrameAcquisition::Ready(retry) = renderer.acquire_frame().unwrap() else {
        panic!("retry acquisition")
    };
    assert_eq!(retry.slot(), rejected.slot());
    renderer.render(&retry, &mut next, |_| {}).unwrap();
    renderer.present(retry).unwrap().surface.unwrap();
    assert_eq!(renderer.swap_data.frame_counter(), 2);
    assert_eq!(
        renderer
            .read_buffer_completed(buffer, BufferByteRange::new(0, 4))
            .unwrap()
            .unwrap(),
        9u32.to_ne_bytes()
    );
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_completed_buffer_owner_remains_ready_when_frame_fence_is_reused() {
    use ash::vk::Handle;
    let mut renderer = submission_renderer();
    let (buffer, desc) = readback_buffer(&mut renderer);
    let mut first = submission_graph(buffer, desc, 7);
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless acquisition")
    };
    renderer.render(&frame, &mut first, |_| {}).unwrap();
    renderer.present(frame).unwrap().surface.unwrap();
    let key = renderer
        .graph_buffers
        .get(buffer)
        .unwrap()
        .vk_buffer()
        .as_raw();
    let mut graph = FrameGraphBuilder::new().build::<VulkanRenderer>().unwrap();
    let backbuffer = graph.resource_id("backbuffer").unwrap();
    let mut clear = PassDesc::new("clear", PassType::Graphics, vec![], vec![backbuffer]);
    clear
        .color_attachments
        .push((backbuffer, AttachmentOps::clear(ClearValue::OPAQUE_BLACK)));
    graph.add_pass(clear);
    graph.compile().unwrap();
    for _ in 0..2 {
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless acquisition")
        };
        assert_eq!(renderer.graph_buffer_consumers.get(&key), Some(&None));
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap().surface.unwrap();
        assert_eq!(renderer.graph_buffer_consumers.get(&key), Some(&None));
        assert_eq!(
            renderer
                .read_buffer_completed(buffer, BufferByteRange::new(0, 4))
                .unwrap()
                .unwrap(),
            7u32.to_ne_bytes()
        );
    }
    renderer.destroy();
}
