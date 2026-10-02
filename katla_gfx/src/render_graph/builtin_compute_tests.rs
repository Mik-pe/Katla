//! GPU output checks for renderer-owned built-in buffers in the compiled graph.

use crate::animation::{AnimChannelInfo, AnimClipHeader, JointInfo, SkeletonAnimParams};
#[cfg(target_os = "macos")]
use crate::backend::resource::GpuBuffer;
use crate::particles::types::EmitterConfig;
use crate::render_graph::*;
use crate::renderer::frame_scope::FrameAcquisition;
#[cfg(target_os = "macos")]
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};
use crate::{AnimationBufferUploader, GpuRenderer, ValidationMode};
use std::collections::HashMap;
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

fn animation_uploader(renderer: &mut NativeRenderer) -> &mut dyn AnimationBufferUploader {
    #[cfg(target_os = "macos")]
    return renderer.animation_uploader_mut().unwrap();
    #[cfg(not(target_os = "macos"))]
    return renderer.animation_buffers.as_mut().unwrap();
}

fn prepare_particles(renderer: &mut NativeRenderer, config: EmitterConfig) {
    #[cfg(target_os = "macos")]
    let mut particles =
        crate::metal::particle::MetalParticleSubsystem::new(&renderer.context, 64).unwrap();
    #[cfg(not(target_os = "macos"))]
    let mut particles = crate::particles::GlobalParticleSystem::new(&renderer.context, 64).unwrap();
    let emitter = particles.create_emitter(config).unwrap();
    particles.burst(emitter, 4).unwrap();
    renderer.particle_system = Some(particles);
}

fn step_particles(renderer: &mut NativeRenderer, slot: usize) {
    #[cfg(target_os = "macos")]
    {
        let _ = slot;
        assert_eq!(renderer.step_particle_system(0.25).unwrap(), (1, 1));
    }
    #[cfg(not(target_os = "macos"))]
    {
        renderer
            .particle_system
            .as_mut()
            .unwrap()
            .update(0.25, slot as u32)
            .unwrap();
    }
}

fn import(
    graph: &mut FrameGraph<NativeRenderer>,
    renderer: &NativeRenderer,
    roles: &[BuiltinBuffer],
) -> HashMap<BuiltinBuffer, ResourceId> {
    roles
        .iter()
        .map(|&role| {
            let buffer = renderer.builtin_buffer(role).unwrap();
            let id = graph.import_builtin_buffer(format!("{role:?}"), role, buffer.desc);
            (role, id)
        })
        .collect()
}

fn dispatch(
    graph: &mut FrameGraph<NativeRenderer>,
    name: &str,
    kernel: BuiltinComputeKernel,
    bindings: &[(u32, u32, BuiltinBuffer)],
    ids: &HashMap<BuiltinBuffer, ResourceId>,
) {
    let command = ComputeDispatch {
        kernel: ComputeKernel::Builtin(kernel),
        bindings: bindings
            .iter()
            .map(|&(group, binding, role)| ComputeBinding {
                group,
                binding,
                resource: ids[&role],
                range: BufferByteRange::WHOLE,
            })
            .collect(),
        constants: Vec::new(),
        size: ComputeDispatchSize::Direct([1, 1, 1]),
    };
    graph.add_pass(
        PassDesc::new(name, PassType::Compute, Vec::new(), Vec::new())
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command)]),
    );
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
    graph.add_pass(
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
    );
    let mut host = PassDesc::new(
        "builtin host visibility",
        PassType::Transfer,
        Vec::new(),
        Vec::new(),
    )
    .with_buffer_accesses([BufferAccess::readback_read(readback)])
    .with_commands([]);
    host.side_effect = true;
    graph.add_pass(host);
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
fn test_native_graph_animation_interpolates_renderer_owned_joint_output() {
    animation_output(false);
}

#[test]
fn test_native_graph_animation_without_channels_preserves_rest_pose() {
    animation_output(true);
}

