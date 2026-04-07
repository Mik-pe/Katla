// GPU Pose Evaluation Validation Example
//
// Validates the GPU animation compute shader (pose_eval.wgsl) by:
// - Creating a headless Vulkan context with validation enabled
// - Building synthetic test animation data (3-joint chain, 2 clips)
// - Uploading to GPU, dispatching compute, reading back results
// - Checking interpolation correctness, hierarchy propagation, blending, and identity output
//
// Exit codes:
// - 0: All validations passed
// - 1: One or more validations failed

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use katla_gfx::animation::{
    AnimChannelInfo, AnimClipHeader, JointInfo, PoseComputeBuffers, PoseComputePipeline,
    SkeletonAnimParams,
};
use katla_gfx::renderer::AssetRegistry;
use katla_gfx::sync::VkShaderModule;
use katla_gfx::{ShaderCache, ValidationMode, VulkanContext};
use std::ffi::CString;
use std::path::PathBuf;
use std::process::ExitCode;

const JOINT_COUNT: usize = 3;
const MAX_MAT4_ELEMENT: f32 = 1e6;

// ---------------------------------------------------------------------------
// find_shader_directory (same as particle_validation_helpers)
// ---------------------------------------------------------------------------

fn find_shader_directory() -> PathBuf {
    let possible_paths = vec![
        PathBuf::from("resources/shaders"),
        PathBuf::from("../resources/shaders"),
        PathBuf::from("../../resources/shaders"),
        PathBuf::from("../../../resources/shaders"),
    ];

    for path in possible_paths {
        if path.exists() {
            log::info!("Found shader directory at: {:?}", path);
            return path;
        }
    }

    log::warn!("Could not find shader directory, defaulting to resources/shaders");
    PathBuf::from("resources/shaders")
}

// ---------------------------------------------------------------------------
// Synthetic test data
// ---------------------------------------------------------------------------

fn identity_mat4() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, //
    ]
}

struct TestData {
    clip_headers: Vec<AnimClipHeader>,
    channel_infos: Vec<AnimChannelInfo>,
    keyframe_times: Vec<f32>,
    keyframe_values: Vec<f32>,
    joint_infos: Vec<JointInfo>,
    joint_count: usize,
}

