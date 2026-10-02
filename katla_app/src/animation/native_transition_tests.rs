//! Native acceptance of an agent request, typed playback and joint-matrix output.

use std::collections::HashMap;
use std::ffi::CString;

use katla_agent::animation::AnimationOp;
use katla_ecs::{SystemExecutionOrder, World};
use katla_gfx::render_graph::any_frame_graph::AnyFrameGraph;
use katla_gfx::render_graph::*;
use katla_gfx::renderer::frame_scope::FrameAcquisition;
use katla_gfx::{AnyRenderer, GpuRenderer, ValidationMode};
use katla_math::Mat4;

use super::gpu_clip_loader::{build_skeleton_params, prepare_gpu_anim_data};
use super::*;

#[test]
#[ignore = "requires a native GPU with API validation; CI runs this after capability probing"]
fn test_native_agent_fade_reaches_target_after_source_completion() {
    assert_native_fade(false);
}

#[test]
#[ignore = "requires a native GPU with API validation; CI runs this after capability probing"]
fn test_native_trigger_enter_fades_to_target_joint_matrix() {
    assert_native_fade(true);
}

fn assert_native_fade(from_trigger: bool) {
    let model = AnimatedModel {
        animations: [("source", 0.1, 2.0), ("target", 0.2, 12.0)]
            .into_iter()
            .map(|(name, duration, x)| {
                (
                    name.into(),
                    AnimationClip {
                        name: name.into(),
                        duration,
                        channels: vec![AnimationChannel {
                            target_node: 0,
                            path: ChannelPath::Translation,
                            sampler: AnimationSampler::new_translation(
                                vec![0.0, duration],
                                vec![[x, 0.0, 0.0]; 2],
                                Interpolation::Linear,
                            ),
                        }],
                    },
                )
            })
            .collect(),
        sequences: HashMap::new(),
    };
    let names = model
        .animations
        .keys()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u32))
        .collect();
    let data = prepare_gpu_anim_data(
        &model,
        &Skin::new("rig", vec![0], vec![Mat4::identity()]),
        &Skeleton::with_parents("rig", vec![None]),
    );
    let mut world = World::new();
    let entity = world.spawn((model, AnimationPlayer::new("source")));
    world.register_typed_system(AnimationUpdateSystem, SystemExecutionOrder::NORMAL);
    if from_trigger {
        use crate::components::TransformComponent;
        use katla_physics::{ColliderShape, PhysicsActive, PhysicsWorld, RigidBody, SphereShape};
        world.add_component(entity, TransformComponent::default());
        world.add_component(entity, ColliderShape::Sphere(SphereShape::new(0.5)));
        world.add_component(entity, RigidBody::kinematic());
        world.insert_resource(PhysicsWorld::new());
        world.insert_resource(PhysicsActive(true));
        let op = serde_json::from_value::<katla_agent::events::TriggerOp<String>>(serde_json::json!({
            "action":"create_box", "name":"Fade zone", "position":[0,0,0], "half_extents":[2,2,2],
            "rules":[{"event":"enter", "other_entity":entity.id().to_string(), "actions":[{
                "action":"play_animation", "target":{"kind":"other"}, "clip":"target",
                "fade_seconds":1.0, "looping":false
            }]}]
        }))
        .unwrap();
        crate::events::control::execute(&mut world, op.resolve_ids().unwrap()).unwrap();
        katla_ecs::System::update(
            &mut crate::systems::physics::RapierPhysicsSystem,
            &mut world,
            0.016,
        );
        assert!(
            world
                .get_component::<AnimationPlayer>(entity)
                .unwrap()
                .blending
        );
    } else {
        control::execute(
            &mut world,
            AnimationOp::Play {
                entity_id: entity.id(),
                clip: "target".into(),
                fade_seconds: 1.0,
                looping: false,
                speed: 1.0,
            },
        )
        .unwrap();
    }

    let label = CString::new("agent animation transition").unwrap();
    let engine = CString::new("Katla").unwrap();
    #[cfg(target_os = "macos")]
    let mut renderer =
        AnyRenderer::new_metal_headless(16, 16, ValidationMode::Enabled, label, engine).unwrap();
    #[cfg(not(target_os = "macos"))]
    let mut renderer =
        AnyRenderer::new_vulkan_headless(16, 16, ValidationMode::Enabled, label, engine).unwrap();
    let mut graph = match &renderer {
        AnyRenderer::Vulkan(_) => AnyFrameGraph::from_vulkan(FrameGraph::new()),
        #[cfg(target_os = "macos")]
        AnyRenderer::Metal(_) => AnyFrameGraph::from_metal(FrameGraph::new()),
    };
    #[cfg(not(target_os = "macos"))]
    let errors = {
        let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let AnyRenderer::Vulkan(backend) = &renderer;
            assert!(backend.context().validation_active());
            let captured = errors.clone();
            backend
                .context()
                .set_validation_callback(move |message, level| {
                    if level == katla_gfx::ValidationLevel::Error {
                        captured.lock().unwrap().push(message.to_string());
                    }
                });
        }
        errors
    };
    let storage = BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE;
    let descriptions = [
        BufferDesc::new(32, storage, BufferMemoryPolicy::CpuVisible),
        BufferDesc::new(64, storage, BufferMemoryPolicy::DeviceLocal),
        BufferDesc::new(64, storage, BufferMemoryPolicy::DeviceLocal),
        BufferDesc::new(
            64,
            BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
            BufferMemoryPolicy::Readback,
        ),
    ];
    let mut handles = Vec::new();
    let mut ids = Vec::new();
    for slot in 0..3 {
        for (i, desc) in descriptions.into_iter().enumerate() {
            let handle = renderer.create_buffer(desc).unwrap();
            handles.push(handle);
            if slot == 0 {
                ids.push(
                    graph
                        .import_buffer(format!("dynamic {i}"), handle, desc)
                        .unwrap(),
                );
            }
        }
    }
    let [params_id, world_id, output_id, readback_id] = ids.as_slice() else {
        panic!("four buffers")
    };
    let (params_id, world_id, output_id, readback_id) =
        (*params_id, *world_id, *output_id, *readback_id);
    let static_bytes: [&[u8]; 5] = [
        bytemuck::cast_slice(&data.clip_headers),
        bytemuck::cast_slice(&data.channel_infos),
        bytemuck::cast_slice(&data.keyframe_times),
        bytemuck::cast_slice(&data.keyframe_values),
        bytemuck::cast_slice(&data.joint_infos),
    ];
    let mut bindings = vec![params_id];
    for (i, bytes) in static_bytes.into_iter().enumerate() {
        let desc = BufferDesc::new(bytes.len() as u64, storage, BufferMemoryPolicy::DeviceLocal);
        let handle = renderer.create_buffer_with_data(desc, bytes).unwrap();
        handles.push(handle);
        bindings.push(
            graph
                .import_buffer(format!("static {i}"), handle, desc)
                .unwrap(),
        );
    }
    bindings.extend([world_id, output_id]);
    let command = ComputeDispatch {
        pipeline: ComputePipelineDesc {
            wgsl: include_str!("../../../resources/shaders/compute/animation/pose_eval.wgsl")
                .into(),
            entry: "cs_main".into(),
        },
        bindings: bindings
            .into_iter()
            .enumerate()
            .map(|(binding, resource)| ComputeBinding {
                group: 0,
                binding: binding as u32,
                resource,
                range: BufferByteRange::WHOLE,
            })
            .collect(),
        constants: vec![],
        size: ComputeDispatchSize::Direct([1, 1, 1]),
    };
    graph
        .add_pass(
            PassDesc::new("pose", PassType::Compute, vec![], vec![])
                .with_buffer_accesses(command.accesses().unwrap())
                .with_commands([ComputeCommand::Dispatch(command)]),
        )
        .unwrap();
    graph
        .add_pass(
            PassDesc::new("copy", PassType::Transfer, vec![], vec![])
                .with_buffer_accesses([
                    BufferAccess::transfer_read(output_id),
                    BufferAccess::transfer_write(readback_id),
                ])
                .with_commands([ComputeCommand::CopyBuffer {
                    source: output_id,
                    destination: readback_id,
                    source_offset: 0,
                    destination_offset: 0,
                    size: 64,
                }]),
        )
        .unwrap();
    let mut host = PassDesc::new("host", PassType::Transfer, vec![], vec![])
        .with_buffer_accesses([BufferAccess::readback_read(readback_id)])
        .with_commands([]);
    host.side_effect = true;
    graph.add_pass(host).unwrap();
    graph.initialize_compute_pipelines(&mut renderer).unwrap();
    for (dt, expected_x) in [(0.0, 2.0), (0.5, 7.0), (0.5, 12.0)] {
        world.update(dt);
        #[cfg(target_os = "macos")]
        {
            let drawable = renderer.create_offscreen_texture(16, 16);
            renderer.set_headless_drawable(drawable);
        }
        let FrameAcquisition::Ready(frame) = renderer.acquire_frame().unwrap() else {
            panic!("headless frame")
        };
        let params = build_skeleton_params(
            world.get_component::<AnimationPlayer>(entity).unwrap(),
            &names,
            0,
            1,
        );
        let offset = frame.slot() * 4;
        for (&resource, &handle) in ids.iter().zip(&handles[offset..offset + 4]) {
            graph.rebind_imported_buffer(resource, handle).unwrap();
        }
        renderer
            .write_buffer(&frame, handles[offset], 0, bytemuck::bytes_of(&params))
            .unwrap();
        renderer.render(&frame, &mut graph, |_| {}).unwrap();
        renderer.present(frame).unwrap();
        renderer.wait_for_device();
        let bytes = renderer
            .read_buffer_completed(handles[offset + 3], BufferByteRange::new(0, 64))
            .unwrap()
            .unwrap();
        let matrix: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        let expected = Mat4::from_translation([expected_x, 0.0, 0.0]).to_array();
        for (actual, expected) in matrix.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-5,
                "expected x={expected_x}: {matrix:?}"
            );
        }
    }
    graph.cleanup();
    for handle in handles {
        renderer.destroy_buffer(handle).unwrap();
    }
    renderer.destroy();
    #[cfg(not(target_os = "macos"))]
    assert!(
        errors.lock().unwrap().is_empty(),
        "{:?}",
        errors.lock().unwrap()
    );
}
