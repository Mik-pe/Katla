// Light + Shadow Validation Example
//
// Validates the lighting and shadow pipeline by:
// - Testing CSM cascade computation (CPU-side, no GPU needed)
// - Testing Forward+ light culling compute shader via GPU dispatch + readback
// - Testing shadow sampling via GPU compute dispatch + readback
//
// Exit codes:
// - 0: All validations passed
// - 1: One or more validations failed

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use katla_gfx::lighting::{LightCullFrameData, LightCullingBuffers, PointLightGPU, TILE_SIZE};
use katla_gfx::shadow::cascade::{
    CascadeParams, CascadeShadowMap, ShadowCascadeGPU, ShadowFrameData,
};
use katla_gfx::sync::VkDescriptorSetLayout;
use katla_gfx::{
    CommandBuffer, ComputePipeline, ComputePipelineBuilder, ShaderCache, ValidationMode,
    VulkanContext,
};
use std::ffi::CString;
use std::path::PathBuf;
use std::process::ExitCode;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TEST_SCREEN_WIDTH: u32 = 64;
const TEST_SCREEN_HEIGHT: u32 = 64;
const MAX_SHADOW_VALIDATE_RESULTS: u32 = 16;

// ---------------------------------------------------------------------------
// Helpers
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
            return path;
        }
    }

    log::warn!("Could not find shader directory, defaulting to resources/shaders");
    PathBuf::from("resources/shaders")
}

fn identity_mat4() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn reverse_z_proj(fov: f32, aspect: f32, near: f32) -> [f32; 16] {
    let f = 1.0 / (fov.to_radians() * 0.5).tan();
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        -f,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        -1.0,
        0.0,
        0.0,
        near,
        0.0,
    ]
}

fn mat4_det(m: &[f32; 16]) -> f32 {
    let sub_det =
        |r0: f32, r1: f32, r2: f32, r3: f32, r4: f32, r5: f32, r6: f32, r7: f32, r8: f32| -> f32 {
            r0 * (r4 * r8 - r5 * r7) - r1 * (r3 * r8 - r5 * r6) + r2 * (r3 * r7 - r4 * r6)
        };

    let a = m[0];
    let b = m[1];
    let c = m[2];
    let d = m[3];

    a * sub_det(m[5], m[6], m[7], m[9], m[10], m[11], m[13], m[14], m[15])
        - b * sub_det(m[4], m[6], m[7], m[8], m[10], m[11], m[12], m[14], m[15])
        + c * sub_det(m[4], m[5], m[7], m[8], m[9], m[11], m[12], m[13], m[15])
        - d * sub_det(m[4], m[5], m[6], m[8], m[9], m[10], m[12], m[13], m[14])
}