fn build_test_data() -> TestData {
    let joint_count = JOINT_COUNT;

    let joint_infos = vec![
        JointInfo {
            inverse_bind_matrix: identity_mat4(),
            parent_index: 0xFFFFFFFF,
            _pad: [0; 3],
            rest_translation: [0.0, 0.0, 0.0],
            _pad2: 0,
            rest_rotation: [0.0, 0.0, 0.0, 1.0],
            rest_scale: [1.0, 1.0, 1.0],
            _pad3: 0,
        },
        JointInfo {
            inverse_bind_matrix: identity_mat4(),
            parent_index: 0,
            _pad: [0; 3],
            rest_translation: [0.0, 0.0, 0.0],
            _pad2: 0,
            rest_rotation: [0.0, 0.0, 0.0, 1.0],
            rest_scale: [1.0, 1.0, 1.0],
            _pad3: 0,
        },
        JointInfo {
            inverse_bind_matrix: identity_mat4(),
            parent_index: 1,
            _pad: [0; 3],
            rest_translation: [0.0, 0.0, 0.0],
            _pad2: 0,
            rest_rotation: [0.0, 0.0, 0.0, 1.0],
            rest_scale: [1.0, 1.0, 1.0],
            _pad3: 0,
        },
    ];

    // Clip 0: "Move" — translation on joint 0, rotation on joint 1
    //   Translation keyframes: t=0 -> (0,0,0), t=1 -> (2,0,0)
    //   Rotation keyframes:    t=0 -> identity quat, t=1 -> 90 deg Y rotation
    // Clip 1: "Scale" — scale on joint 0
    //   Scale keyframes: t=0 -> (1,1,1), t=1 -> (2,2,2)

    let clip_headers = vec![
        AnimClipHeader {
            duration: 1.0,
            channel_offset: 0,
            channel_count: 2,
            _pad: 0,
        },
        AnimClipHeader {
            duration: 1.0,
            channel_offset: 2,
            channel_count: 1,
            _pad: 0,
        },
    ];

    // Channel 0: Joint 0 translation, linear, 2 keyframes (times at offset 0, values at offset 0)
    // Channel 1: Joint 1 rotation, linear, 2 keyframes (times at offset 0, values at offset 6)
    // Channel 2: Joint 0 scale, linear, 2 keyframes (times at offset 0, values at offset 14)
    //
    // All channels share the same [0.0, 1.0] timeline via time_offset=0.
    // Values are packed sequentially: translation(6) + rotation(8) + scale(6) = 20.
    let channel_infos = vec![
        AnimChannelInfo {
            target_joint: 0,
            path_type: 0, // PATH_TRANSLATION
            time_offset: 0,
            value_offset: 0,
            keyframe_count: 2,
            interpolation: 0,
            _pad: [0; 2],
        },
        AnimChannelInfo {
            target_joint: 1,
            path_type: 1,    // PATH_ROTATION
            time_offset: 0,  // shared times at offset 0
            value_offset: 6, // 2 translation keyframes * 3 floats = 6 values
            keyframe_count: 2,
            interpolation: 0,
            _pad: [0; 2],
        },
        AnimChannelInfo {
            target_joint: 0,
            path_type: 2,     // PATH_SCALE
            time_offset: 0,   // shared times at offset 0
            value_offset: 14, // 6 + 2*4 = 14 values
            keyframe_count: 2,
            interpolation: 0,
            _pad: [0; 2],
        },
    ];

    // All channels use time_offset=0, so only one copy of the timeline is needed.
    let keyframe_times = vec![0.0, 1.0];

    // Translation (joint 0): k0 (0,0,0), k1 (2,0,0)
    // Rotation (joint 1):    k0 identity (0,0,0,1), k1 90° Y (0, sin45, 0, cos45)
    // Scale (joint 0, clip 1): k0 (1,1,1), k1 (2,2,2)
    let keyframe_values = vec![
        // Translation keyframes
        0.0,
        0.0,
        0.0, // k0
        2.0,
        0.0,
        0.0, // k1
        // Rotation keyframes
        0.0,
        0.0,
        0.0,
        1.0, // k0: identity
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2, // k1: 90 deg Y
        // Scale keyframes (clip 1)
        1.0,
        1.0,
        1.0, // k0
        2.0,
        2.0,
        2.0, // k1
    ];

    TestData {
        clip_headers,
        channel_infos,
        keyframe_times,
        keyframe_values,
        joint_infos,
        joint_count,
    }
}

// ---------------------------------------------------------------------------
// Staging buffer helpers
// ---------------------------------------------------------------------------

struct StagingBuffer {
    buffer: vk::Buffer,
    allocation: Allocation,
    size: u64,
}

impl StagingBuffer {
    fn new(context: &VulkanContext, size: u64, name: &str) -> Result<Self, String> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            context
                .device
                .create_buffer(&buffer_info, None)
                .map_err(|e| format!("Failed to create staging buffer '{}': {:?}", name, e))?
        };

        let requirements = unsafe { context.device.get_buffer_memory_requirements(buffer) };

        let allocation = context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate staging memory '{}': {}", name, e))?;

        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind staging buffer '{}': {:?}", name, e))?;
        }

        Ok(Self {
            buffer,
            allocation,
            size,
        })
    }
}

// ---------------------------------------------------------------------------
// Dispatch + readback
// ---------------------------------------------------------------------------

