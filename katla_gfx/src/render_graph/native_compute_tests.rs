//! Native output and queued-slot checks for application-owned compute buffers.

mod animation;
mod material_reloads;
mod materials;
#[cfg(target_os = "macos")]
use crate::backend::resource::GpuBuffer;
use crate::particles::types::EmitterConfig;
use crate::render_graph::*;
use crate::renderer::frame_scope::FrameAcquisition;
#[cfg(target_os = "macos")]
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use crate::{GpuRenderer, ValidationMode};
use std::ffi::CString;

#[cfg(target_os = "macos")]
type NativeRenderer = crate::MetalRenderer;
#[cfg(not(target_os = "macos"))]
type NativeRenderer = crate::VulkanRenderer;
#[cfg(target_os = "macos")]
const FRAME_SLOTS: usize = 3;
#[cfg(not(target_os = "macos"))]
const FRAME_SLOTS: usize = crate::renderer::FRAMES_IN_FLIGHT;

fn renderer() -> NativeRenderer {
    NativeRenderer::init_headless(
        16,
        16,
        ValidationMode::Enabled,
        CString::new("builtin compute contract").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

pub(super) fn capture_validation_errors(
    renderer: &NativeRenderer,
) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    #[cfg(not(target_os = "macos"))]
    {
        assert!(
            renderer.context().validation_active(),
            "native Vulkan tests require active Khronos validation"
        );
        let captured = errors.clone();
        renderer
            .context()
            .set_validation_callback(move |message, level| {
                if level == crate::ValidationLevel::Error {
                    captured.lock().unwrap().push(message.to_owned());
                }
            });
    }
    #[cfg(target_os = "macos")]
    let _ = renderer;
    errors
}

fn acquire(renderer: &mut NativeRenderer) -> crate::renderer::frame_scope::FrameToken {
    #[cfg(target_os = "macos")]
    {
        let desc = TextureDescriptor::new(16, 16, ImageFormat::B8G8R8A8Srgb)
            .with_usage(TextureUsage::COLOR_ATTACHMENT);
        let (_, view) = renderer.context.create_texture_shared(&desc).unwrap();
        renderer.set_headless_drawable(view.inner);
    }
    let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
        panic!("headless frame")
    };
    frame
}

fn import_data(
    renderer: &mut NativeRenderer,
    graph: &mut FrameGraph<NativeRenderer>,
    name: &str,
    data: &[u8],
    usages: BufferUsages,
    memory: BufferMemoryPolicy,
) -> (crate::handle::BufferHandle, ResourceId) {
    let desc = BufferDesc::new(data.len() as u64, usages, memory);
    let handle = renderer.create_buffer_with_data(desc, data).unwrap();
    let id = graph.import_buffer(name, handle, desc).unwrap();
    (handle, id)
}

fn shader(source: &str) -> ComputePipelineDesc {
    ComputePipelineDesc {
        wgsl: source.replace(
            "#include \"common.wgsl\"",
            include_str!("../../../resources/shaders/particles/common.wgsl"),
        ),
        entry: "cs_main".into(),
    }
}

fn dispatch(
    graph: &mut FrameGraph<NativeRenderer>,
    name: &str,
    source: &str,
    bindings: &[(u32, u32, ResourceId)],
) {
    let command = ComputeDispatch {
        pipeline: shader(source),
        bindings: bindings
            .iter()
            .map(|&(group, binding, resource)| ComputeBinding {
                group,
                binding,
                resource,
                range: BufferByteRange::WHOLE,
            })
            .collect(),
        constants: Vec::new(),
        size: ComputeDispatchSize::Direct([1, 1, 1]),
    };
    graph
        .add_pass(
            PassDesc::new(name, PassType::Compute, Vec::new(), Vec::new())
                .with_buffer_accesses(command.accesses().unwrap())
                .with_commands([ComputeCommand::Dispatch(command)]),
        )
        .unwrap();
}

fn readback_graph() -> FrameGraph<NativeRenderer> {
    FrameGraphBuilder::new()
        .create_buffer(GraphBufferDesc::new(
            "readback",
            BufferDesc::new(
                4160,
                BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
                BufferMemoryPolicy::Readback,
            ),
        ))
        .export_resource("readback")
        .build::<NativeRenderer>()
        .unwrap()
}

fn prepare_copies(
    renderer: &mut NativeRenderer,
    graph: &mut FrameGraph<NativeRenderer>,
    copies: &[(ResourceId, u64, u64)],
) {
    let readback = graph.resource_id("readback").unwrap();
    let mut accesses: Vec<_> = copies
        .iter()
        .map(|&(source, _, _)| BufferAccess::transfer_read(source))
        .collect();
    accesses.push(BufferAccess::transfer_write(readback));
    graph
        .add_pass(
            PassDesc::new(
                "builtin output copies",
                PassType::Transfer,
                Vec::new(),
                Vec::new(),
            )
            .with_buffer_accesses(accesses)
            .with_commands(copies.iter().map(|&(source, destination_offset, size)| {
                ComputeCommand::CopyBuffer {
                    source,
                    destination: readback,
                    source_offset: 0,
                    destination_offset,
                    size,
                }
            })),
        )
        .unwrap();
    let mut host = PassDesc::new(
        "builtin host visibility",
        PassType::Transfer,
        Vec::new(),
        Vec::new(),
    )
    .with_buffer_accesses([BufferAccess::readback_read(readback)])
    .with_commands([]);
    host.side_effect = true;
    graph.add_pass(host).unwrap();
    graph.compile().unwrap();
    graph.initialize_transient_buffers(renderer).unwrap();
    graph.initialize_compute_pipelines(renderer).unwrap();
}