fn vec3_len(v: &[f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

// ---------------------------------------------------------------------------
// Staging buffer for GPU readback
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
            .usage(vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC)
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

    #[allow(dead_code)]
    fn read_f32_slice(&self, context: &VulkanContext, count: usize) -> Result<Vec<f32>, String> {
        context.invalidate_mapped_memory(&self.allocation, 0, self.size);
        let mut result = vec![0.0f32; count];
        if let Some(mapped) = self.allocation.mapped_ptr() {
            let src = mapped.as_ptr() as *const f32;
            unsafe {
                std::ptr::copy_nonoverlapping(src, result.as_mut_ptr(), count);
            }
        } else {
            return Err("Staging buffer not mapped".to_string());
        }
        Ok(result)
    }

    fn read_u32_slice(&self, context: &VulkanContext, count: usize) -> Result<Vec<u32>, String> {
        context.invalidate_mapped_memory(&self.allocation, 0, self.size);
        let mut result = vec![0u32; count];
        if let Some(mapped) = self.allocation.mapped_ptr() {
            let src = mapped.as_ptr() as *const u32;
            unsafe {
                std::ptr::copy_nonoverlapping(src, result.as_mut_ptr(), count);
            }
        } else {
            return Err("Staging buffer not mapped".to_string());
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Dispatch + readback helpers
// ---------------------------------------------------------------------------

fn submit_and_wait(context: &VulkanContext, cmd_buf: &CommandBuffer) -> Result<(), String> {
    let cmd = cmd_buf.vk_command_buffer();

    let fence_info = vk::FenceCreateInfo::default();
    let fence = unsafe {
        context
            .device
            .create_fence(&fence_info, None)
            .map_err(|e| format!("Failed to create fence: {:?}", e))?
    };

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
    Ok(())
}

fn record_copy_buffer(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Buffer,
    dst: vk::Buffer,
    size: u64,
) {
    let region = vk::BufferCopy::default()
        .src_offset(0)
        .dst_offset(0)
        .size(size);
    unsafe {
        device.cmd_copy_buffer(cmd, src, dst, &[region]);
    }
}

// ---------------------------------------------------------------------------
// Cascade validation (CPU-only)
// ---------------------------------------------------------------------------

fn validate_cascade_split_ordering(gpu_data: &ShadowFrameData) -> Result<(), String> {
    let num_cascades = gpu_data.light_direction[3] as usize;
    for i in 1..num_cascades {
        let prev = gpu_data.cascades[i - 1].split_distance;
        let curr = gpu_data.cascades[i].split_distance;
        if curr <= prev {
            return Err(format!(
                "Cascade split {} ({:.4}) not strictly greater than split {} ({:.4})",
                i,
                curr,
                i - 1,
                prev
            ));
        }
    }
    log::info!("  PASSED: cascade splits are strictly increasing");
    Ok(())
}

fn validate_cascade_view_proj(gpu_data: &ShadowFrameData) -> Result<(), String> {
    let num_cascades = gpu_data.light_direction[3] as usize;
    for i in 0..num_cascades {
        let vp = gpu_data.cascades[i].view_proj;
        for (j, &v) in vp.iter().enumerate() {
            if v.is_nan() {
                return Err(format!("Cascade {} view_proj[{}] is NaN", i, j));
            }
            if v.is_infinite() {
                return Err(format!("Cascade {} view_proj[{}] is Inf", i, j));
            }
        }
        let det = mat4_det(&vp);
        if det.abs() < 1e-10 {
            return Err(format!(
                "Cascade {} view_proj has near-zero determinant ({:.6})",
                i, det
            ));
        }
    }
    log::info!("  PASSED: all cascade view_proj matrices are valid (no NaN/Inf, det != 0)");
    Ok(())
}

fn validate_cascade_texel_size(
    gpu_data: &ShadowFrameData,
    shadow_map_size: u32,
) -> Result<(), String> {
    let expected = 1.0 / shadow_map_size as f32;
    let num_cascades = gpu_data.light_direction[3] as usize;
    for i in 0..num_cascades {
        let texel = gpu_data.cascades[i].texel_size;
        if (texel - expected).abs() > 1e-8 {
            return Err(format!(
                "Cascade {} texel_size = {} (expected {})",
                i, texel, expected
            ));
        }
    }
    log::info!("  PASSED: all cascade texel_size = {}", expected);
    Ok(())
}

fn validate_cascade_light_direction(gpu_data: &ShadowFrameData) -> Result<(), String> {
    let dir = [
        gpu_data.light_direction[0],
        gpu_data.light_direction[1],
        gpu_data.light_direction[2],
    ];
    let len = vec3_len(&dir);
    if (len - 1.0).abs() > 1e-5 {
        return Err(format!(
            "Light direction not normalized: length = {:.8}, dir = ({:.4}, {:.4}, {:.4})",
            len, dir[0], dir[1], dir[2]
        ));
    }
    log::info!(
        "  PASSED: light direction is normalized (length = {:.8})",
        len
    );
    Ok(())
}

fn validate_cascade_frustum_coverage(
    gpu_data: &ShadowFrameData,
    near: f32,
    max_distance: f32,
) -> Result<(), String> {
    let num_cascades = gpu_data.light_direction[3] as usize;
    if num_cascades == 0 {
        return Err("No cascades".to_string());
    }
    let first_split = gpu_data.cascades[0].split_distance;
    if first_split <= near {
        return Err(format!(
            "First cascade split ({:.4}) does not cover beyond near ({:.4})",
            first_split, near
        ));
    }
    let last_split = gpu_data.cascades[num_cascades - 1].split_distance;
    if last_split < max_distance * 0.9 {
        return Err(format!(
            "Last cascade split ({:.4}) does not reach max_distance ({:.4})",
            last_split, max_distance
        ));
    }
    log::info!(
        "  PASSED: cascades cover [{:.4}, {:.4}] (near={:.4}, max_dist={:.4})",
        first_split,
        last_split,
        near,
        max_distance
    );
    Ok(())
}

fn validate_cascade_different_light_directions() -> Result<(), String> {
    log::info!("Testing cascade output changes with different light directions...");

    let params = CascadeParams {
        num_cascades: 4,
        lambda: 0.65,
        max_distance: 50.0,
        shadow_map_size: 1024,
        depth_bias_constant: 1.5,
        depth_bias_slope: 2.0,
    };
    let view = identity_mat4();
    let proj = reverse_z_proj(60.0, 16.0 / 9.0, 0.1);

    let mut csm1 = CascadeShadowMap::new(params.clone());
    csm1.update([0.5, -0.8, -0.3], &view, &proj);
    let gpu1 = csm1.gpu_data();

    let mut csm2 = CascadeShadowMap::new(params);
    csm2.update([-0.3, -0.9, 0.1], &view, &proj);
    let gpu2 = csm2.gpu_data();

    let mut any_diff = false;
    for i in 0..4 {
        for j in 0..16 {
            if (gpu1.cascades[i].view_proj[j] - gpu2.cascades[i].view_proj[j]).abs() > 1e-6 {
                any_diff = true;
            }
        }
    }

    if !any_diff {
        return Err("Cascade output is identical for different light directions".to_string());
    }
    log::info!("  PASSED: cascade output differs for different light directions");
    Ok(())
}

fn validate_cascade_camera_movement() -> Result<(), String> {
    log::info!("Testing cascade output changes with camera movement...");

    let params = CascadeParams {
        num_cascades: 4,
        lambda: 0.65,
        max_distance: 50.0,
        shadow_map_size: 1024,
        depth_bias_constant: 1.5,
        depth_bias_slope: 2.0,
    };
    let light_dir = [0.5, -0.8, -0.3];
    let proj = reverse_z_proj(60.0, 16.0 / 9.0, 0.1);

    // Camera at origin
    let view1 = identity_mat4();
    let mut csm1 = CascadeShadowMap::new(params.clone());
    csm1.update(light_dir, &view1, &proj);
    let gpu1 = csm1.gpu_data();

    // Camera offset
    let view2: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 10.0, 1.0,
    ];
    let mut csm2 = CascadeShadowMap::new(params);
    csm2.update(light_dir, &view2, &proj);
    let gpu2 = csm2.gpu_data();

    let mut any_diff = false;
    for i in 0..4 {
        for j in 0..16 {
            if (gpu1.cascades[i].view_proj[j] - gpu2.cascades[i].view_proj[j]).abs() > 1e-6 {
                any_diff = true;
            }
        }
    }

    if !any_diff {
        return Err("Cascade output is identical for different camera positions".to_string());
    }
    log::info!("  PASSED: cascade output differs for different camera positions");
    Ok(())
}

// ---------------------------------------------------------------------------
// Light culling GPU validation
// ---------------------------------------------------------------------------

struct LightCullingTestResources {
    lc_buffers: LightCullingBuffers,
    compute_pipeline: ComputePipeline,
}

fn create_light_culling_resources(
    context: &std::rc::Rc<VulkanContext>,
    shader_dir: &PathBuf,
) -> Result<LightCullingTestResources, String> {
    let lc_buffers =
        LightCullingBuffers::new(context.clone(), TEST_SCREEN_WIDTH, TEST_SCREEN_HEIGHT)
            .map_err(|e| format!("Failed to create LightCullingBuffers: {}", e))?;

    let compute_layout = lc_buffers
        .compute_descriptor_layout()
        .ok_or("Compute descriptor layout not created")?;

    let mut shader_cache = ShaderCache::new(context.device.clone());
    let lc_shader_path = shader_dir.join("lighting/light_cull.wgsl");
    let compute_shader = shader_cache
        .load_shader(&lc_shader_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load light culling shader: {}", e))?;

    let pipeline = ComputePipelineBuilder::new(context.clone())
        .with_shader(katla_gfx::sync::VkShaderModule(compute_shader))
        .add_descriptor_layout(VkDescriptorSetLayout(compute_layout))
        .build()
        .map_err(|e| format!("Failed to build light culling pipeline: {:?}", e))?;

    Ok(LightCullingTestResources {
        lc_buffers,
        compute_pipeline: pipeline,
    })
}

fn dispatch_light_culling_and_readback(
    context: &VulkanContext,
    resources: &mut LightCullingTestResources,
    lights: &[PointLightGPU],
    view_matrix: &[f32; 16],
    proj_matrix: &[f32; 16],
) -> Result<(Vec<u32>, Vec<u32>), String> {
    let lc = &mut resources.lc_buffers;
    let pipeline = &resources.compute_pipeline;
    let device = &context.device;

    lc.upload_lights(lights);

    let light_count = lights.len().min(256) as u32;
    let frame_data = LightCullFrameData {
        view_matrix: *view_matrix,
        proj_matrix: *proj_matrix,
        light_count,
        tiles_x: lc.tiles_x(),
        tiles_y: lc.tiles_y(),
        screen_width: lc.screen_width(),
        screen_height: lc.screen_height(),
        _pad0: 0,
        _pad1: 0,
        _pad2: 0,
    };
    lc.write_frame_data(&frame_data);

    let tiles_x = lc.tiles_x();
    let tiles_y = lc.tiles_y();
    let num_tiles = tiles_x * tiles_y;
    let tile_index_size = (num_tiles as u64) * (128u64) * 4;
    let tile_header_size = (num_tiles as u64) * 4;

    let header_staging = StagingBuffer::new(context, tile_header_size, "lc_header_readback")?;
    let index_staging = StagingBuffer::new(context, tile_index_size, "lc_index_readback")?;

    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();

    unsafe {
        lc.record_clear_tile_headers(cmd);

        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            pipeline.pipeline().into(),
        );

        lc.push_compute_descriptors(cmd, pipeline.pipeline_layout().into())?;

        if light_count > 0 {
            device.cmd_dispatch(cmd, tiles_x, tiles_y, 1);
        }

        // Barrier: compute -> copy
        let compute_to_copy = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(lc.tile_header_buffer())
            .offset(0)
            .size(vk::WHOLE_SIZE);

        let compute_to_copy_idx = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(lc.tile_index_buffer())
            .offset(0)
            .size(vk::WHOLE_SIZE);

        let buf_barriers = [compute_to_copy, compute_to_copy_idx];
        let dep_info = vk::DependencyInfo::default().buffer_memory_barriers(&buf_barriers);
        device.cmd_pipeline_barrier2(cmd, &dep_info);

        // Copy tile headers
        record_copy_buffer(
            device,
            cmd,
            lc.tile_header_buffer(),
            header_staging.buffer,
            tile_header_size,
        );

        // Copy tile indices
        record_copy_buffer(
            device,
            cmd,
            lc.tile_index_buffer(),
            index_staging.buffer,
            tile_index_size,
        );

        // Barrier: copy -> host read
        let copy_to_host_h = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(header_staging.buffer)
            .offset(0)
            .size(tile_header_size);

        let copy_to_host_i = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(index_staging.buffer)
            .offset(0)
            .size(tile_index_size);

        let host_barriers = [copy_to_host_h, copy_to_host_i];
        let dep_info2 = vk::DependencyInfo::default().buffer_memory_barriers(&host_barriers);
        device.cmd_pipeline_barrier2(cmd, &dep_info2);
    }

    submit_and_wait(context, &cmd_buf)?;

    let tile_counts = header_staging.read_u32_slice(context, num_tiles as usize)?;
    let tile_indices = index_staging.read_u32_slice(context, (tile_index_size / 4) as usize)?;

    Ok((tile_counts, tile_indices))
}

fn test_no_lights(
    context: &VulkanContext,
    resources: &mut LightCullingTestResources,
) -> Result<(), String> {
    log::info!("Testing light culling with zero lights...");

    let view = identity_mat4();
    let proj = reverse_z_proj(
        60.0,
        TEST_SCREEN_WIDTH as f32 / TEST_SCREEN_HEIGHT as f32,
        0.1,
    );

    let (tile_counts, _) =
        dispatch_light_culling_and_readback(context, resources, &[], &view, &proj)?;

    let non_zero = tile_counts.iter().filter(|&&c| c > 0).count();
    if non_zero > 0 {
        return Err(format!(
            "Expected all tile counts == 0 with no lights, but {} tiles have count > 0",
            non_zero
        ));
    }
    log::info!("  PASSED: all {} tiles have count == 0", tile_counts.len());
    Ok(())
}

fn test_single_light_centered(
    context: &VulkanContext,
    resources: &mut LightCullingTestResources,
) -> Result<(), String> {
    log::info!("Testing light culling with single centered light...");

    let view = identity_mat4();
    let proj = reverse_z_proj(
        60.0,
        TEST_SCREEN_WIDTH as f32 / TEST_SCREEN_HEIGHT as f32,
        0.1,
    );

    // Light at the origin (camera position), range 100 — should cover all tiles
    let lights = [PointLightGPU {
        position: [0.0, 0.0, 0.0],
        range: 100.0,
        color: [1.0, 0.0, 0.0],
        intensity: 1.0,
    }];

    let (tile_counts, tile_indices) =
        dispatch_light_culling_and_readback(context, resources, &lights, &view, &proj)?;

    let lit_tiles = tile_counts.iter().filter(|&&c| c > 0).count();
    let total_tiles = tile_counts.len();

    if lit_tiles == 0 {
        return Err("No tiles lit for a light at the camera origin with range 100".to_string());
    }

    // With a light at origin and range 100, most tiles should be lit
    let lit_ratio = lit_tiles as f32 / total_tiles as f32;
    if lit_ratio < 0.5 {
        return Err(format!(
            "Only {:.1}% of tiles lit (expected >50%) for centered light with range 100",
            lit_ratio * 100.0
        ));
    }

    // Verify that light index 0 appears in the tile indices
    let has_light_0 = tile_indices.iter().any(|&idx| idx == 0);
    if !has_light_0 {
        return Err("Light index 0 not found in any tile's light index list".to_string());
    }

    log::info!(
        "  PASSED: {}/{} tiles lit ({:.0}%), light index 0 found in tile data",
        lit_tiles,
        total_tiles,
        lit_ratio * 100.0
    );
    Ok(())
}

fn test_out_of_range_light(
    context: &VulkanContext,
    resources: &mut LightCullingTestResources,
) -> Result<(), String> {
    log::info!("Testing light culling with out-of-range light...");

    let view = identity_mat4();
    let proj = reverse_z_proj(
        60.0,
        TEST_SCREEN_WIDTH as f32 / TEST_SCREEN_HEIGHT as f32,
        0.1,
    );

    // Light far off to the side with very small range.
    // In view space (identity view), position (1000, 0, -5) is 5 units in front
    // but 1000 units to the right. The projected position will be way off screen.
    let lights = [PointLightGPU {
        position: [1000.0, 0.0, -5.0],
        range: 0.01,
        color: [1.0, 0.0, 0.0],
        intensity: 1.0,
    }];

    let (tile_counts, _) =
        dispatch_light_culling_and_readback(context, resources, &lights, &view, &proj)?;

    let lit_tiles = tile_counts.iter().filter(|&&c| c > 0).count();
    if lit_tiles > 0 {
        return Err(format!(
            "Expected 0 lit tiles for out-of-range light, but {} tiles lit",
            lit_tiles
        ));
    }
    log::info!("  PASSED: 0 tiles lit for out-of-range light");
    Ok(())
}

fn test_multiple_lights_overlapping(
    context: &VulkanContext,
    resources: &mut LightCullingTestResources,
) -> Result<(), String> {
    log::info!("Testing light culling with multiple overlapping lights...");

    let view = identity_mat4();
    let proj = reverse_z_proj(
        60.0,
        TEST_SCREEN_WIDTH as f32 / TEST_SCREEN_HEIGHT as f32,
        0.1,
    );

    // Two lights at camera origin with large range — both should light all tiles
    let lights = [
        PointLightGPU {
            position: [0.0, 0.0, 0.0],
            range: 100.0,
            color: [1.0, 0.0, 0.0],
            intensity: 1.0,
        },
        PointLightGPU {
            position: [1.0, 0.0, 0.0],
            range: 100.0,
            color: [0.0, 1.0, 0.0],
            intensity: 1.0,
        },
    ];

    let (tile_counts, tile_indices) =
        dispatch_light_culling_and_readback(context, resources, &lights, &view, &proj)?;

    // Center tiles should have count >= 2
    let tiles_x = TEST_SCREEN_WIDTH / TILE_SIZE;
    let tiles_y = TEST_SCREEN_HEIGHT / TILE_SIZE;
    let center_tile = (tiles_y / 2) * tiles_x + (tiles_x / 2);
    let center_count = tile_counts[center_tile as usize];

    if center_count < 2 {
        return Err(format!(
            "Center tile expected >= 2 lights, got {}",
            center_count
        ));
    }

    // Verify both light indices appear
    let has_0 = tile_indices.iter().any(|&idx| idx == 0);
    let has_1 = tile_indices.iter().any(|&idx| idx == 1);
    if !has_0 || !has_1 {
        return Err(format!(
            "Missing light indices: has_0={}, has_1={}",
            has_0, has_1
        ));
    }

    log::info!(
        "  PASSED: center tile has {} lights, both indices found",
        center_count
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Shadow sampling GPU validation
// ---------------------------------------------------------------------------

struct ShadowValidateResources {
    // Storage buffer for ShadowFrameData
    shadow_data_buffer: vk::Buffer,
    shadow_data_allocation: Option<Allocation>,
    // 1x1 depth texture (shadow atlas)
    depth_image: vk::Image,
    #[allow(dead_code)]
    depth_allocation: Option<Allocation>,
    #[allow(dead_code)]
    depth_image_view: vk::ImageView,
    // Output buffer (readback)
    output_buffer: vk::Buffer,
    output_allocation: Option<Allocation>,
    // Test params uniform buffer
    test_params_buffer: vk::Buffer,
    test_params_allocation: Option<Allocation>,
    // Compute pipeline
    pipeline: ComputePipeline,
}

fn create_shadow_validate_resources(
    context: &std::rc::Rc<VulkanContext>,
    shader_dir: &PathBuf,
) -> Result<ShadowValidateResources, String> {
    use gpu_allocator::vulkan::AllocationCreateDesc;

    let device = &context.device;

    // Shadow data storage buffer
    let shadow_data_size = std::mem::size_of::<ShadowFrameData>() as u64;
    let shadow_data_info = vk::BufferCreateInfo::default()
        .size(shadow_data_size)
        .usage(vk::BufferUsageFlags::STORAGE_BUFFER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let shadow_data_buffer = unsafe {
        device
            .create_buffer(&shadow_data_info, None)
            .map_err(|e| format!("Failed to create shadow data buffer: {:?}", e))?
    };
    let shadow_data_reqs = unsafe { device.get_buffer_memory_requirements(shadow_data_buffer) };
    let shadow_data_allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name: "shadow_val_data",
            requirements: shadow_data_reqs,
            location: gpu_allocator::MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate shadow data memory: {}", e))?;

    unsafe {
        device
            .bind_buffer_memory(
                shadow_data_buffer,
                shadow_data_allocation.memory(),
                shadow_data_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind shadow data buffer: {:?}", e))?;
    }

    // 2x2 depth texture (matches real shadow atlas 2x2 cascade layout)
    let depth_image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::D32_SFLOAT)
        .extent(vk::Extent3D {
            width: 2,
            height: 2,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let (depth_image, depth_allocation) =
        context.create_image(depth_image_info, gpu_allocator::MemoryLocation::GpuOnly);

    // Transition to SHADER_READ
    let cmd = context.begin_single_time_commands();
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_READ)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image(depth_image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    let barriers = [barrier];
    let dep_info = vk::DependencyInfo::default().image_memory_barriers(&barriers);
    unsafe {
        device.cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dep_info);
    }
    context.end_single_time_commands(cmd);

    let depth_image_view = unsafe {
        device
            .create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(depth_image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::D32_SFLOAT)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::DEPTH,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )
            .map_err(|e| format!("Failed to create depth image view: {:?}", e))?
    };

    // Output buffer (CPU-visible)
    let output_size = (MAX_SHADOW_VALIDATE_RESULTS as u64) * 4;
    let output_info = vk::BufferCreateInfo::default()
        .size(output_size)
        .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let output_buffer = unsafe {
        device
            .create_buffer(&output_info, None)
            .map_err(|e| format!("Failed to create output buffer: {:?}", e))?
    };
    let output_reqs = unsafe { device.get_buffer_memory_requirements(output_buffer) };
    let output_allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name: "shadow_val_output",
            requirements: output_reqs,
            location: gpu_allocator::MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate output memory: {}", e))?;

    unsafe {
        device
            .bind_buffer_memory(
                output_buffer,
                output_allocation.memory(),
                output_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind output buffer: {:?}", e))?;
    }

    // Test params uniform buffer (32 bytes: vec3f + f32 + u32 + padding)
    let test_params_size: u64 = 32;
    let test_params_info = vk::BufferCreateInfo::default()
        .size(test_params_size)
        .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let test_params_buffer = unsafe {
        device
            .create_buffer(&test_params_info, None)
            .map_err(|e| format!("Failed to create test params buffer: {:?}", e))?
    };
    let test_params_reqs = unsafe { device.get_buffer_memory_requirements(test_params_buffer) };
    let test_params_allocation = context
        .allocator
        .borrow_mut()
        .allocate(&AllocationCreateDesc {
            name: "shadow_val_params",
            requirements: test_params_reqs,
            location: gpu_allocator::MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| format!("Failed to allocate test params memory: {}", e))?;

    unsafe {
        device
            .bind_buffer_memory(
                test_params_buffer,
                test_params_allocation.memory(),
                test_params_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind test params buffer: {:?}", e))?;
    }

    // Create compute pipeline with push descriptors
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
    ];

    let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
        .bindings(&bindings)
        .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

    let descriptor_layout = unsafe {
        device
            .create_descriptor_set_layout(&layout_info, None)
            .map_err(|e| format!("Failed to create descriptor layout: {:?}", e))?
    };

    let mut shader_cache = ShaderCache::new(device.clone());
    let validate_path = shader_dir.join("lighting/shadow_validate.wgsl");
    let shader = shader_cache
        .load_shader(&validate_path, vk::ShaderStageFlags::COMPUTE)
        .map_err(|e| format!("Failed to load shadow validate shader: {}", e))?;

    let pipeline = ComputePipelineBuilder::new(context.clone())
        .with_shader(katla_gfx::sync::VkShaderModule(shader))
        .add_descriptor_layout(VkDescriptorSetLayout(descriptor_layout))
        .build()
        .map_err(|e| format!("Failed to build shadow validate pipeline: {:?}", e))?;

    Ok(ShadowValidateResources {
        shadow_data_buffer,
        shadow_data_allocation: Some(shadow_data_allocation),
        depth_image,
        depth_allocation: Some(depth_allocation),
        depth_image_view,
        output_buffer,
        output_allocation: Some(output_allocation),
        test_params_buffer,
        test_params_allocation: Some(test_params_allocation),
        pipeline,
    })
}

fn upload_shadow_data(
    context: &VulkanContext,
    res: &ShadowValidateResources,
    data: &ShadowFrameData,
) {
    if let Some(ref alloc) = res.shadow_data_allocation {
        if let Some(mapped) = alloc.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data as *const ShadowFrameData as *const u8,
                    mapped.as_ptr() as *mut u8,
                    std::mem::size_of::<ShadowFrameData>(),
                );
            }
            context.flush_mapped_memory(alloc, 0, std::mem::size_of::<ShadowFrameData>() as u64);
        }
    }
}

fn clear_depth_to(
    context: &VulkanContext,
    res: &ShadowValidateResources,
    depth: f32,
) -> Result<(), String> {
    let cmd = context.begin_single_time_commands();
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_READ)
        .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .image(res.depth_image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    let img_barriers = [barrier];
    let dep_info = vk::DependencyInfo::default().image_memory_barriers(&img_barriers);
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dep_info);
    }
    context.end_single_time_commands(cmd);

    // Clear the entire 2x2 atlas using vkCmdClearDepthImage
    let cmd = context.begin_single_time_commands();
    let clear_value = vk::ClearDepthStencilValue { depth, stencil: 0 };
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::DEPTH,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    unsafe {
        context.device.cmd_clear_depth_stencil_image(
            cmd.vk_command_buffer(),
            res.depth_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &clear_value,
            &[range],
        );
    }
    context.end_single_time_commands(cmd);

    // Transition back to SHADER_READ
    let cmd = context.begin_single_time_commands();
    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image(res.depth_image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    let img_barriers2 = [barrier];
    let dep_info = vk::DependencyInfo::default().image_memory_barriers(&img_barriers2);
    unsafe {
        context
            .device
            .cmd_pipeline_barrier2(cmd.vk_command_buffer(), &dep_info);
    }
    context.end_single_time_commands(cmd);

    Ok(())
}

fn dispatch_shadow_validate(
    context: &VulkanContext,
    res: &ShadowValidateResources,
    test_world_pos: [f32; 3],
    test_view_z: f32,
    test_index: u32,
) -> Result<f32, String> {
    let device = &context.device;
    let shadow_data_size = std::mem::size_of::<ShadowFrameData>() as u64;
    let output_size = (MAX_SHADOW_VALIDATE_RESULTS as u64) * 4;

    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();

    unsafe {
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            res.pipeline.pipeline().into(),
        );

        // Push descriptors
        let push_descriptor = context
            .push_descriptor_khr
            .as_ref()
            .ok_or("VK_KHR_push_descriptor not available")?;

        let shadow_data_info = [vk::DescriptorBufferInfo::default()
            .buffer(res.shadow_data_buffer)
            .offset(0)
            .range(shadow_data_size)];

        let image_info = [vk::DescriptorImageInfo::default()
            .image_view(res.depth_image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];

        let output_info = [vk::DescriptorBufferInfo::default()
            .buffer(res.output_buffer)
            .offset(0)
            .range(output_size)];

        let params_buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(res.test_params_buffer)
            .offset(0)
            .range(32)];

        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&shadow_data_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(1)
                .image_info(&image_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .buffer_info(&output_info),
            vk::WriteDescriptorSet::default()
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .buffer_info(&params_buffer_info),
        ];

        push_descriptor.cmd_push_descriptor_set(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            res.pipeline.pipeline_layout().into(),
            0,
            &writes,
        );

        // Write test params to uniform buffer
        if let Some(ref alloc) = res.test_params_allocation {
            if let Some(mapped) = alloc.mapped_ptr() {
                let mut params_data = [0u8; 32];
                let world_pos = [
                    test_world_pos[0],
                    test_world_pos[1],
                    test_world_pos[2],
                    test_view_z,
                ];
                let world_pos_bytes: &[u8] = bytemuck::cast_slice(&world_pos);
                params_data[..16].copy_from_slice(world_pos_bytes);
                params_data[16..20].copy_from_slice(&test_index.to_le_bytes());
                std::ptr::copy_nonoverlapping(params_data.as_ptr(), mapped.as_ptr() as *mut u8, 32);
                context.flush_mapped_memory(alloc, 0, 32);
            }
        }

        // Barrier: output buffer needs to be writable
        let output_barrier = vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::HOST_WRITE)
            .dst_access_mask(vk::AccessFlags2::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(res.output_buffer)
            .offset(0)
            .size(output_size);

        let out_barriers = [output_barrier];
        let dep = vk::DependencyInfo::default().buffer_memory_barriers(&out_barriers);
        device.cmd_pipeline_barrier2(cmd, &dep);

        device.cmd_dispatch(cmd, 1, 1, 1);
    }

    submit_and_wait(context, &cmd_buf)?;

    // Read back output
    if let Some(ref alloc) = res.output_allocation {
        context.invalidate_mapped_memory(alloc, 0, output_size);
        if let Some(mapped) = alloc.mapped_ptr() {
            let ptr = mapped.as_ptr() as *const f32;
            let result = unsafe { *ptr.add(test_index as usize) };
            return Ok(result);
        }
    }

    Err("Failed to read back shadow validation output".to_string())
}

fn test_shadow_fully_lit(
    context: &VulkanContext,
    res: &ShadowValidateResources,
) -> Result<(), String> {
    log::info!("Testing shadow sampling: fully lit (depth=1.0)...");

    clear_depth_to(context, res, 1.0)?;

    // Reverse-Z orthographic light VP mapping world (0,0,0) to NDC (0,0,0.5):
    //   light_space = VP * (0,0,0,1) = (0, 0, 0, 1)  -> proj = (0, 0, 0)
    //   uv = (0,0)*0.5 + 0.5 = (0.5, 0.5)
    //   depth = 0*0.5 + 0.5 = 0.5
    // Cascade 0 quadrant: offset (0, 0.5), scale (0.5, 0.5).
    //   atlas_uv = (0, 0.5) + (0.5, 0.5)*(0.5, 0.5) = (0.25, 0.75)
    //   coords = floor((0.25, 0.75) * 2) = (0, 1) -> top-left quadrant texel.
    // compare_depth = 0.5. stored_depth = 1.0. 0.5 <= 1.0 -> lit.
    let vp: [f32; 16] = [
        0.5, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0,
    ];

    let gpu_data = ShadowFrameData {
        cascades: [ShadowCascadeGPU {
            view_proj: vp,
            split_distance: 10.0,
            texel_size: 0.5,
            _pad: [0.0, 0.0],
        }; 4],
        light_direction: [0.0, -1.0, 0.0, 4.0],
        shadow_bias: [0.0, 0.0, 0.0, 0.0],
    };
    upload_shadow_data(context, res, &gpu_data);

    let visibility = dispatch_shadow_validate(context, res, [0.0, 0.0, 0.0], 1.0, 0)?;

    log::info!("  visibility = {:.4}", visibility);
    if visibility < 0.95 {
        return Err(format!(
            "Expected visibility >= 0.95 with depth=1.0, got {:.4}",
            visibility
        ));
    }
    log::info!("  PASSED: visibility = {:.4} (fully lit)", visibility);
    Ok(())
}

fn test_shadow_fully_shadowed(
    context: &VulkanContext,
    res: &ShadowValidateResources,
) -> Result<(), String> {
    log::info!("Testing shadow sampling: fully shadowed (depth=0.0)...");

    clear_depth_to(context, res, 0.0)?;

    // Same VP as test_shadow_fully_lit.
    // compare_depth = 0.5. stored_depth = 0.0. 0.5 <= 0.0 -> shadowed.
    let vp: [f32; 16] = [
        0.5, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0,
    ];

    let gpu_data = ShadowFrameData {
        cascades: [ShadowCascadeGPU {
            view_proj: vp,
            split_distance: 10.0,
            texel_size: 0.5,
            _pad: [0.0, 0.0],
        }; 4],
        light_direction: [0.0, -1.0, 0.0, 4.0],
        shadow_bias: [0.0, 0.0, 0.0, 0.0],
    };
    upload_shadow_data(context, res, &gpu_data);

    let visibility = dispatch_shadow_validate(context, res, [0.0, 0.0, 0.0], 1.0, 1)?;

    log::info!("  visibility = {:.4}", visibility);
    if visibility > 0.05 {
        return Err(format!(
            "Expected visibility <= 0.05 with depth=0.0, got {:.4}",
            visibility
        ));
    }
    log::info!("  PASSED: visibility = {:.4} (fully shadowed)", visibility);
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("=== Light + Shadow Validation ===");

    let app_name = CString::new("Light Shadow Validation").unwrap();
    let engine_name = CString::new("Katla Engine").unwrap();

    log::info!("Creating headless Vulkan context...");
    let context = VulkanContext::init_headless(ValidationMode::GpuAssisted, app_name, engine_name);
    let context = std::rc::Rc::new(context);
    log::info!("Vulkan context created successfully");

    let shader_dir = find_shader_directory();
    log::info!("Using shader directory: {:?}", shader_dir);

    let mut failed = false;

    // ========================================================================
    // Phase 1: Cascade Shadow Map (CPU-only)
    // ========================================================================
    log::info!("");
    log::info!("--- Cascade Shadow Map Validation (CPU) ---");

    let cascade_params = CascadeParams {
        num_cascades: 4,
        lambda: 0.65,
        max_distance: 100.0,
        shadow_map_size: 2048,
        depth_bias_constant: 1.5,
        depth_bias_slope: 2.0,
    };

    let view = identity_mat4();
    let proj = reverse_z_proj(60.0, 16.0 / 9.0, 0.1);
    let light_dir = [0.5, -0.8, -0.3];

    let mut csm = CascadeShadowMap::new(cascade_params.clone());
    csm.update(light_dir, &view, &proj);
    let gpu_data = csm.gpu_data();

    for (name, result) in [
        ("split_ordering", validate_cascade_split_ordering(&gpu_data)),
        ("view_proj_valid", validate_cascade_view_proj(&gpu_data)),
        (
            "texel_size",
            validate_cascade_texel_size(&gpu_data, cascade_params.shadow_map_size),
        ),
        (
            "light_direction_normalized",
            validate_cascade_light_direction(&gpu_data),
        ),
        (
            "frustum_coverage",
            validate_cascade_frustum_coverage(&gpu_data, 0.1, cascade_params.max_distance),
        ),
        (
            "different_light_directions",
            validate_cascade_different_light_directions(),
        ),
        ("camera_movement", validate_cascade_camera_movement()),
    ] {
        if let Err(e) = result {
            log::error!("FAIL: {}: {}", name, e);
            failed = true;
        }
    }

    // ========================================================================
    // Phase 2: Light Culling (GPU)
    // ========================================================================
    log::info!("");
    log::info!("--- Light Culling Validation (GPU) ---");

    let mut lc_resources: Option<LightCullingTestResources> =
        match create_light_culling_resources(&context, &shader_dir) {
            Ok(r) => {
                log::info!("Light culling resources created");
                Some(r)
            }
            Err(e) => {
                log::error!("Failed to create light culling resources: {}", e);
                failed = true;
                None
            }
        };

    // Only run light culling tests if resources were created successfully
    if let Some(ref mut lc_resources) = lc_resources {
        for (name, result) in [
            ("no_lights", test_no_lights(&context, lc_resources)),
            (
                "single_light_centered",
                test_single_light_centered(&context, lc_resources),
            ),
            (
                "out_of_range_light",
                test_out_of_range_light(&context, lc_resources),
            ),
            (
                "multiple_lights_overlapping",
                test_multiple_lights_overlapping(&context, lc_resources),
            ),
        ] {
            if let Err(e) = result {
                log::error!("FAIL: {}: {}", name, e);
                failed = true;
            }
        }
    }

    // ========================================================================
    // Phase 3: Shadow Sampling (GPU)
    // ========================================================================
    log::info!("");
    log::info!("--- Shadow Sampling Validation (GPU) ---");

    let shadow_res = match create_shadow_validate_resources(&context, &shader_dir) {
        Ok(r) => {
            log::info!("Shadow validate resources created");
            Some(r)
        }
        Err(e) => {
            log::error!("Failed to create shadow validate resources: {}", e);
            failed = true;
            None
        }
    };

    if let Some(ref res) = shadow_res {
        for (name, result) in [
            ("fully_lit", test_shadow_fully_lit(&context, res)),
            ("fully_shadowed", test_shadow_fully_shadowed(&context, res)),
        ] {
            if let Err(e) = result {
                log::error!("FAIL: {}: {}", name, e);
                failed = true;
            }
        }
    }

    if failed {
        log::error!("=== Validation FAILED ===");
        ExitCode::from(1)
    } else {
        log::info!("=== All Validations Passed ===");
        ExitCode::SUCCESS
    }
}