fn dispatch_and_readback(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    params: &[SkeletonAnimParams],
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<Vec<[f32; 16]>, String> {
    let output_size = buffers.output_size();

    // Update params
    buffers.update_params(params);

    // Allocate command buffer, fence
    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe {
        context
            .device
            .create_fence(&fence_info, None)
            .map_err(|e| format!("Failed to create fence: {:?}", e))?
    };

    // Record dispatch
    pipeline.record_dispatch(cmd, asset_registry, params.len() as u32);

    // Barrier: COMPUTE_SHADER -> TRANSFER (output buffer)
    pipeline.add_output_barrier(
        cmd,
        buffers,
        vk::PipelineStageFlags2::TRANSFER,
        vk::AccessFlags2::TRANSFER_READ,
    );

    // Change output barrier dst_access to TRANSFER_COPY for copy source
    let copy_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_stage_mask(vk::PipelineStageFlags2::COPY)
        .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffers.output_buffer())
        .offset(0)
        .size(output_size);

    let dep_info =
        vk::DependencyInfo::default().buffer_memory_barriers(std::slice::from_ref(&copy_barrier));
    unsafe {
        context.device.cmd_pipeline_barrier2(cmd, &dep_info);
    }

    // Copy output -> staging
    let copy_region = vk::BufferCopy::default()
        .src_offset(0)
        .dst_offset(0)
        .size(output_size.min(staging.size));

    unsafe {
        context.device.cmd_copy_buffer(
            cmd,
            buffers.output_buffer(),
            staging.buffer,
            &[copy_region],
        );
    }

    // Barrier: staging TRANSFER_WRITE -> HOST_READ
    let host_barrier = vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COPY)
        .dst_stage_mask(vk::PipelineStageFlags2::HOST)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags2::HOST_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(staging.buffer)
        .offset(0)
        .size(staging.size);

    let dep_info2 =
        vk::DependencyInfo::default().buffer_memory_barriers(std::slice::from_ref(&host_barrier));
    unsafe {
        context.device.cmd_pipeline_barrier2(cmd, &dep_info2);
    }

    // End command buffer and submit with fence
    cmd_buf.end_single_time_command();

    unsafe {
        let submit_info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));
        context
            .device
            .queue_submit(context.gfx_queue.vk_queue(), &[submit_info], fence)
            .map_err(|e| format!("Failed to submit queue: {}", e))?;
        context
            .device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|e| format!("Failed to wait for fence: {:?}", e))?;
        context.device.destroy_fence(fence, None);
    }

    cmd_buf.return_to_pool();

    // Read back from staging
    context.invalidate_mapped_memory(&staging.allocation, 0, staging.size);

    let matrix_count = (output_size as usize) / 64;
    let mut matrices = Vec::with_capacity(matrix_count);

    if let Some(mapped) = staging.allocation.mapped_ptr() {
        let src = mapped.as_ptr() as *const f32;
        for i in 0..matrix_count {
            let mut m = [0.0f32; 16];
            unsafe {
                std::ptr::copy_nonoverlapping(src.add(i * 16), m.as_mut_ptr(), 16);
            }
            matrices.push(m);
        }
    } else {
        return Err("Staging buffer not mapped".to_string());
    }

    Ok(matrices)
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

fn validate_matrices_finite(matrices: &[[f32; 16]], label: &str) -> Result<(), String> {
    for (i, m) in matrices.iter().enumerate() {
        for (j, &v) in m.iter().enumerate() {
            if v.is_nan() {
                return Err(format!(
                    "{}: joint {} matrix element {} is NaN",
                    label, i, j
                ));
            }
            if v.is_infinite() {
                return Err(format!(
                    "{}: joint {} matrix element {} is Inf",
                    label, i, j
                ));
            }
            if v.abs() > MAX_MAT4_ELEMENT {
                return Err(format!(
                    "{}: joint {} matrix element {} = {} exceeds {}",
                    label, i, j, v, MAX_MAT4_ELEMENT
                ));
            }
        }
    }
    Ok(())
}

fn is_identity_matrix(m: &[f32; 16], tolerance: f32) -> bool {
    let expected = identity_mat4();
    for i in 0..16 {
        if (m[i] - expected[i]).abs() > tolerance {
            return false;
        }
    }
    true
}

