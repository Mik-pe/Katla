//! Native output checks for the shared compiled compute/transfer contract.

use super::*;
use crate::renderer::frame_scope::FrameAcquisition;
use crate::{GpuRenderer, ValidationMode};
use std::ffi::CString;

#[cfg(target_os = "macos")]
type NativeRenderer = crate::MetalRenderer;
#[cfg(not(target_os = "macos"))]
type NativeRenderer = crate::VulkanRenderer;

fn graph_buffer(name: &str, size: u64, usages: BufferUsages) -> GraphBufferDesc {
    GraphBufferDesc::new(
        name,
        BufferDesc::new(size, usages, BufferMemoryPolicy::DeviceLocal),
    )
}

fn dispatch(
    input: ResourceId,
    output: ResourceId,
    params: ResourceId,
    size: ComputeDispatchSize,
) -> ComputeDispatch {
    let source = r#"
struct Params { multiplier: u32, bias: u32, padding: vec2u }
@group(0) @binding(3) var<storage, read> input: array<u32>;
@group(2) @binding(5) var<storage, read_write> output: array<u32>;
@group(1) @binding(7) var<uniform> params: Params;
@compute @workgroup_size(8)
fn cs_main(@builtin(global_invocation_id) id: vec3u) {
    if id.x < 64u {
        output[id.x] = input[id.x] * params.multiplier + params.bias + id.x;
    }
}
"#;
    ComputeDispatch {
        pipeline: ComputePipelineDesc {
            wgsl: source.into(),
            entry: "cs_main".into(),
        },
        bindings: vec![
            ComputeBinding {
                group: 1,
                binding: 7,
                resource: params,
                range: BufferByteRange::new(0, 16),
            },
            ComputeBinding {
                group: 0,
                binding: 3,
                resource: input,
                range: BufferByteRange::WHOLE,
            },
            ComputeBinding {
                group: 2,
                binding: 5,
                resource: output,
                range: BufferByteRange::WHOLE,
            },
        ],
        constants: [3u32, 11, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
        size,
    }
}

fn add_dispatch(graph: &mut FrameGraph<NativeRenderer>, name: &str, dispatch: ComputeDispatch) {
    let mut accesses = dispatch.accesses().unwrap();
    let uniform = dispatch
        .bindings
        .iter()
        .find(|binding| binding.group == 1)
        .unwrap();
    accesses.push(BufferAccess::transfer_write(uniform.resource).with_range(uniform.range));
    if let ComputeDispatchSize::Indirect { resource, offset } = dispatch.size {
        accesses.push(
            BufferAccess::indirect_read(resource).with_range(BufferByteRange::new(offset, 12)),
        );
    }
    graph
        .add_pass(
            PassDesc::new(name, PassType::Compute, vec![], vec![])
                .with_buffer_accesses(accesses)
                .with_commands([ComputeCommand::Dispatch(dispatch)]),
        )
        .unwrap();
}

#[cfg(target_os = "macos")]
fn prepare_drawable(renderer: &mut NativeRenderer) {
    let desc = crate::TextureDescriptor::new(16, 16, crate::texture::ImageFormat::B8G8R8A8Srgb)
        .with_usage(crate::texture::TextureUsage::COLOR_ATTACHMENT);
    let (_, view) = renderer.context.create_texture_shared(&desc).unwrap();
    renderer.set_headless_drawable(view.inner);
}
#[cfg(not(target_os = "macos"))]
fn prepare_drawable(_: &mut NativeRenderer) {}

#[cfg(target_os = "macos")]
fn wait_and_read(
    renderer: &mut NativeRenderer,
    graph: &FrameGraph<NativeRenderer>,
    slot: usize,
) -> Vec<u8> {
    use crate::backend::resource::GpuBuffer;
    renderer.wait_for_slot(slot).unwrap();
    let buffer = &graph.transient_buffer("readback", slot).unwrap().buffer;
    // SAFETY: The exact owning submission completed and the readback allocation is shared.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.map(), 768) }.to_vec();
    buffer.unmap();
    bytes
}
#[cfg(not(target_os = "macos"))]
fn wait_and_read(
    renderer: &mut NativeRenderer,
    graph: &FrameGraph<NativeRenderer>,
    slot: usize,
) -> Vec<u8> {
    renderer.wait_for_device();
    graph
        .transient_buffer("readback", slot)
        .unwrap()
        .read_completed()
        .unwrap()
}

#[test]
fn test_native_compiled_custom_compute_direct_and_indirect_outputs() {
    let mut renderer = NativeRenderer::init_headless(
        16,
        16,
        ValidationMode::Enabled,
        CString::new("compute contract").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap();
    let errors = super::native_compute_tests::capture_validation_errors(&renderer);
    let storage =
        BufferUsages::STORAGE | BufferUsages::TRANSFER_DESTINATION | BufferUsages::TRANSFER_SOURCE;
    let mut graph = FrameGraphBuilder::new()
        .create_buffer(graph_buffer("input", 256, storage))
        .create_buffer(graph_buffer(
            "params",
            16,
            BufferUsages::UNIFORM | BufferUsages::TRANSFER_DESTINATION,
        ))
        .create_buffer(graph_buffer("direct", 256, storage))
        .create_buffer(graph_buffer("indirect", 256, storage))
        .create_buffer(graph_buffer("chained", 256, storage))
        .create_buffer(graph_buffer(
            "arguments",
            12,
            BufferUsages::INDIRECT | BufferUsages::TRANSFER_DESTINATION,
        ))
        .create_buffer(GraphBufferDesc::new(
            "readback",
            BufferDesc::new(
                768,
                BufferUsages::TRANSFER_DESTINATION | BufferUsages::READBACK,
                BufferMemoryPolicy::Readback,
            ),
        ))
        .export_resource("readback")
        .build::<NativeRenderer>()
        .unwrap();
    let input = graph.resource_id("input").unwrap();
    let params = graph.resource_id("params").unwrap();
    let direct = graph.resource_id("direct").unwrap();
    let indirect = graph.resource_id("indirect").unwrap();
    let chained = graph.resource_id("chained").unwrap();
    let arguments = graph.resource_id("arguments").unwrap();
    let readback = graph.resource_id("readback").unwrap();
    graph
        .add_pass(
            PassDesc::new("initialize", PassType::Transfer, vec![], vec![])
                .with_buffer_accesses(
                    [input, direct, indirect, chained, arguments].map(BufferAccess::transfer_write),
                )
                .with_commands([
                    ComputeCommand::FillBuffer {
                        resource: input,
                        range: BufferByteRange::WHOLE,
                        value: 7,
                    },
                    ComputeCommand::FillBuffer {
                        resource: direct,
                        range: BufferByteRange::WHOLE,
                        value: 999,
                    },
                    ComputeCommand::FillBuffer {
                        resource: indirect,
                        range: BufferByteRange::WHOLE,
                        value: 999,
                    },
                    ComputeCommand::FillBuffer {
                        resource: chained,
                        range: BufferByteRange::WHOLE,
                        value: 999,
                    },
                    ComputeCommand::FillBuffer {
                        resource: arguments,
                        range: BufferByteRange::WHOLE,
                        value: 1,
                    },
                    ComputeCommand::FillBuffer {
                        resource: arguments,
                        range: BufferByteRange::new(0, 4),
                        value: 4,
                    },
                ]),
        )
        .unwrap();
    let direct_command = dispatch(
        input,
        direct,
        params,
        ComputeDispatchSize::Direct([8, 1, 1]),
    );
    let mut chained_command = dispatch(
        direct,
        chained,
        params,
        ComputeDispatchSize::Direct([8, 1, 1]),
    );
    chained_command.constants = [2u32, 5, 0, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    graph
        .add_pass(
            PassDesc::new(
                "same pass chained dispatches",
                PassType::Compute,
                vec![],
                vec![],
            )
            .with_buffer_accesses([
                BufferAccess::storage_read(input),
                BufferAccess::storage_read_write(direct),
                BufferAccess::storage_read_write(chained),
                BufferAccess::uniform_read(params).with_range(BufferByteRange::new(0, 16)),
                BufferAccess::transfer_write(params).with_range(BufferByteRange::new(0, 16)),
            ])
            .with_commands([
                ComputeCommand::Dispatch(direct_command),
                ComputeCommand::Dispatch(chained_command),
            ]),
        )
        .unwrap();
    add_dispatch(
        &mut graph,
        "indirect dispatch",
        dispatch(
            input,
            indirect,
            params,
            ComputeDispatchSize::Indirect {
                resource: arguments,
                offset: 0,
            },
        ),
    );
    graph
        .add_pass(
            PassDesc::new("copy results", PassType::Transfer, vec![], vec![])
                .with_buffer_accesses([
                    BufferAccess::transfer_read(direct),
                    BufferAccess::transfer_read(indirect),
                    BufferAccess::transfer_read(chained),
                    BufferAccess::transfer_write(readback),
                ])
                .with_commands([
                    ComputeCommand::CopyBuffer {
                        source: direct,
                        destination: readback,
                        source_offset: 0,
                        destination_offset: 0,
                        size: 256,
                    },
                    ComputeCommand::CopyBuffer {
                        source: indirect,
                        destination: readback,
                        source_offset: 0,
                        destination_offset: 256,
                        size: 256,
                    },
                    ComputeCommand::CopyBuffer {
                        source: chained,
                        destination: readback,
                        source_offset: 0,
                        destination_offset: 512,
                        size: 256,
                    },
                ]),
        )
        .unwrap();
    let mut host = PassDesc::new("host visibility", PassType::Transfer, vec![], vec![])
        .with_buffer_accesses([BufferAccess::readback_read(readback)])
        .with_commands([]);
    host.side_effect = true;
    graph.add_pass(host).unwrap();
    graph.compile().unwrap();
    graph.initialize_transient_buffers(&renderer).unwrap();
    graph.initialize_compute_pipelines(&mut renderer).unwrap();
    prepare_drawable(&mut renderer);
    let FrameAcquisition::Ready(aborted) = renderer.acquire_frame().unwrap() else {
        panic!("headless abort frame")
    };
    let before_abort = wait_and_read(&mut renderer, &graph, aborted.slot());
    let mut callback_ran = false;
    renderer
        .render(&aborted, &mut graph, |_| callback_ran = true)
        .unwrap();
    assert!(callback_ran);
    renderer.abort(aborted).unwrap();
    assert_eq!(
        wait_and_read(&mut renderer, &graph, aborted.slot()),
        before_abort,
        "aborted encoded dispatch/copy must never reach the GPU"
    );
    let host_write = renderer
        .create_buffer(BufferDesc::new(
            4,
            BufferUsages::UNIFORM,
            BufferMemoryPolicy::CpuVisible,
        ))
        .unwrap();
    for _ in 0..3 {
        prepare_drawable(&mut renderer);
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless compute frame")
        };
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        assert!(
            renderer
                .render(&frame, &mut graph, |_| panic!("duplicate render callback"))
                .is_err()
        );
        for result in [
            renderer.write_buffer(&frame, host_write, 0, &0u32.to_le_bytes()),
            GpuRenderer::execute_draw_calls(&mut renderer, &frame, &crate::DrawList::new()),
        ] {
            let error = result.unwrap_err();
            assert!(
                matches!(error, crate::RendererError::InvalidOperation(_)),
                "{error:?}"
            );
            assert!(
                error
                    .to_string()
                    .contains("Frame-local writes must precede render"),
                "{error}"
            );
        }
        renderer.present(frame).unwrap();
        let bytes = wait_and_read(&mut renderer, &graph, frame.slot());
        let values: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        assert_eq!(&values[..64], &(32..96).collect::<Vec<u32>>());
        assert_eq!(&values[64..96], &(32..64).collect::<Vec<u32>>());
        assert_eq!(&values[96..128], &[999; 32]);
        assert_eq!(
            &values[128..],
            &(0..64u32).map(|index| 69 + 3 * index).collect::<Vec<_>>()
        );
    }
    graph.cleanup();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