fn read_completed(
    renderer: &mut NativeRenderer,
    graph: &FrameGraph<NativeRenderer>,
    slot: usize,
) -> Vec<u8> {
    #[cfg(target_os = "macos")]
    {
        renderer.wait_for_slot(slot).unwrap();
        let buffer = &graph.transient_buffer("readback", slot).unwrap().buffer;
        let bytes = unsafe { std::slice::from_raw_parts(buffer.map(), 4160) }.to_vec();
        buffer.unmap();
        bytes
    }
    #[cfg(not(target_os = "macos"))]
    {
        renderer.wait_for_device();
        graph
            .transient_buffer("readback", slot)
            .unwrap()
            .read_completed()
            .unwrap()
    }
}

#[test]
fn test_native_graph_particle_emit_simulate_and_indirect_draw_outputs() {
    use crate::particles::{FrameData, ParticleCounters};
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    let config = EmitterConfig {
        position: [2.0, 3.0, 4.0],
        emit_rate: 0.0,
        base_lifetime: 5.0,
        lifetime_variation: 0.0,
        velocity_direction: [0.0, 1.0, 0.0],
        velocity_magnitude: 2.0,
        velocity_cone_angle: 0.0,
        scale_variation: 0.0,
        color_variation: 0.0,
        gravity: 0.0,
        ..Default::default()
    };
    let mut graph = readback_graph();
    let storage = BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE;
    let gpu = BufferMemoryPolicy::DeviceLocal;
    let particle = import_data(
        &mut renderer,
        &mut graph,
        "particles",
        &[0; 4096],
        storage,
        gpu,
    )
    .1;
    let dead = import_data(
        &mut renderer,
        &mut graph,
        "dead list",
        bytemuck::cast_slice(&(0..64u32).collect::<Vec<_>>()),
        storage,
        gpu,
    )
    .1;
    let alive_read = import_data(
        &mut renderer,
        &mut graph,
        "alive input",
        &[0; 256],
        storage,
        gpu,
    )
    .1;
    let alive_write = import_data(
        &mut renderer,
        &mut graph,
        "alive output",
        &[0; 256],
        storage,
        gpu,
    )
    .1;
    let counters = import_data(
        &mut renderer,
        &mut graph,
        "counters",
        bytemuck::bytes_of(&ParticleCounters {
            alive_count: 0,
            dead_count: 64,
            emit_count: 0,
            workgroups_finished: 0,
        }),
        storage,
        gpu,
    )
    .1;
    let indirect = import_data(
        &mut renderer,
        &mut graph,
        "indirect",
        &[0; 16],
        storage | BufferUsages::INDIRECT,
        gpu,
    )
    .1;
    let frame_data = import_data(
        &mut renderer,
        &mut graph,
        "particle frame",
        bytemuck::bytes_of(&FrameData {
            delta_time: 0.25,
            total_emit_count: 4,
            emitter_count: 1,
            random_seed: 17,
            total_simulate_count: 4,
            burst_count: 4,
            frame_index: 0,
            max_particles: 64,
        }),
        BufferUsages::UNIFORM,
        gpu,
    )
    .1;
    let emitters = import_data(
        &mut renderer,
        &mut graph,
        "emitters",
        &config.gpu_bytes(),
        storage,
        gpu,
    )
    .1;
    let bindings = [
        (0, 0, particle),
        (0, 1, dead),
        (0, 2, alive_read),
        (0, 3, alive_write),
        (0, 4, counters),
        (1, 0, frame_data),
        (1, 1, emitters),
    ];
    let emitter_indices = import_data(
        &mut renderer,
        &mut graph,
        "active emitters",
        bytemuck::cast_slice(&[0u32]),
        storage,
        gpu,
    )
    .1;
    let mut emit_bindings = bindings.to_vec();
    emit_bindings.push((1, 2, emitter_indices));
    dispatch(
        &mut graph,
        "particle emit",
        include_str!("../../../resources/shaders/particles/particle_emit.wgsl"),
        &emit_bindings,
    );
    dispatch(
        &mut graph,
        "particle simulate",
        include_str!("../../../resources/shaders/particles/particle_simulate.wgsl"),
        &bindings,
    );
    dispatch(
        &mut graph,
        "particle draw command",
        include_str!("../../../resources/shaders/particles/particle_draw_command.wgsl"),
        &[(0, 0, counters), (0, 1, indirect)],
    );
    prepare_copies(
        &mut renderer,
        &mut graph,
        &[
            (indirect, 0, 16),
            (counters, 16, 16),
            (alive_write, 32, 16),
            (particle, 64, 4096),
        ],
    );
    let frame = acquire(&mut renderer);
    renderer.render(&frame, &mut graph, |_| {}).unwrap();
    renderer.present(frame).unwrap();
    let bytes = read_completed(&mut renderer, &graph, frame.slot());
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    assert_eq!((word(0), word(4), word(8), word(12)), (24, 1, 0, 0));
    assert_eq!((word(16), word(20), word(24)), (4, 60, 4));
    let mut alive = Vec::new();
    for offset in (32..48).step_by(4) {
        let index = word(offset) as usize;
        assert!(index < 64);
        assert!(!alive.contains(&index));
        alive.push(index);
        let base = 64 + index * 64;
        let float = |offset| {
            f32::from_le_bytes(bytes[base + offset..base + offset + 4].try_into().unwrap())
        };
        assert!((float(0) - 2.0).abs() < 1e-5);
        assert!((1.0..=3.0).contains(&float(20)));
        assert!((float(4) - (3.0 + float(20) * 0.25)).abs() < 1e-5);
        assert!((float(8) - 4.0).abs() < 1e-5);
        assert!((float(28) - 4.75).abs() < 1e-5);
    }
    graph.cleanup();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