fn validate_identity_when_not_playing(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing FLAG_PLAYING=0 (should output identity matrices)...");

    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 0,
        current_time: 0.5,
        target_time: 0.5,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 0, // NOT playing
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "not_playing")?;

    for (i, m) in matrices.iter().enumerate() {
        if !is_identity_matrix(m, 1e-4) {
            return Err(format!(
                "not_playing: joint {} expected identity, got {:?}",
                i, m
            ));
        }
    }

    log::info!(
        "  PASSED: all {} joints output identity matrices",
        matrices.len()
    );
    Ok(())
}

fn validate_interpolation_at_t0(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing t=0 (first keyframe values)...");

    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 0,
        current_time: 0.0,
        target_time: 0.0,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 1, // FLAG_PLAYING
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "t0")?;

    // At t=0: joint 0 has translation (0,0,0), joint 1 has identity rotation
    // Joint 0: local = translate(0,0,0) * identity_rotation * scale(1,1,1) = identity
    // With identity IBM, output = identity
    // Joint 1: local = translate(0,0,0) * identity_rotation * scale(1,1,1) = identity
    //          parent (joint 0) is identity -> world = identity * identity = identity
    //          output = identity * identity_ibm = identity
    // Joint 2: same as joint 1 (no channels targeting it, all parents identity)

    // All joints should be identity at t=0
    for (i, m) in matrices.iter().enumerate() {
        if !is_identity_matrix(m, 1e-3) {
            log::info!(
                "  t0 joint {}: [{:.4}, {:.4}, {:.4}, {:.4} | {:.4}, {:.4}, {:.4}, {:.4} | {:.4}, {:.4}, {:.4}, {:.4} | {:.4}, {:.4}, {:.4}, {:.4}]",
                i,
                m[0],
                m[1],
                m[2],
                m[3],
                m[4],
                m[5],
                m[6],
                m[7],
                m[8],
                m[9],
                m[10],
                m[11],
                m[12],
                m[13],
                m[14],
                m[15]
            );
        }
    }

    // Joint 0 and 1 should be identity at t=0 (identity translation + identity rotation)
    // Joint 2 should also be identity (parent chain all identity)
    for (i, m) in matrices.iter().enumerate().take(JOINT_COUNT) {
        if !is_identity_matrix(m, 1e-3) {
            return Err(format!("t0: joint {} expected identity matrix at t=0", i));
        }
    }

    log::info!(
        "  PASSED: all {} joints are identity at t=0",
        matrices.len()
    );
    Ok(())
}