fn animation_output(rest_pose: bool) {
    use BuiltinBuffer::*;
    let mut renderer = renderer();
    let errors = capture_validation_errors(&renderer);
    renderer
        .init_animation_pipeline(std::path::Path::new("unused"))
        .unwrap();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let channels = [AnimChannelInfo {
        target_joint: 0,
        path_type: 0,
        time_offset: 0,
        value_offset: 0,
        keyframe_count: 2,
        interpolation: 0,
        _pad: [0; 2],
    }];
    animation_uploader(&mut renderer)
        .upload_static_data(crate::animation::AnimationUpload {
            max_skeletons: 1,
            max_joints: 1,
            headers: &[AnimClipHeader {
                duration: 1.0,
                channel_offset: 0,
                channel_count: u32::from(!rest_pose),
                _pad: 0,
            }],
            channels: if rest_pose { &[] } else { &channels },
            times: &[0.0, 1.0],
            values: &[2.0, 4.0, 6.0, 12.0, 14.0, 16.0],
            joints: &[JointInfo {
                inverse_bind_matrix: identity,
                parent_index: u32::MAX,
                _pad: [0; 3],
                rest_translation: [2.0, 3.0, 4.0],
                _pad2: 0,
                rest_rotation: [0.0, 0.0, 0.0, 1.0],
                rest_scale: [1.0; 3],
                _pad3: 0,
            }],
        })
        .unwrap();
    let mut graph = readback_graph();
    let roles = [
        AnimationParams,
        AnimationClips,
        AnimationChannels,
        AnimationTimes,
        AnimationValues,
        AnimationJoints,
        AnimationWorld,
        AnimationOutput,
    ];
    let ids = import(&mut graph, &renderer, &roles);
    let bindings: Vec<_> = roles
        .iter()
        .enumerate()
        .map(|(index, &role)| (0, index as u32, role))
        .collect();
    dispatch(
        &mut graph,
        "animation pose",
        BuiltinComputeKernel::AnimationPose,
        &bindings,
        &ids,
    );
    prepare_copies(&mut renderer, &mut graph, &[(ids[&AnimationOutput], 0, 64)]);
    for time in [0.25, 0.5, 0.75].into_iter().take(FRAME_SLOTS) {
        let frame = acquire(&mut renderer);
        animation_uploader(&mut renderer).update_params(&[SkeletonAnimParams {
            clip_index: 0,
            target_clip_index: 0,
            current_time: time,
            target_time: 0.0,
            blend_weight: 0.0,
            joint_offset: 0,
            joint_count: 1,
            flags: 1,
        }]);
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap();
        #[cfg(target_os = "macos")]
        assert!(renderer.frame_slots[frame.slot()].submission.is_some());
    }
    for (slot, time) in [0.25, 0.5, 0.75].into_iter().take(FRAME_SLOTS).enumerate() {
        let bytes = read_completed(&mut renderer, &graph, slot);
        let matrix: Vec<_> = bytes[..64]
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let mut expected = identity;
        expected[12..15].copy_from_slice(&if rest_pose {
            [2.0, 3.0, 4.0]
        } else {
            [2.0 + 10.0 * time, 4.0 + 10.0 * time, 6.0 + 10.0 * time]
        });
        for (actual, expected) in matrix.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5, "slot {slot}: {matrix:?}");
        }
    }
    graph.cleanup();
    renderer.destroy();
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}

#[test]
fn test_native_graph_particle_emit_simulate_and_indirect_draw_outputs() {
    use BuiltinBuffer::*;
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
    prepare_particles(&mut renderer, config);
    let mut graph = readback_graph();
    let roles = [
        ParticleData,
        ParticleDeadList,
        ParticleAliveRead,
        ParticleAliveWrite,
        ParticleCounters,
        ParticleIndirect,
        ParticleFrame,
        ParticleEmitters,
    ];
    let ids = import(&mut graph, &renderer, &roles);
    let bindings = [
        (0, 0, ParticleData),
        (0, 1, ParticleDeadList),
        (0, 2, ParticleAliveRead),
        (0, 3, ParticleAliveWrite),
        (0, 4, ParticleCounters),
        (1, 0, ParticleFrame),
        (1, 1, ParticleEmitters),
    ];
    dispatch(
        &mut graph,
        "particle emit",
        BuiltinComputeKernel::ParticleEmit,
        &bindings,
        &ids,
    );
    dispatch(
        &mut graph,
        "particle simulate",
        BuiltinComputeKernel::ParticleSimulate,
        &bindings,
        &ids,
    );
    dispatch(
        &mut graph,
        "particle draw command",
        BuiltinComputeKernel::ParticleDrawCommand,
        &[(0, 0, ParticleCounters), (0, 1, ParticleIndirect)],
        &ids,
    );
    prepare_copies(
        &mut renderer,
        &mut graph,
        &[
            (ids[&ParticleIndirect], 0, 16),
            (ids[&ParticleCounters], 16, 16),
            (ids[&ParticleAliveWrite], 32, 16),
            (ids[&ParticleData], 64, 4096),
        ],
    );
    let frame = acquire(&mut renderer);
    step_particles(&mut renderer, frame.slot());
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