fn validate_interpolation_at_t1(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing t=1 (last keyframe values)...");

    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 0,
        current_time: 1.0,
        target_time: 1.0,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 1,
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "t1")?;

    // At t=1: joint 0 translation = (2,0,0), joint 1 rotation = 90° Y
    // Joint 0: local = translate(2,0,0) * identity_rot * scale(1,1,1)
    //   mat4 = [[1,0,0,0],[0,1,0,0],[0,0,1,0],[2,0,0,1]]
    //   output = local * identity_ibm = local
    // Joint 1: local = translate(0,0,0) * rotY(90°) * scale(1,1,1)
    //   rotY(90°) = [[0,0,-1,0],[0,1,0,0],[1,0,0,0],[0,0,0,1]]  (column-major)
    //   parent = joint0_world = translate(2,0,0)
    //   world = parent * local = translate(2,0,0) * rotY(90°)
    //   output = world * identity_ibm = world
    // Joint 2: local = identity (no channels), parent = joint1_world
    //   world = joint1_world * identity = joint1_world
    //   output = world * identity_ibm = joint1_world

    // Check joint 0: should be translation by (2,0,0)
    let m0 = &matrices[0];
    // Column-major: m[12]=tx, m[13]=ty, m[14]=tz
    let tx0 = m0[12];
    let ty0 = m0[13];
    let tz0 = m0[14];
    if (tx0 - 2.0).abs() > 1e-3 || ty0.abs() > 1e-3 || tz0.abs() > 1e-3 {
        return Err(format!(
            "t1: joint 0 expected translation (2,0,0), got ({:.4}, {:.4}, {:.4})",
            tx0, ty0, tz0
        ));
    }

    log::info!("  PASSED: joint 0 translation correct at t=1");

    // Check that joints 1 and 2 are not identity (they have rotation/hierarchy)
    if is_identity_matrix(&matrices[1], 1e-3) {
        return Err("t1: joint 1 should NOT be identity at t=1 (has rotation)".to_string());
    }
    if is_identity_matrix(&matrices[2], 1e-3) {
        return Err("t1: joint 2 should NOT be identity at t=1 (parent has rotation)".to_string());
    }

    // Check hierarchy: joint 2 should equal joint 1 (joint 2 has identity local, parent = joint 1)
    // Since IBM is identity for both, output[2] = world[2] = world[1] * identity = world[1]
    // And output[1] = world[1] * identity_ibm = world[1]
    // So output[1] == output[2]
    for (i, (v1, v2)) in matrices[1].iter().zip(&matrices[2]).enumerate().take(16) {
        if (v1 - v2).abs() > 1e-3 {
            return Err(format!(
                "t1: joint 2 should equal joint 1 (identity local + same parent), \
                 but element {} differs: {:.6} vs {:.6}",
                i, v1, v2
            ));
        }
    }

    log::info!("  PASSED: all {} joints correct at t=1", matrices.len());
    Ok(())
}

fn validate_interpolation_at_t05(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing t=0.5 (midpoint interpolation)...");

    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 0,
        current_time: 0.5,
        target_time: 0.5,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 1,
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "t05")?;

    // At t=0.5: joint 0 translation = lerp((0,0,0), (2,0,0), 0.5) = (1,0,0)
    let m0 = &matrices[0];
    let tx0 = m0[12];
    let ty0 = m0[13];
    let tz0 = m0[14];
    if (tx0 - 1.0).abs() > 0.01 || ty0.abs() > 0.01 || tz0.abs() > 0.01 {
        return Err(format!(
            "t05: joint 0 expected translation (1,0,0), got ({:.4}, {:.4}, {:.4})",
            tx0, ty0, tz0
        ));
    }
    log::info!("  PASSED: joint 0 translation midpoint correct");

    // The rotation matrix from slerp(identity, rotY90, 0.5) should be a valid rotation matrix.
    // A valid rotation matrix has columns that are orthogonal unit vectors with determinant +1.
    // Check that the 3x3 upper-left is a valid rotation (columns are unit vectors).
    let m1 = &matrices[1];
    let col0_len = (m1[0].powi(2) + m1[1].powi(2) + m1[2].powi(2)).sqrt();
    let col1_len = (m1[4].powi(2) + m1[5].powi(2) + m1[6].powi(2)).sqrt();
    let col2_len = (m1[8].powi(2) + m1[9].powi(2) + m1[10].powi(2)).sqrt();

    if (col0_len - 1.0).abs() > 0.01 {
        return Err(format!(
            "t05: joint 1 column 0 length = {} (expected 1.0)",
            col0_len
        ));
    }
    if (col1_len - 1.0).abs() > 0.01 {
        return Err(format!(
            "t05: joint 1 column 1 length = {} (expected 1.0)",
            col1_len
        ));
    }
    if (col2_len - 1.0).abs() > 0.01 {
        return Err(format!(
            "t05: joint 1 column 2 length = {} (expected 1.0)",
            col2_len
        ));
    }
    log::info!("  PASSED: joint 1 rotation produces unit column vectors");

    // Check hierarchy propagation: joint 1's translation should include joint 0's translation
    // Joint 0 world = translate(1,0,0), joint 1 world = translate(1,0,0) * rotY(45°)
    // So joint 1's world translation = (1,0,0) and joint 2's world = same as joint 1
    let tx1 = m1[12];
    if (tx1 - 1.0).abs() > 0.01 {
        return Err(format!(
            "t05: joint 1 expected tx=1.0 (from parent chain), got {:.4}",
            tx1
        ));
    }

    // Joint 2 should inherit the full chain
    let m2 = &matrices[2];
    let tx2 = m2[12];
    if (tx2 - tx1).abs() > 0.01 {
        return Err(format!(
            "t05: joint 2 tx ({:.4}) should match joint 1 tx ({:.4})",
            tx2, tx1
        ));
    }

    log::info!("  PASSED: hierarchy propagation correct at t=0.5");
    Ok(())
}

fn validate_hierarchy_propagation(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing hierarchy propagation (joint chain)...");

    // At t=1, joint 0 has translation (2,0,0), joint 1 has rotation, joint 2 has no channels.
    // Joint 2's world transform should include joint 0's translation (through the chain).
    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 0,
        current_time: 1.0,
        target_time: 1.0,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 1,
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    // Joint 2's translation column should be non-zero (inherited from joint 0's translation)
    let m2 = &matrices[2];
    let tx2 = m2[12];
    let ty2 = m2[13];
    let _tz2 = m2[14];

    // Joint 0 translates by (2,0,0), joint 1 rotates 90° around Y
    // Joint 2 inherits the chain: translate(2,0,0) * rotY(90°) * identity
    // The resulting translation of joint 2 should be (2,0,0) since rotation
    // is applied at the joint 1 level and joint 2 has identity local transform.
    // world[2] = world[1] * local[2] = (translate(2,0,0) * rotY(90°)) * identity
    // So joint 2's translation = joint 1's translation = (2,0,0)
    if (tx2 - 2.0).abs() > 0.01 {
        return Err(format!(
            "hierarchy: joint 2 expected tx=2.0 (inherited from root), got {:.4}",
            tx2
        ));
    }
    if ty2.abs() > 0.01 {
        return Err(format!(
            "hierarchy: joint 2 expected ty=0.0, got {:.4}",
            ty2
        ));
    }

    // Joint 2's rotation should match joint 1's (both inherit the same rotation)
    for (i, (v1, v2)) in matrices[1].iter().zip(&matrices[2]).enumerate().take(12) {
        if (v1 - v2).abs() > 1e-3 {
            return Err(format!(
                "hierarchy: joint 2 upper 3x3 ({:.4}) should match joint 1 ({:.4}) at element {}",
                v2, v1, i
            ));
        }
    }

    log::info!("  PASSED: hierarchy propagation verified");
    Ok(())
}

fn validate_clip1_scale(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing clip 1 scale evaluation at t=0.5...");

    let params = [SkeletonAnimParams {
        clip_index: 1,
        target_clip_index: 0,
        current_time: 0.5,
        target_time: 0.0,
        blend_weight: 0.0,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 1,
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "clip1_scale")?;

    // At t=0.5: scale channel gives lerp((1,1,1), (2,2,2), 0.5) = (1.5, 1.5, 1.5)
    let m0 = &matrices[0];
    let scale_x = m0[0];

    log::info!("  clip1 joint 0: scale_x={:.4}", scale_x);

    if (scale_x - 1.5).abs() > 0.05 {
        return Err(format!(
            "clip1_scale: joint 0 expected scale_x=1.5, got {:.4}",
            scale_x
        ));
    }

    log::info!("  PASSED: clip 1 scale evaluation correct");
    Ok(())
}

fn validate_blending(
    context: &VulkanContext,
    pipeline: &PoseComputePipeline,
    buffers: &PoseComputeBuffers,
    staging: &StagingBuffer,
    asset_registry: &AssetRegistry,
) -> Result<(), String> {
    log::info!("Testing clip blending (blend_weight=0.5)...");

    // Clip 0 at t=0.5: joint 0 translate(1,0,0), identity scale
    // Clip 1 at t=0.5: joint 0 identity translate, scale(1.5,1.5,1.5)
    // Blend 0.5: translate = lerp((1,0,0), (0,0,0), 0.5) = (0.5,0,0)
    //             scale = lerp((1,1,1), (1.5,1.5,1.5), 0.5) = (1.25, 1.25, 1.25)
    let params = [SkeletonAnimParams {
        clip_index: 0,
        target_clip_index: 1,
        current_time: 0.5,
        target_time: 0.5,
        blend_weight: 0.5,
        joint_offset: 0,
        joint_count: JOINT_COUNT as u32,
        flags: 5, // FLAG_PLAYING | FLAG_BLENDING
    }];

    let matrices =
        dispatch_and_readback(context, pipeline, buffers, &params, staging, asset_registry)?;

    validate_matrices_finite(&matrices, "blending")?;

    // Joint 0: blended translate = (0.5,0,0), scale = (1.25, 1.25, 1.25)
    // mat4_from_trs(translate(0.5,0,0), identity_rot, scale(1.25,1.25,1.25))
    // = [[1.25, 0, 0, 0], [0, 1.25, 0, 0], [0, 0, 1.25, 0], [0.5, 0, 0, 1]]
    let m0 = &matrices[0];
    let tx0 = m0[12];
    let scale_x = m0[0]; // diagonal element for scale.x

    if (tx0 - 0.5).abs() > 0.01 {
        return Err(format!(
            "blending: joint 0 expected tx=0.5 (blended), got {:.4}",
            tx0
        ));
    }
    if (scale_x - 1.25).abs() > 0.05 {
        return Err(format!(
            "blending: joint 0 expected scale_x=1.25 (blended), got {:.4}",
            scale_x
        ));
    }

    log::info!("  PASSED: blending produces correct interpolated values");
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("=== GPU Pose Evaluation Validation ===");

    // Create headless Vulkan context with validation
    let app_name = CString::new("Animation Validation").unwrap();
    let engine_name = CString::new("Katla Engine").unwrap();

    log::info!("Creating headless Vulkan context with GPU-assisted validation...");
    let context = VulkanContext::init_headless(ValidationMode::GpuAssisted, app_name, engine_name);
    let context = std::rc::Rc::new(context);
    log::info!("Vulkan context created successfully");

    let mut asset_registry = AssetRegistry::new();

    let shader_dir = find_shader_directory();
    log::info!("Using shader directory: {:?}", shader_dir);

    // Build synthetic test data
    let test_data = build_test_data();
    log::info!(
        "Test data: {} clips, {} channels, {} joints, {} keyframe times, {} keyframe values",
        test_data.clip_headers.len(),
        test_data.channel_infos.len(),
        test_data.joint_count,
        test_data.keyframe_times.len(),
        test_data.keyframe_values.len(),
    );

    // Create pipeline and buffers
    log::info!("Creating PoseComputePipeline...");
    let mut pipeline = PoseComputePipeline::new(context.clone());

    let mut shader_cache = ShaderCache::new(context.device.clone());
    let pose_shader_path = shader_dir.join("compute/animation/pose_eval.wgsl");
    let pose_shader =
        match shader_cache.load_shader(&pose_shader_path, vk::ShaderStageFlags::COMPUTE) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to load pose eval shader: {}", e);
                return ExitCode::from(1);
            }
        };

    if let Err(e) = pipeline.initialize(&mut asset_registry, VkShaderModule(pose_shader)) {
        log::error!("Failed to initialize pose compute pipeline: {}", e);
        return ExitCode::from(1);
    }
    log::info!("Pose compute pipeline initialized");

    log::info!("Creating PoseComputeBuffers...");
    let mut buffers = PoseComputeBuffers::new(context.clone());

    if let Err(e) = buffers.allocate_params(1) {
        log::error!("Failed to allocate params buffer: {}", e);
        return ExitCode::from(1);
    }

    let headers_size =
        (test_data.clip_headers.len() * std::mem::size_of::<AnimClipHeader>()) as u64;
    let channels_size =
        (test_data.channel_infos.len() * std::mem::size_of::<AnimChannelInfo>()) as u64;
    let times_size = (test_data.keyframe_times.len() * std::mem::size_of::<f32>()) as u64;
    let values_size = (test_data.keyframe_values.len() * std::mem::size_of::<f32>()) as u64;

    if let Err(e) = buffers.allocate_clip_data(headers_size, channels_size, times_size, values_size)
    {
        log::error!("Failed to allocate clip data buffers: {}", e);
        return ExitCode::from(1);
    }

    if let Err(e) = buffers.allocate_joints(test_data.joint_count) {
        log::error!("Failed to allocate joints buffer: {}", e);
        return ExitCode::from(1);
    }

    if let Err(e) = buffers.allocate_world(test_data.joint_count) {
        log::error!("Failed to allocate world buffer: {}", e);
        return ExitCode::from(1);
    }

    if let Err(e) = buffers.allocate_output(test_data.joint_count) {
        log::error!("Failed to allocate output buffer: {}", e);
        return ExitCode::from(1);
    }

    // Update descriptor bindings
    pipeline.update_bindings(&buffers);

    // Upload clip data via staging: the GPU-only buffers need data uploaded.
    // PoseComputeBuffers.upload_clip_data writes to CpuToGpu allocations.
    buffers.upload_clip_data(
        &test_data.clip_headers,
        &test_data.channel_infos,
        &test_data.keyframe_times,
        &test_data.keyframe_values,
    );
    buffers.upload_joints(&test_data.joint_infos);
    log::info!("Test data uploaded to GPU buffers");

    // Create staging buffer for readback
    let output_size = buffers.output_size();
    let staging = match StagingBuffer::new(&context, output_size, "pose_readback_staging") {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create staging buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    // Run validations
    let mut failed = false;

    if let Err(e) =
        validate_identity_when_not_playing(&context, &pipeline, &buffers, &staging, &asset_registry)
    {
        log::error!("FAIL: identity when not playing: {}", e);
        failed = true;
    }

    if let Err(e) =
        validate_interpolation_at_t0(&context, &pipeline, &buffers, &staging, &asset_registry)
    {
        log::error!("FAIL: interpolation at t=0: {}", e);
        failed = true;
    }

    if let Err(e) =
        validate_interpolation_at_t1(&context, &pipeline, &buffers, &staging, &asset_registry)
    {
        log::error!("FAIL: interpolation at t=1: {}", e);
        failed = true;
    }

    if let Err(e) =
        validate_interpolation_at_t05(&context, &pipeline, &buffers, &staging, &asset_registry)
    {
        log::error!("FAIL: interpolation at t=0.5: {}", e);
        failed = true;
    }

    if let Err(e) =
        validate_hierarchy_propagation(&context, &pipeline, &buffers, &staging, &asset_registry)
    {
        log::error!("FAIL: hierarchy propagation: {}", e);
        failed = true;
    }

    if let Err(e) = validate_clip1_scale(&context, &pipeline, &buffers, &staging, &asset_registry) {
        log::error!("FAIL: clip1_scale: {}", e);
        failed = true;
    }

    if let Err(e) = validate_blending(&context, &pipeline, &buffers, &staging, &asset_registry) {
        log::error!("FAIL: blending: {}", e);
        failed = true;
    }

    // Cleanup
    pipeline.destroy();
    buffers.destroy();
    unsafe {
        context.device.destroy_buffer(staging.buffer, None);
    }
    context
        .allocator
        .free(staging.allocation, "animation staging buffer");

    if failed {
        log::error!("=== Validation FAILED ===");
        ExitCode::from(1)
    } else {
        log::info!("=== All Validations Passed ===");
        ExitCode::SUCCESS
    }
}
