// GPU Outline Validation Example
//
// Validates the stencil-based outline + wallhack overlay system by:
// - Creating a headless Vulkan context with validation enabled
// - Rendering two overlapping spheres with a depth prepass
// - Running the outline pipeline (stencil mark, occlusion mark, outline draw)
// - Running the stencil indicator pass (R8 texture where stencil == 2)
// - Running a simplified tonemap pass that applies the wallhack overlay tint
// - Reading back pixels and verifying correctness
//
// Exit codes:
// - 0: All validations passed
// - 1: One or more validations failed

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use katla_gfx::{CommandBuffer, ShaderCache, ValidationMode, VulkanContext};
use katla_gfx::{
    CompareOp, CullMode, FrontFace, ImageFormat, Pipeline, PipelineBuilder, VertexFormat,
};
use std::ffi::CString;
use std::path::PathBuf;
use std::process::ExitCode;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

fn find_shader_directory() -> PathBuf {
    for candidate in &[
        "resources/shaders",
        "../resources/shaders",
        "../../resources/shaders",
        "../../../resources/shaders",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("resources/shaders")
}

fn transition_image_layout(
    cmd: vk::CommandBuffer,
    device: &ash::Device,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    subresource_range: vk::ImageSubresourceRange,
) {
    let (src_stage, dst_stage, src_access, dst_access) = match (old_layout, new_layout) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::AccessFlags::NONE,
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags::NONE,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ,
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ,
            vk::AccessFlags::SHADER_READ,
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::TRANSFER_SRC_OPTIMAL) => (
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::TRANSFER_READ,
        ),
        (vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::ImageLayout::TRANSFER_SRC_OPTIMAL) => (
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::TRANSFER,
            vk::AccessFlags::SHADER_READ,
            vk::AccessFlags::TRANSFER_READ,
        ),
        (old, new) if old == new => (
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::AccessFlags::NONE,
            vk::AccessFlags::NONE,
        ),
        _ => return,
    };

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);

    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

fn depth_render_pass_sync(cmd: vk::CommandBuffer, device: &ash::Device, image: vk::Image) {
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
            base_mip_level: 0,
            level_count: vk::REMAINING_MIP_LEVELS,
            base_array_layer: 0,
            layer_count: vk::REMAINING_ARRAY_LAYERS,
        })
        .src_access_mask(
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        )
        .dst_access_mask(
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );

    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

fn submit_and_wait(context: &VulkanContext, cmd_buf: &CommandBuffer) -> Result<(), String> {
    let cmd = cmd_buf.vk_command_buffer();
    let fence = unsafe {
        context
            .device
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .map_err(|e| format!("Failed to create fence: {:?}", e))?
    };
    cmd_buf.end_single_time_command();
    unsafe {
        let submit = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));
        context
            .device
            .queue_submit(context.gfx_queue.vk_queue(), &[submit], fence)
            .map_err(|e| format!("Failed to submit: {}", e))?;
        context
            .device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|e| format!("Failed to wait for fence: {:?}", e))?;
        context.device.destroy_fence(fence, None);
    }
    cmd_buf.return_to_pool();
    Ok(())
}

// ---------------------------------------------------------------------------
// FrameUniforms and ObjectUniforms (must match WGSL structs)
// ---------------------------------------------------------------------------

#[repr(C, align(16))]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct FrameUniforms {
    view: [f32; 16],
    proj: [f32; 16],
    inv_view_proj: [f32; 16],
    camera_position: [f32; 4],
    light_direction: [f32; 4],
    light_color: [f32; 4],
    light_intensity: [f32; 4],
    tiles: [u32; 4],
}

#[repr(C, align(16))]
#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy)]
struct ObjectUniforms {
    model: [f32; 16],
    base_color: [f32; 4],
    material_params: [f32; 4],
    texture_indices: [u32; 4],
}

// ---------------------------------------------------------------------------
// Matrix math (minimal, no external dependency)
// ---------------------------------------------------------------------------

fn mat4_identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mat4_translate(tx: f32, ty: f32, tz: f32) -> [f32; 16] {
    let mut m = mat4_identity();
    m[12] = tx;
    m[13] = ty;
    m[14] = tz;
    m
}

fn mat4_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y / 2.0).tan();
    // Reverse-Z: map [near, far] to [1, 0]
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        near / (far - near),
        -1.0,
        0.0,
        0.0,
        (near * far) / (far - near),
        0.0,
    ]
}

fn mat4_look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = normalize3([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let s = cross3(f, up);
    let s = normalize3(s);
    let u = cross3(s, f);

    // Vulkan: Y-down, Z-forward (flip Y and Z compared to OpenGL)
    [
        s[0],
        -u[0],
        -f[0],
        0.0,
        s[1],
        -u[1],
        -f[1],
        0.0,
        s[2],
        -u[2],
        -f[2],
        0.0,
        -dot3(s, eye),
        dot3(u, eye),
        dot3(f, eye),
        1.0,
    ]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = sum;
        }
    }
    r
}

fn mat4_inverse(m: &[f32; 16]) -> [f32; 16] {
    // Column-major: m[col*4 + row]
    let m00 = m[0];
    let m01 = m[1];
    let m02 = m[2];
    let m03 = m[3];
    let m10 = m[4];
    let m11 = m[5];
    let m12 = m[6];
    let m13 = m[7];
    let m20 = m[8];
    let m21 = m[9];
    let m22 = m[10];
    let m23 = m[11];
    let m30 = m[12];
    let m31 = m[13];
    let m32 = m[14];
    let m33 = m[15];

    let a00 = m00 * m11 - m01 * m10;
    let a01 = m00 * m12 - m02 * m10;
    let a02 = m00 * m13 - m03 * m10;
    let a03 = m01 * m12 - m02 * m11;
    let a04 = m01 * m13 - m03 * m11;
    let a05 = m02 * m13 - m03 * m12;
    let a06 = m20 * m31 - m21 * m30;
    let a07 = m20 * m32 - m22 * m30;
    let a08 = m20 * m33 - m23 * m30;
    let a09 = m21 * m32 - m22 * m31;
    let a10 = m21 * m33 - m23 * m31;
    let a11 = m22 * m33 - m23 * m32;

    let det = a00 * a11 - a01 * a10 + a02 * a09 + a03 * a08 - a04 * a07 + a05 * a06;
    if det.abs() < 1e-10 {
        return mat4_identity();
    }
    let inv_det = 1.0 / det;

    [
        (m11 * a11 - m12 * a10 + m13 * a09) * inv_det,
        (m02 * a10 - m01 * a11 - m03 * a09) * inv_det,
        (m31 * a05 - m32 * a04 + m33 * a03) * inv_det,
        (m22 * a04 - m21 * a05 - m23 * a03) * inv_det,
        (m12 * a08 - m10 * a11 - m13 * a07) * inv_det,
        (m00 * a11 - m02 * a08 + m03 * a07) * inv_det,
        (m32 * a02 - m30 * a05 - m33 * a01) * inv_det,
        (m20 * a05 - m22 * a02 + m23 * a01) * inv_det,
        (m10 * a10 - m11 * a08 + m13 * a06) * inv_det,
        (m01 * a08 - m00 * a10 - m03 * a06) * inv_det,
        (m30 * a04 - m31 * a02 + m33 * a00) * inv_det,
        (m21 * a02 - m20 * a04 - m23 * a00) * inv_det,
        (m11 * a07 - m10 * a09 - m12 * a06) * inv_det,
        (m00 * a09 - m01 * a07 + m02 * a06) * inv_det,
        (m31 * a01 - m30 * a03 - m32 * a00) * inv_det,
        (m20 * a03 - m21 * a01 + m22 * a00) * inv_det,
    ]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

// ---------------------------------------------------------------------------
// UV Sphere generation
// ---------------------------------------------------------------------------

struct SphereMesh {
    positions: Vec<f32>,
    indices: Vec<u32>,
}

fn create_uv_sphere(radius: f32, lat_segments: u32, lon_segments: u32) -> SphereMesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    for lat in 0..=lat_segments {
        let theta = (lat as f32) * std::f32::consts::PI / (lat_segments as f32);
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for lon in 0..=lon_segments {
            let phi = (lon as f32) * 2.0 * std::f32::consts::PI / (lon_segments as f32);
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let x = cos_phi * sin_theta;
            let y = cos_theta;
            let z = sin_phi * sin_theta;

            positions.push(x * radius);
            positions.push(y * radius);
            positions.push(z * radius);
        }
    }

    for lat in 0..lat_segments {
        for lon in 0..lon_segments {
            let a = lat * (lon_segments + 1) + lon;
            let b = a + lon_segments + 1;

            indices.push(a);
            indices.push(b);
            indices.push(a + 1);

            indices.push(a + 1);
            indices.push(b);
            indices.push(b + 1);
        }
    }

    SphereMesh { positions, indices }
}

// ---------------------------------------------------------------------------
// GPU resources
// ---------------------------------------------------------------------------

struct GpuBuffer {
    buffer: vk::Buffer,
    allocation: Allocation,
}

struct ImageResources {
    depth_image: vk::Image,
    depth_allocation: Allocation,
    depth_view: vk::ImageView,
    depth_stencil_view: vk::ImageView,
    hdr_image: vk::Image,
    hdr_allocation: Allocation,
    hdr_view: vk::ImageView,
    indicator_image: vk::Image,
    indicator_allocation: Allocation,
    indicator_view: vk::ImageView,
    ldr_image: vk::Image,
    ldr_allocation: Allocation,
    ldr_view: vk::ImageView,
    picking_image: vk::Image,
    picking_allocation: Allocation,
    picking_view: vk::ImageView,
    ldr_staging: vk::Buffer,
    ldr_staging_allocation: Allocation,
    indicator_staging: vk::Buffer,
    indicator_staging_allocation: Allocation,
    depth_staging: vk::Buffer,
    depth_staging_allocation: Allocation,
    picking_staging: vk::Buffer,
    picking_staging_allocation: Allocation,
    stencil_staging: vk::Buffer,
    stencil_staging_allocation: Allocation,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct OutlinePushConstants {
    outline_width: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    outline_color: [f32; 4],
}

struct PipelineResources {
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    depth_prepass_pipeline: vk::Pipeline,
    stencil_mark_pipeline: Pipeline,
    occlusion_mark_pipeline: Pipeline,
    outline_draw_pipeline: Pipeline,
    stencil_indicator_pipeline: Pipeline,
    tonemap_pipeline: vk::Pipeline,
    tonemap_descriptor_set_layout: vk::DescriptorSetLayout,
    tonemap_pipeline_layout: vk::PipelineLayout,
    tonemap_descriptor_pool: vk::DescriptorPool,
    tonemap_descriptor_set: vk::DescriptorSet,
}

impl GpuBuffer {
    fn new(
        context: &VulkanContext,
        data: &[u8],
        usage: vk::BufferUsageFlags,
        name: &str,
    ) -> Result<Self, String> {
        let size = data.len() as u64;
        let buffer = unsafe {
            context
                .device
                .create_buffer(
                    &vk::BufferCreateInfo::default().size(size).usage(usage),
                    None,
                )
                .map_err(|e| format!("Failed to create buffer '{}': {:?}", name, e))?
        };
        let allocation = {
            let reqs = unsafe { context.device.get_buffer_memory_requirements(buffer) };
            context
                .allocator
                .borrow_mut()
                .allocate(&AllocationCreateDesc {
                    name,
                    requirements: reqs,
                    location: gpu_allocator::MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| format!("Failed to allocate '{}': {}", name, e))?
        };
        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind '{}': {:?}", name, e))?;
        }
        if !data.is_empty() {
            if let Some(mapped) = allocation.mapped_ptr() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        mapped.as_ptr() as *mut u8,
                        size as usize,
                    );
                }
            }
            context.flush_mapped_memory(&allocation, 0, size);
        }
        Ok(Self { buffer, allocation })
    }

    fn new_staging(context: &VulkanContext, size: u64, name: &str) -> Result<Self, String> {
        let buffer = unsafe {
            context
                .device
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(size)
                        .usage(vk::BufferUsageFlags::TRANSFER_DST),
                    None,
                )
                .map_err(|e| format!("Failed to create staging buffer '{}': {:?}", name, e))?
        };
        let allocation = {
            let reqs = unsafe { context.device.get_buffer_memory_requirements(buffer) };
            context
                .allocator
                .borrow_mut()
                .allocate(&AllocationCreateDesc {
                    name,
                    requirements: reqs,
                    location: gpu_allocator::MemoryLocation::GpuToCpu,
                    linear: true,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| format!("Failed to allocate staging '{}': {}", name, e))?
        };
        unsafe {
            context
                .device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .map_err(|e| format!("Failed to bind staging '{}': {:?}", name, e))?;
        }
        Ok(Self { buffer, allocation })
    }

    fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_buffer(self.buffer, None);
        }
        context
            .allocator
            .free(self.allocation, "outline validation buffer");
    }
}

fn create_image_resources(context: &VulkanContext) -> Result<ImageResources, String> {
    let device = &context.device;
    let extent_3d = vk::Extent3D {
        width: WIDTH,
        height: HEIGHT,
        depth: 1,
    };

    // Depth-stencil image (D32SfloatS8Uint)
    let depth_format = vk::Format::D32_SFLOAT_S8_UINT;
    let depth_image = unsafe {
        device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(depth_format)
                    .extent(extent_3d)
                    .mip_levels(1)
                    .array_layers(1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(
                        vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC,
                    ),
                None,
            )
            .map_err(|e| format!("Failed to create depth image: {:?}", e))?
    };
    let depth_allocation = {
        let reqs = unsafe { device.get_image_memory_requirements(depth_image) };
        context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "depth_image",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate depth image: {}", e))?
    };
    unsafe {
        device
            .bind_image_memory(
                depth_image,
                depth_allocation.memory(),
                depth_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind depth image: {:?}", e))?;
    }

    let depth_view = create_image_view(
        device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH,
    )?;
    let depth_stencil_view = create_image_view(
        device,
        depth_image,
        depth_format,
        vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
    )?;

    // HDR color image (R16G16B16A16Sfloat)
    let hdr_format = vk::Format::R16G16B16A16_SFLOAT;
    let hdr_image = unsafe {
        device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(hdr_format)
                    .extent(extent_3d)
                    .mip_levels(1)
                    .array_layers(1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::TRANSFER_DST
                            | vk::ImageUsageFlags::SAMPLED,
                    ),
                None,
            )
            .map_err(|e| format!("Failed to create HDR image: {:?}", e))?
    };
    let hdr_allocation = {
        let reqs = unsafe { device.get_image_memory_requirements(hdr_image) };
        context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "hdr_image",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate HDR image: {}", e))?
    };
    unsafe {
        device
            .bind_image_memory(hdr_image, hdr_allocation.memory(), hdr_allocation.offset())
            .map_err(|e| format!("Failed to bind HDR image: {:?}", e))?;
    }
    let hdr_view = create_image_view(device, hdr_image, hdr_format, vk::ImageAspectFlags::COLOR)?;

    // Stencil indicator image (R8Unorm)
    let indicator_format = vk::Format::R8_UNORM;
    let indicator_image = unsafe {
        device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(indicator_format)
                    .extent(extent_3d)
                    .mip_levels(1)
                    .array_layers(1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::SAMPLED,
                    ),
                None,
            )
            .map_err(|e| format!("Failed to create indicator image: {:?}", e))?
    };
    let indicator_allocation = {
        let reqs = unsafe { device.get_image_memory_requirements(indicator_image) };
        context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "indicator_image",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate indicator image: {}", e))?
    };
    unsafe {
        device
            .bind_image_memory(
                indicator_image,
                indicator_allocation.memory(),
                indicator_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind indicator image: {:?}", e))?;
    }
    let indicator_view = create_image_view(
        device,
        indicator_image,
        indicator_format,
        vk::ImageAspectFlags::COLOR,
    )?;

    // LDR output image (R8G8B8A8Unorm)
    let ldr_format = vk::Format::R8G8B8A8_UNORM;
    let ldr_image = unsafe {
        device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(ldr_format)
                    .extent(extent_3d)
                    .mip_levels(1)
                    .array_layers(1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::TRANSFER_DST,
                    ),
                None,
            )
            .map_err(|e| format!("Failed to create LDR image: {:?}", e))?
    };
    let ldr_allocation = {
        let reqs = unsafe { device.get_image_memory_requirements(ldr_image) };
        context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "ldr_image",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate LDR image: {}", e))?
    };
    unsafe {
        device
            .bind_image_memory(ldr_image, ldr_allocation.memory(), ldr_allocation.offset())
            .map_err(|e| format!("Failed to bind LDR image: {:?}", e))?;
    }
    let ldr_view = create_image_view(device, ldr_image, ldr_format, vk::ImageAspectFlags::COLOR)?;

    // Picking image (R32Uint) for depth prepass color output
    let picking_format = vk::Format::R32_UINT;
    let picking_image = unsafe {
        device
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(picking_format)
                    .extent(extent_3d)
                    .mip_levels(1)
                    .array_layers(1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::TRANSFER_DST,
                    ),
                None,
            )
            .map_err(|e| format!("Failed to create picking image: {:?}", e))?
    };
    let picking_allocation = {
        let reqs = unsafe { device.get_image_memory_requirements(picking_image) };
        context
            .allocator
            .borrow_mut()
            .allocate(&AllocationCreateDesc {
                name: "picking_image",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| format!("Failed to allocate picking image: {}", e))?
    };
    unsafe {
        device
            .bind_image_memory(
                picking_image,
                picking_allocation.memory(),
                picking_allocation.offset(),
            )
            .map_err(|e| format!("Failed to bind picking image: {:?}", e))?;
    }
    let picking_view = create_image_view(
        device,
        picking_image,
        picking_format,
        vk::ImageAspectFlags::COLOR,
    )?;

    // Staging buffers for readback (properly sized)
    let ldr_pixel_size = (WIDTH * HEIGHT * 4) as u64;
    let indicator_pixel_size = (WIDTH * HEIGHT) as u64;
    let depth_pixel_size = (WIDTH * HEIGHT * 4) as u64; // D32_SFLOAT = 4 bytes per pixel
    let picking_pixel_size = (WIDTH * HEIGHT * 4) as u64; // R32_UINT = 4 bytes per pixel

    let ldr_staging = GpuBuffer::new_staging(context, ldr_pixel_size, "ldr_staging")?;
    let indicator_staging =
        GpuBuffer::new_staging(context, indicator_pixel_size, "indicator_staging")?;
    let depth_staging = GpuBuffer::new_staging(context, depth_pixel_size, "depth_staging")?;
    let picking_staging = GpuBuffer::new_staging(context, picking_pixel_size, "picking_staging")?;
    let stencil_staging =
        GpuBuffer::new_staging(context, (WIDTH * HEIGHT) as u64, "stencil_staging")?;

    // Transition all images to correct layouts
    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();

    let color_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    let ds_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    transition_image_layout(
        cmd,
        device,
        depth_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        ds_range,
    );
    transition_image_layout(
        cmd,
        device,
        hdr_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        color_range,
    );
    transition_image_layout(
        cmd,
        device,
        indicator_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        color_range,
    );
    transition_image_layout(
        cmd,
        device,
        ldr_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        color_range,
    );
    transition_image_layout(
        cmd,
        device,
        picking_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        color_range,
    );

    submit_and_wait(context, &cmd_buf)?;

    Ok(ImageResources {
        depth_image,
        depth_allocation,
        depth_view,
        depth_stencil_view,
        hdr_image,
        hdr_allocation,
        hdr_view,
        indicator_image,
        indicator_allocation,
        indicator_view,
        ldr_image,
        ldr_allocation,
        ldr_view,
        picking_image,
        picking_allocation,
        picking_view,
        ldr_staging: ldr_staging.buffer,
        ldr_staging_allocation: ldr_staging.allocation,
        indicator_staging: indicator_staging.buffer,
        indicator_staging_allocation: indicator_staging.allocation,
        depth_staging: depth_staging.buffer,
        depth_staging_allocation: depth_staging.allocation,
        picking_staging: picking_staging.buffer,
        picking_staging_allocation: picking_staging.allocation,
        stencil_staging: stencil_staging.buffer,
        stencil_staging_allocation: stencil_staging.allocation,
    })
}

fn create_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect_mask: vk::ImageAspectFlags,
) -> Result<vk::ImageView, String> {
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(vk::ComponentMapping {
            r: vk::ComponentSwizzle::IDENTITY,
            g: vk::ComponentSwizzle::IDENTITY,
            b: vk::ComponentSwizzle::IDENTITY,
            a: vk::ComponentSwizzle::IDENTITY,
        })
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    unsafe { device.create_image_view(&create_info, None) }
        .map_err(|e| format!("Failed to create image view: {:?}", e))
}

fn create_pipelines(
    context: &std::rc::Rc<VulkanContext>,
    shader_dir: &PathBuf,
    _indicator_view: vk::ImageView,
) -> Result<PipelineResources, String> {
    let device = &context.device;
    let vs_entry = CString::new("vs_main").unwrap();
    let fs_entry = CString::new("fs_main").unwrap();

    // Descriptor set layout for storage buffers (binding 0 = frame, binding 1 = objects)
    let bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
    ];
    let descriptor_set_layout = unsafe {
        device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
            .map_err(|e| format!("Failed to create descriptor set layout: {:?}", e))?
    };

    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&[descriptor_set_layout]),
                None,
            )
            .map_err(|e| format!("Failed to create pipeline layout: {:?}", e))?
    };

    let pool_sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::STORAGE_BUFFER,
        descriptor_count: 2,
    }];
    let descriptor_pool = unsafe {
        device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&pool_sizes)
                    .max_sets(1),
                None,
            )
            .map_err(|e| format!("Failed to create descriptor pool: {:?}", e))?
    };
    let descriptor_sets = unsafe {
        device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&[descriptor_set_layout]),
            )
            .map_err(|e| format!("Failed to allocate descriptor set: {:?}", e))?
    };
    let descriptor_set = descriptor_sets[0];

    let mut shader_cache = ShaderCache::new(device.clone());

    let full_color_write = vk::ColorComponentFlags::R
        | vk::ColorComponentFlags::G
        | vk::ColorComponentFlags::B
        | vk::ColorComponentFlags::A;

    // === Depth prepass pipeline (raw VK, not part of outline system) ===
    let depth_prepass_path = shader_dir.join("depth_prepass.wgsl");
    let dp_vert = shader_cache
        .load_shader(&depth_prepass_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load depth prepass vertex: {}", e))?;
    let dp_frag = shader_cache
        .load_shader(&depth_prepass_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load depth prepass fragment: {}", e))?;

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let dp_vertex_bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(12)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let dp_vertex_attributes = [vk::VertexInputAttributeDescription::default()
        .location(0)
        .binding(0)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset(0)];
    let dp_vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&dp_vertex_bindings)
        .vertex_attribute_descriptions(&dp_vertex_attributes);

    let dp_shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(dp_vert)
            .name(&vs_entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(dp_frag)
            .name(&fs_entry),
    ];
    let dp_rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);
    let dp_color_blend_attachments =
        [vk::PipelineColorBlendAttachmentState::default().color_write_mask(full_color_write)];
    let dp_color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&dp_color_blend_attachments);
    let dp_depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL);
    let mut dp_rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&[vk::Format::R32_UINT])
        .depth_attachment_format(vk::Format::D32_SFLOAT_S8_UINT)
        .stencil_attachment_format(vk::Format::D32_SFLOAT_S8_UINT);
    let dp_create_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&dp_shader_stages)
        .vertex_input_state(&dp_vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .depth_stencil_state(&dp_depth_stencil)
        .rasterization_state(&dp_rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&dp_color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut dp_rendering);

    let depth_prepass_pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[dp_create_info], None)
            .map_err(|e| format!("Failed to create depth prepass pipeline: {:?}", e))?[0]
    };

    // === Stencil mark pipeline (PipelineBuilder) ===
    let stencil_mark_path = shader_dir.join("outline/stencil_mark.wgsl");
    let sm_vert = shader_cache
        .load_shader(&stencil_mark_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load stencil mark vertex: {}", e))?;
    let sm_frag = shader_cache
        .load_shader(&stencil_mark_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load stencil mark fragment: {}", e))?;

    let stencil_state_mark = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::REPLACE,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::ALWAYS,
        compare_mask: 0xFF,
        write_mask: 0x01,
        reference: 1,
    };

    let stencil_mark_pipeline = PipelineBuilder::new(context.clone())
        .with_shaders(sm_vert, sm_frag)
        .with_descriptor_layouts(vec![descriptor_set_layout])
        .with_soa_attribute(0, VertexFormat::RGB32f)
        .with_depth_test(true, false, CompareOp::GreaterOrEqual)
        .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
        .with_stencil_test(stencil_state_mark, stencil_state_mark)
        .with_color_write_mask(vk::ColorComponentFlags::empty())
        .with_rendering_formats(
            Some(ImageFormat::R16G16B16A16Sfloat),
            Some(ImageFormat::D32SfloatS8Uint),
        )
        .build_dynamic()
        .map_err(|e| format!("Failed to build stencil mark pipeline: {:?}", e))?;

    // === Occlusion mark pipeline (PipelineBuilder) ===
    let stencil_state_occlusion = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::REPLACE,
        compare_op: vk::CompareOp::EQUAL,
        compare_mask: 0x01,
        write_mask: 0x02,
        reference: 2,
    };

    let occlusion_mark_pipeline = PipelineBuilder::new(context.clone())
        .with_shaders(sm_vert, sm_frag)
        .with_descriptor_layouts(vec![descriptor_set_layout])
        .with_soa_attribute(0, VertexFormat::RGB32f)
        .with_depth_test(true, false, CompareOp::GreaterOrEqual)
        .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
        .with_stencil_test(stencil_state_occlusion, stencil_state_occlusion)
        .with_color_write_mask(vk::ColorComponentFlags::empty())
        .with_rendering_formats(
            Some(ImageFormat::R16G16B16A16Sfloat),
            Some(ImageFormat::D32SfloatS8Uint),
        )
        .build_dynamic()
        .map_err(|e| format!("Failed to build occlusion mark pipeline: {:?}", e))?;

    // === Outline draw pipeline (PipelineBuilder) ===
    let outline_draw_path = shader_dir.join("outline/outline_draw.wgsl");
    let od_vert = shader_cache
        .load_shader(&outline_draw_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load outline draw vertex: {}", e))?;
    let od_frag = shader_cache
        .load_shader(&outline_draw_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load outline draw fragment: {}", e))?;

    let stencil_state_outline = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::EQUAL,
        compare_mask: 0xFF,
        write_mask: 0x00,
        reference: 0,
    };

    let outline_draw_pipeline = PipelineBuilder::new(context.clone())
        .with_shaders(od_vert, od_frag)
        .with_descriptor_layouts(vec![descriptor_set_layout])
        .with_soa_attribute(0, VertexFormat::RGB32f)
        .with_depth_test(true, false, CompareOp::GreaterOrEqual)
        .with_cull_mode(CullMode::Front, FrontFace::CounterClockwise)
        .with_stencil_test(stencil_state_outline, stencil_state_outline)
        .with_rendering_formats(
            Some(ImageFormat::R16G16B16A16Sfloat),
            Some(ImageFormat::D32SfloatS8Uint),
        )
        .with_push_constant_range(
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            32,
        )
        .build_dynamic()
        .map_err(|e| format!("Failed to build outline draw pipeline: {:?}", e))?;

    // === Stencil indicator pipeline (PipelineBuilder) ===
    let stencil_indicator_path = shader_dir.join("outline/stencil_indicator.wgsl");
    let si_vert = shader_cache
        .load_shader(&stencil_indicator_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load stencil indicator vertex: {}", e))?;
    let si_frag = shader_cache
        .load_shader(&stencil_indicator_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load stencil indicator fragment: {}", e))?;

    let stencil_state_indicator = vk::StencilOpState {
        fail_op: vk::StencilOp::KEEP,
        pass_op: vk::StencilOp::KEEP,
        depth_fail_op: vk::StencilOp::KEEP,
        compare_op: vk::CompareOp::EQUAL,
        compare_mask: 0xFF,
        write_mask: 0x00,
        reference: 2,
    };

    let stencil_indicator_pipeline = PipelineBuilder::new(context.clone())
        .with_shaders(si_vert, si_frag)
        .with_descriptor_layouts(vec![descriptor_set_layout])
        .with_soa_attribute(0, VertexFormat::RGB32f)
        .with_depth_test(true, false, CompareOp::Always)
        .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
        .with_stencil_test(stencil_state_indicator, stencil_state_indicator)
        .with_rendering_formats(
            Some(ImageFormat::R8Unorm),
            Some(ImageFormat::D32SfloatS8Uint),
        )
        .build_dynamic()
        .map_err(|e| format!("Failed to build stencil indicator pipeline: {:?}", e))?;

    // === Simplified tonemap pipeline (raw VK, not part of outline system) ===
    let tonemap_path = shader_dir.join("outline_validation_tonemap.wgsl");
    let tm_vert = shader_cache
        .load_shader(&tonemap_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Failed to load tonemap vertex: {}", e))?;
    let tm_frag = shader_cache
        .load_shader(&tonemap_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Failed to load tonemap fragment: {}", e))?;

    // Tonemap descriptor set: separate texture + sampler bindings matching WGSL layout
    let tonemap_bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(2)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    ];
    let tonemap_descriptor_set_layout = unsafe {
        device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&tonemap_bindings),
                None,
            )
            .map_err(|e| format!("Failed to create tonemap descriptor set layout: {:?}", e))?
    };

    let tonemap_pipeline_layout = unsafe {
        device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[tonemap_descriptor_set_layout]),
                None,
            )
            .map_err(|e| format!("Failed to create tonemap pipeline layout: {:?}", e))?
    };

    let tonemap_pool_sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: 2,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLER,
            descriptor_count: 2,
        },
    ];
    let tonemap_descriptor_pool = unsafe {
        device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&tonemap_pool_sizes)
                    .max_sets(1),
                None,
            )
            .map_err(|e| format!("Failed to create tonemap descriptor pool: {:?}", e))?
    };
    let tonemap_descriptor_sets = unsafe {
        device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(tonemap_descriptor_pool)
                    .set_layouts(&[tonemap_descriptor_set_layout]),
            )
            .map_err(|e| format!("Failed to allocate tonemap descriptor set: {:?}", e))?
    };
    let tonemap_descriptor_set = tonemap_descriptor_sets[0];

    // Tonemap has no vertex input (fullscreen triangle)
    let tm_vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let tm_shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(tm_vert)
            .name(&vs_entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(tm_frag)
            .name(&fs_entry),
    ];
    let tm_rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE);
    let tm_color_blend =
        vk::PipelineColorBlendAttachmentState::default().color_write_mask(full_color_write);
    let tm_color_blend_attachments = [tm_color_blend];
    let tm_color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .attachments(&tm_color_blend_attachments);
    let tm_depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false);
    let mut tm_rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&[vk::Format::R8G8B8A8_UNORM]);
    let tm_create_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&tm_shader_stages)
        .vertex_input_state(&tm_vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .depth_stencil_state(&tm_depth_stencil)
        .rasterization_state(&tm_rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&tm_color_blending)
        .dynamic_state(&dynamic_state)
        .layout(tonemap_pipeline_layout)
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut tm_rendering);

    let tonemap_pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[tm_create_info], None)
            .map_err(|e| format!("Failed to build tonemap pipeline: {:?}", e))?[0]
    };

    Ok(PipelineResources {
        descriptor_set_layout,
        pipeline_layout,
        descriptor_pool,
        descriptor_set,
        depth_prepass_pipeline,
        stencil_mark_pipeline,
        occlusion_mark_pipeline,
        outline_draw_pipeline,
        stencil_indicator_pipeline,
        tonemap_pipeline,
        tonemap_descriptor_set_layout,
        tonemap_pipeline_layout,
        tonemap_descriptor_pool,
        tonemap_descriptor_set,
    })
}

fn create_sampler(context: &VulkanContext) -> Result<vk::Sampler, String> {
    let create_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
    unsafe {
        context
            .device
            .create_sampler(&create_info, None)
            .map_err(|e| format!("Failed to create sampler: {:?}", e))
    }
}

// ---------------------------------------------------------------------------
// GPU rendering capability test
// ---------------------------------------------------------------------------

/// Tests if the GPU can render at all by clearing an image to a known color
/// and reading it back. Returns Ok(true) if rendering works, Ok(false) if
/// the GPU appears to not produce rendering output (common on Intel headless).
fn test_gpu_rendering_capability(
    context: &VulkanContext,
    images: &ImageResources,
) -> Result<bool, String> {
    let device = &context.device;
    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();
    let render_area = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: WIDTH,
            height: HEIGHT,
        },
    };

    // Clear the LDR image to a known color (red) using a render pass
    let color_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(images.ldr_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.5, 0.25, 0.0, 1.0],
            },
        });

    unsafe {
        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[color_attachment]),
        );
        device.cmd_end_rendering(cmd);

        // Transition to TRANSFER_SRC and copy to staging
        transition_image_layout(
            cmd,
            device,
            images.ldr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        );

        let copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd,
            images.ldr_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.ldr_staging,
            &copy_regions,
        );

        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    }

    submit_and_wait(context, &cmd_buf)?;

    // Read back and check
    context.invalidate_mapped_memory(
        &images.ldr_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );

    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    if let Some(mapped) = images.ldr_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                pixels.as_mut_ptr(),
                pixels.len(),
            );
        }
    }

    // Check first pixel: should be (0.5*255, 0.25*255, 0, 255) ≈ (128, 64, 0, 255)
    let r = pixels[0];
    let g = pixels[1];
    let b = pixels[2];
    let a = pixels[3];

    log::info!(
        "  GPU clear test: first pixel = ({}, {}, {}, {}) — expected ~(128, 64, 0, 255)",
        r,
        g,
        b,
        a
    );

    // Restore LDR image layout for subsequent use
    let cmd_buf2 = context.begin_single_time_commands();
    let cmd2 = cmd_buf2.vk_command_buffer();
    transition_image_layout(
        cmd2,
        device,
        images.ldr_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
    );
    submit_and_wait(context, &cmd_buf2)?;

    // Allow some tolerance for format conversion
    let clear_works = r > 100 && r < 160 && g > 40 && g < 100 && b < 20 && a > 200;

    if !clear_works {
        return Ok(false);
    }

    // Phase 2: Test actual pipeline rendering with a simple fullscreen triangle
    log::info!("  Testing GPU pipeline execution...");

    let shader_dir = find_shader_directory();
    let mut shader_cache = ShaderCache::new(device.clone());

    let vs_entry = CString::new("vs_main").unwrap();
    let fs_entry = CString::new("fs_main").unwrap();

    // Create a simple pipeline that renders a green fullscreen triangle
    let simple_wgsl = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
    var pos = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );
    return vec4f(pos[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return vec4f(0.0, 1.0, 0.0, 1.0);
}
"#;
    let simple_shader_path = shader_dir.join("_outline_validation_simple.wgsl");
    std::fs::write(&simple_shader_path, simple_wgsl).ok();

    let vert = shader_cache
        .load_shader(&simple_shader_path, vk::ShaderStageFlags::VERTEX)
        .map_err(|e| format!("Simple VS: {}", e))?;
    let frag = shader_cache
        .load_shader(&simple_shader_path, vk::ShaderStageFlags::FRAGMENT)
        .map_err(|e| format!("Simple FS: {}", e))?;

    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default(), None)
            .map_err(|e| format!("Failed to create simple pipeline layout: {:?}", e))?
    };

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert)
            .name(&vs_entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(&fs_entry),
    ];

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&[vk::Format::R8G8B8A8_UNORM]);

    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE);
    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default().color_write_mask(
        vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    );
    let color_blend_attachments = [color_blend_attachment];
    let color_blending =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_blend_attachments);
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let create_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering_info);

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
            .map_err(|e| format!("Failed to create simple pipeline: {:?}", e))?[0]
    };

    // Render a fullscreen green triangle
    let cmd_buf3 = context.begin_single_time_commands();
    let cmd3 = cmd_buf3.vk_command_buffer();

    let green_attachment = vk::RenderingAttachmentInfo::default()
        .image_view(images.ldr_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
        });

    unsafe {
        device.cmd_begin_rendering(
            cmd3,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[green_attachment]),
        );
        device.cmd_set_viewport(
            cmd3,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd3, 0, &[render_area]);
        device.cmd_bind_pipeline(cmd3, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_draw(cmd3, 3, 1, 0, 0);
        device.cmd_end_rendering(cmd3);

        transition_image_layout(
            cmd3,
            device,
            images.ldr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        );

        let copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd3,
            images.ldr_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.ldr_staging,
            &copy_regions,
        );

        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd3,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );
    }

    submit_and_wait(context, &cmd_buf3)?;

    context.invalidate_mapped_memory(
        &images.ldr_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );

    let mut pixels2 = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    if let Some(mapped) = images.ldr_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                pixels2.as_mut_ptr(),
                pixels2.len(),
            );
        }
    }

    let r2 = pixels2[0];
    let g2 = pixels2[1];
    let b2 = pixels2[2];
    let a2 = pixels2[3];

    log::info!(
        "  GPU pipeline test: first pixel = ({}, {}, {}, {}) — expected ~(0, 255, 0, 255)",
        r2,
        g2,
        b2,
        a2
    );

    // Restore LDR image layout
    let cmd_buf4 = context.begin_single_time_commands();
    let cmd4 = cmd_buf4.vk_command_buffer();
    transition_image_layout(
        cmd4,
        device,
        images.ldr_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        },
    );
    submit_and_wait(context, &cmd_buf4)?;

    // Cleanup
    unsafe {
        device.destroy_pipeline(pipeline, None);
        device.destroy_pipeline_layout(pipeline_layout, None);
    }
    let _ = std::fs::remove_file(simple_shader_path);

    // Green triangle: R=0, G=255, B=0, A=255
    let pipeline_works = r2 < 10 && g2 > 200 && b2 < 10 && a2 > 200;
    Ok(pipeline_works)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_and_readback(
    context: &VulkanContext,
    images: &ImageResources,
    pipelines: &PipelineResources,
    _frame_buffer: &GpuBuffer,
    _object_buffer: &GpuBuffer,
    vertex_buffer: &GpuBuffer,
    index_buffer: &GpuBuffer,
    index_count: u32,
    sampler: vk::Sampler,
) -> Result<(Vec<u8>, Vec<u8>, Vec<f32>, Vec<u32>, Vec<u8>), String> {
    let device = &context.device;
    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();
    let render_area = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: WIDTH,
            height: HEIGHT,
        },
    };

    let color_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    unsafe {
        // === Depth prepass (both spheres) ===
        let depth_prepass_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.picking_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
            });
        let depth_prepass_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let depth_prepass_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        // The depth prepass writes to a R32Uint picking texture — but we don't need it.
        // Use a null color attachment since we only care about depth.
        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[depth_prepass_color])
                .depth_attachment(&depth_prepass_depth)
                .stencil_attachment(&depth_prepass_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.depth_prepass_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);
        // Draw only the occluder (instance 0). The selected sphere (instance 1) is
        // excluded from the depth prepass to match production behavior — this prevents
        // self-occlusion artifacts in the stencil outline system.
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Outline pass (3 sub-passes, only selected sphere = instance 1) ===
        // Sync depth buffer between render passes
        depth_render_pass_sync(cmd, device, images.depth_image);

        let outline_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.hdr_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });
        let outline_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let outline_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[outline_color])
                .depth_attachment(&outline_depth)
                .stencil_attachment(&outline_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        // Sub-pass 1: Stencil mark (instance 1 only)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.stencil_mark_pipeline.vk_pipeline(),
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 1); // instance 1 = selected

        // Sub-pass 2: Occlusion mark (instance 1 only)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.occlusion_mark_pipeline.vk_pipeline(),
        );
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 1);

        // Sub-pass 3: Outline draw (instance 1 only, inverted culling)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.outline_draw_pipeline.vk_pipeline(),
        );
        let outline_push = OutlinePushConstants {
            outline_width: 0.004,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            outline_color: [1.0, 0.55, 0.0, 1.0],
        };
        let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        device.cmd_push_constants(
            cmd,
            pipelines.outline_draw_pipeline.vk_layout(),
            stages,
            0,
            bytemuck::bytes_of(&outline_push),
        );
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 1);

        device.cmd_end_rendering(cmd);

        // === Stencil indicator pass ===
        depth_render_pass_sync(cmd, device, images.depth_image);

        let indicator_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.indicator_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });
        let indicator_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let indicator_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[indicator_color])
                .depth_attachment(&indicator_depth)
                .stencil_attachment(&indicator_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.stencil_indicator_pipeline.vk_pipeline(),
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 1);

        device.cmd_end_rendering(cmd);

        // === Tonemap pass ===
        // Transition HDR and indicator to SHADER_READ_ONLY
        transition_image_layout(
            cmd,
            device,
            images.hdr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            color_range,
        );
        transition_image_layout(
            cmd,
            device,
            images.indicator_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            color_range,
        );

        // Update tonemap descriptor set with image views and sampler
        let hdr_image_info = vk::DescriptorImageInfo::default()
            .image_view(images.hdr_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let indicator_image_info = vk::DescriptorImageInfo::default()
            .image_view(images.indicator_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let sampler_info = vk::DescriptorImageInfo::default().sampler(sampler);
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .image_info(&[hdr_image_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .image_info(&[sampler_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .image_info(&[indicator_image_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .image_info(&[sampler_info]),
            ],
            &[],
        );

        let tonemap_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.ldr_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[tonemap_color]),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.tonemap_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.tonemap_pipeline_layout,
            0,
            &[pipelines.tonemap_descriptor_set],
            &[],
        );
        // Fullscreen triangle: 3 vertices, no vertex buffer
        device.cmd_draw(cmd, 3, 1, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Copy to staging buffers ===
        // Transition LDR, indicator, depth, and picking to TRANSFER_SRC
        let transfer_color_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        transition_image_layout(
            cmd,
            device,
            images.ldr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );
        transition_image_layout(
            cmd,
            device,
            images.indicator_image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );
        // Depth is still in DEPTH_STENCIL_ATTACHMENT_OPTIMAL after stencil indicator pass
        transition_image_layout(
            cmd,
            device,
            images.depth_image,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        );
        transition_image_layout(
            cmd,
            device,
            images.picking_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );

        // Memory barrier to ensure rendering is visible to transfer
        let memory_barrier = vk::MemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::TRANSFER_WRITE,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[memory_barrier],
            &[],
            &[],
        );

        let color_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        let depth_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd,
            images.ldr_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.ldr_staging,
            &color_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.indicator_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.indicator_staging,
            &color_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.depth_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.depth_staging,
            &depth_copy_regions,
        );

        // Copy stencil aspect (S8 component of D32_SFLOAT_S8_UINT)
        let stencil_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::STENCIL,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd,
            images.depth_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.stencil_staging,
            &stencil_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.picking_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.picking_staging,
            &color_copy_regions,
        );

        // Barrier to make transfer visible to host
        let transfer_barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[transfer_barrier],
            &[],
            &[],
        );
    }

    submit_and_wait(context, &cmd_buf)?;

    // Readback LDR pixels
    context.invalidate_mapped_memory(
        &images.ldr_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut ldr_pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    if let Some(mapped) = images.ldr_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                ldr_pixels.as_mut_ptr(),
                ldr_pixels.len(),
            );
        }
    }

    context.invalidate_mapped_memory(
        &images.indicator_staging_allocation,
        0,
        (WIDTH * HEIGHT) as u64,
    );
    let mut indicator_pixels = vec![0u8; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.indicator_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                indicator_pixels.as_mut_ptr(),
                indicator_pixels.len(),
            );
        }
    }

    // Readback depth pixels (f32 per pixel)
    context.invalidate_mapped_memory(
        &images.depth_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut depth_pixels = vec![0.0f32; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.depth_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const f32,
                depth_pixels.as_mut_ptr(),
                depth_pixels.len(),
            );
        }
    }

    // Readback picking pixels (u32 per pixel)
    context.invalidate_mapped_memory(
        &images.picking_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut picking_pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.picking_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u32,
                picking_pixels.as_mut_ptr(),
                picking_pixels.len(),
            );
        }
    }

    // Readback stencil pixels (1 byte per pixel, S8 component)
    context.invalidate_mapped_memory(
        &images.stencil_staging_allocation,
        0,
        (WIDTH * HEIGHT) as u64,
    );
    let mut stencil_pixels = vec![0u8; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.stencil_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                stencil_pixels.as_mut_ptr(),
                stencil_pixels.len(),
            );
        }
    }

    Ok((
        ldr_pixels,
        indicator_pixels,
        depth_pixels,
        picking_pixels,
        stencil_pixels,
    ))
}

// ---------------------------------------------------------------------------
// Two-plane self-occlusion test
// ---------------------------------------------------------------------------

struct QuadPairMesh {
    positions: Vec<f32>,
    indices: Vec<u32>,
}

fn create_quad_pair(front_z: f32, back_z: f32, half_size: f32) -> QuadPairMesh {
    // Two quads facing the camera (+Z direction), at different depths.
    // Each quad is two triangles forming a square in the XY plane.
    let s = half_size;
    let mut positions = Vec::new();
    let mut indices = Vec::new();

    // Front quad (closer to camera at z=5): at z = front_z
    positions.extend_from_slice(&[
        -s, -s, front_z, s, -s, front_z, s, s, front_z, -s, s, front_z,
    ]);
    indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);

    // Back quad (farther from camera): at z = back_z
    positions.extend_from_slice(&[-s, -s, back_z, s, -s, back_z, s, s, back_z, -s, s, back_z]);
    indices.extend_from_slice(&[4, 5, 6, 4, 6, 7]);

    QuadPairMesh { positions, indices }
}

fn render_self_occlusion_test(
    context: &VulkanContext,
    images: &ImageResources,
    pipelines: &PipelineResources,
    vertex_buffer: &GpuBuffer,
    index_buffer: &GpuBuffer,
    index_count: u32,
    sampler: vk::Sampler,
) -> Result<(Vec<u8>, Vec<u8>, Vec<f32>, Vec<u32>, Vec<u8>), String> {
    let device = &context.device;
    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();
    let render_area = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent: vk::Extent2D {
            width: WIDTH,
            height: HEIGHT,
        },
    };

    let color_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    unsafe {
        // === Depth prepass: draw NOTHING (no occluder objects).
        // This matches production behavior where the selected entity is excluded.
        // The depth buffer remains cleared (depth=0.0 in reverse-Z = far plane).
        let depth_prepass_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.picking_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
            });
        let depth_prepass_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let depth_prepass_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[depth_prepass_color])
                .depth_attachment(&depth_prepass_depth)
                .stencil_attachment(&depth_prepass_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.depth_prepass_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);
        // Draw nothing — no occluder objects. The selected object's quads
        // are excluded from the depth prepass to prevent self-occlusion.
        device.cmd_draw_indexed(cmd, 0, 1, 0, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Outline pass (stencil mark, occlusion mark, outline draw) ===
        depth_render_pass_sync(cmd, device, images.depth_image);

        let outline_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.hdr_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });
        let outline_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let outline_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[outline_color])
                .depth_attachment(&outline_depth)
                .stencil_attachment(&outline_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);

        // Sub-pass 1: Stencil mark (instance 0 = selected = two quads)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.stencil_mark_pipeline.vk_pipeline(),
        );
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 0);

        // Sub-pass 2: Occlusion mark (instance 0)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.occlusion_mark_pipeline.vk_pipeline(),
        );
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 0);

        // Sub-pass 3: Outline draw (instance 0, inverted culling)
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.outline_draw_pipeline.vk_pipeline(),
        );
        let outline_push = OutlinePushConstants {
            outline_width: 0.004,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            outline_color: [1.0, 0.55, 0.0, 1.0],
        };
        let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        device.cmd_push_constants(
            cmd,
            pipelines.outline_draw_pipeline.vk_layout(),
            stages,
            0,
            bytemuck::bytes_of(&outline_push),
        );
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Stencil indicator pass ===
        depth_render_pass_sync(cmd, device, images.depth_image);

        let indicator_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.indicator_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });
        let indicator_depth = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });
        let indicator_stencil = vk::RenderingAttachmentInfo::default()
            .image_view(images.depth_stencil_view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 0.0,
                    stencil: 0,
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[indicator_color])
                .depth_attachment(&indicator_depth)
                .stencil_attachment(&indicator_stencil),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.stencil_indicator_pipeline.vk_pipeline(),
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.pipeline_layout,
            0,
            &[pipelines.descriptor_set],
            &[],
        );
        device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
        device.cmd_bind_index_buffer(cmd, index_buffer.buffer, 0, vk::IndexType::UINT32);
        device.cmd_draw_indexed(cmd, index_count, 1, 0, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Tonemap pass ===
        transition_image_layout(
            cmd,
            device,
            images.hdr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            color_range,
        );
        transition_image_layout(
            cmd,
            device,
            images.indicator_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            color_range,
        );

        let hdr_image_info = vk::DescriptorImageInfo::default()
            .image_view(images.hdr_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let indicator_image_info = vk::DescriptorImageInfo::default()
            .image_view(images.indicator_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let sampler_info = vk::DescriptorImageInfo::default().sampler(sampler);
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .image_info(&[hdr_image_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .image_info(&[sampler_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .image_info(&[indicator_image_info]),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.tonemap_descriptor_set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .image_info(&[sampler_info]),
            ],
            &[],
        );

        let tonemap_color = vk::RenderingAttachmentInfo::default()
            .image_view(images.ldr_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            });

        device.cmd_begin_rendering(
            cmd,
            &vk::RenderingInfo::default()
                .render_area(render_area)
                .layer_count(1)
                .color_attachments(&[tonemap_color]),
        );

        device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: WIDTH as f32,
                height: HEIGHT as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        device.cmd_set_scissor(cmd, 0, &[render_area]);

        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.tonemap_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pipelines.tonemap_pipeline_layout,
            0,
            &[pipelines.tonemap_descriptor_set],
            &[],
        );
        device.cmd_draw(cmd, 3, 1, 0, 0);

        device.cmd_end_rendering(cmd);

        // === Copy to staging buffers ===
        let transfer_color_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        transition_image_layout(
            cmd,
            device,
            images.ldr_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );
        transition_image_layout(
            cmd,
            device,
            images.indicator_image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );
        transition_image_layout(
            cmd,
            device,
            images.depth_image,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            },
        );
        transition_image_layout(
            cmd,
            device,
            images.picking_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            transfer_color_range,
        );

        let memory_barrier = vk::MemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::TRANSFER_WRITE,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[memory_barrier],
            &[],
            &[],
        );

        let color_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        let depth_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd,
            images.ldr_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.ldr_staging,
            &color_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.indicator_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.indicator_staging,
            &color_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.depth_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.depth_staging,
            &depth_copy_regions,
        );

        let stencil_copy_regions = [vk::BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            image_subresource: vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::STENCIL,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            },
            image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
            image_extent: vk::Extent3D {
                width: WIDTH,
                height: HEIGHT,
                depth: 1,
            },
        }];

        device.cmd_copy_image_to_buffer(
            cmd,
            images.depth_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.stencil_staging,
            &stencil_copy_regions,
        );

        device.cmd_copy_image_to_buffer(
            cmd,
            images.picking_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            images.picking_staging,
            &color_copy_regions,
        );

        let transfer_barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::HOST_READ);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[transfer_barrier],
            &[],
            &[],
        );
    }

    submit_and_wait(context, &cmd_buf)?;

    // Readback LDR pixels
    context.invalidate_mapped_memory(
        &images.ldr_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut ldr_pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    if let Some(mapped) = images.ldr_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                ldr_pixels.as_mut_ptr(),
                ldr_pixels.len(),
            );
        }
    }

    context.invalidate_mapped_memory(
        &images.indicator_staging_allocation,
        0,
        (WIDTH * HEIGHT) as u64,
    );
    let mut indicator_pixels = vec![0u8; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.indicator_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                indicator_pixels.as_mut_ptr(),
                indicator_pixels.len(),
            );
        }
    }

    context.invalidate_mapped_memory(
        &images.depth_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut depth_pixels = vec![0.0f32; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.depth_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const f32,
                depth_pixels.as_mut_ptr(),
                depth_pixels.len(),
            );
        }
    }

    context.invalidate_mapped_memory(
        &images.picking_staging_allocation,
        0,
        (WIDTH * HEIGHT * 4) as u64,
    );
    let mut picking_pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.picking_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u32,
                picking_pixels.as_mut_ptr(),
                picking_pixels.len(),
            );
        }
    }

    context.invalidate_mapped_memory(
        &images.stencil_staging_allocation,
        0,
        (WIDTH * HEIGHT) as u64,
    );
    let mut stencil_pixels = vec![0u8; (WIDTH * HEIGHT) as usize];
    if let Some(mapped) = images.stencil_staging_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u8,
                stencil_pixels.as_mut_ptr(),
                stencil_pixels.len(),
            );
        }
    }

    Ok((
        ldr_pixels,
        indicator_pixels,
        depth_pixels,
        picking_pixels,
        stencil_pixels,
    ))
}

fn validate_no_self_occlusion(
    indicator_pixels: &[u8],
    stencil_pixels: &[u8],
) -> Result<(), String> {
    // Two-plane self-occlusion test:
    // Two quads at different depths, both part of the same selected object.
    // NO occluder objects rendered into the depth prepass.
    //
    // Expected: zero stencil=2 (occlusion mark) pixels, because there is
    // nothing in the depth buffer to occlude the quads. Both quads should
    // pass the depth test in both stencil mark and occlusion mark passes.
    //
    // If stencil=2 exists, it means self-occlusion is happening — the
    // back quad's depth is failing against the front quad's depth.

    let stencil_0 = stencil_pixels.iter().filter(|&&s| s == 0).count();
    let stencil_1 = stencil_pixels.iter().filter(|&&s| s == 1).count();
    let stencil_2 = stencil_pixels.iter().filter(|&&s| s == 2).count();

    let indicator_nonzero = indicator_pixels.iter().filter(|&&p| p > 0).count();

    log::info!(
        "  Self-occlusion test: s=0:{} s=1:{} s=2:{} indicator_nonzero:{}",
        stencil_0,
        stencil_1,
        stencil_2,
        indicator_nonzero,
    );

    // Stencil mark (s=1) should exist — both quads should pass depth test
    // and get marked as visible.
    if stencil_1 == 0 {
        return Err(
            "No stencil=1 (visible) marks — quads not rendered or stencil mark pass failed"
                .to_string(),
        );
    }

    // Stencil=2 (occluded) should be ZERO — there's nothing in the depth
    // buffer to occlude the quads. This is the key self-occlusion check.
    if stencil_2 > 0 {
        return Err(format!(
            "Self-occlusion detected: {} stencil=2 (occluded) pixels found. \
             With no occluder in the depth prepass, both quads of the selected \
             object should be fully visible. The back quad must be failing depth \
             test against the front quad's depth, which means self-occlusion is \
             NOT fixed.",
            stencil_2
        ));
    }

    // Indicator should have zero non-zero pixels (it only shows stencil=2)
    if indicator_nonzero > 0 {
        return Err(format!(
            "Stencil indicator has {} non-zero pixels — wallhack overlay would be \
             visible where it shouldn't be",
            indicator_nonzero
        ));
    }

    // Note: outline check is not applicable for flat quads because the
    // outline technique relies on inverted culling which requires mesh volume.
    // For flat geometry, there are no back faces visible from the camera side
    // to create the outline edge effect. The stencil and occlusion checks
    // above are the definitive self-occlusion validation.
    log::info!(
        "  Self-occlusion test: OK (no self-occlusion, {} visible stencil marks)",
        stencil_1
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_depth_prepass(depth_pixels: &[f32], picking_pixels: &[u32]) -> Result<(), String> {
    // After depth prepass, both spheres should have written non-zero depth values.
    // With reverse-Z, closer objects get higher depth values.
    let non_zero_depth = depth_pixels.iter().filter(|&&d| d > 0.0).count();
    let total = depth_pixels.len();

    // Check picking image — depth prepass fragment shader writes instance_idx+1
    let non_zero_picking = picking_pixels.iter().filter(|&&p| p > 0).count();

    log::info!(
        "  Depth prepass: {}/{} non-zero depth pixels ({:.1}%)",
        non_zero_depth,
        total,
        100.0 * non_zero_depth as f64 / total as f64
    );
    log::info!(
        "  Picking: {}/{} non-zero pixels (entity IDs)",
        non_zero_picking,
        picking_pixels.len()
    );

    if non_zero_picking == 0 {
        return Err(
            "No entity IDs in picking buffer — GPU fragment shader may not be executing"
                .to_string(),
        );
    }

    if non_zero_depth == 0 {
        return Err("No non-zero depth pixels — depth write may be broken".to_string());
    }

    // Verify we see the occluder entity (instance 0 → ID 1).
    // The selected sphere (instance 1 → ID 2) is excluded from the depth prepass
    // to match production behavior and prevent self-occlusion artifacts.
    let has_entity_1 = picking_pixels.contains(&1);
    let entity_1_count = picking_pixels.iter().filter(|&&p| p == 1).count();
    let entity_2_count = picking_pixels.iter().filter(|&&p| p == 2).count();
    log::info!(
        "  Picking entities: entity_1={}({}) entity_2={}({})",
        has_entity_1,
        entity_1_count,
        false,
        entity_2_count
    );

    if !has_entity_1 {
        return Err(format!(
            "Expected occluder entity (ID 1) in picking buffer, got entity_1={}",
            has_entity_1
        ));
    }

    if entity_2_count > 0 {
        return Err(format!(
            "Selected entity (ID 2) should be excluded from depth prepass, found {} pixels",
            entity_2_count
        ));
    }

    Ok(())
}

fn validate_stencil_indicator(
    indicator_pixels: &[u8],
    stencil_pixels: &[u8],
) -> Result<(), String> {
    let indicator_count = indicator_pixels.iter().filter(|&&p| p > 0).count();

    // Count stencil values: 0=cleared, 1=stencil mark (visible), 2=occlusion mark (occluded)
    let stencil_0 = stencil_pixels.iter().filter(|&&s| s == 0).count();
    let stencil_1 = stencil_pixels.iter().filter(|&&s| s == 1).count();
    let stencil_2 = stencil_pixels.iter().filter(|&&s| s == 2).count();
    let other_stencil = stencil_pixels.len() - stencil_0 - stencil_1 - stencil_2;

    log::info!(
        "  Stencil distribution: s=0:{} s=1:{} s=2:{} other:{}",
        stencil_0,
        stencil_1,
        stencil_2,
        other_stencil
    );
    log::info!(
        "  Stencil indicator: {}/{} non-zero pixels",
        indicator_count,
        indicator_pixels.len()
    );

    if stencil_1 == 0 {
        return Err(
            "Stencil mark pass wrote no stencil=1 values — visible parts of selected object not detected"
                .to_string(),
        );
    }

    if stencil_2 == 0 {
        return Err(
            "Occlusion mark pass wrote no stencil=2 values — this is the known stencil bug: \
             the occlusion mark pipeline uses reference=1 (should be reference=2), so \
             depth-fail fragments get stencil=1 instead of stencil=2"
                .to_string(),
        );
    }

    if indicator_count == 0 {
        return Err(
            "Stencil indicator has no non-zero pixels — stencil=2 values exist but indicator pass failed to read them"
                .to_string(),
        );
    }

    Ok(())
}

fn validate_outline_present(ldr_pixels: &[u8]) -> Result<(), String> {
    // The outline color is (1.0, 0.55, 0.0) orange. After tonemapping (which is
    // a simple pass-through with gamma), these should be visible as orange pixels.
    let orange_pixels = ldr_pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 100 && p[1] > 40 && p[1] < 200 && p[2] < 80)
        .count();

    log::info!("  Orange outline pixels: {}", orange_pixels);

    if orange_pixels == 0 {
        return Err(
            "No orange outline pixels found — outline draw pass may have failed".to_string(),
        );
    }

    Ok(())
}

fn validate_overlay_in_tonemap(ldr_pixels: &[u8], indicator_pixels: &[u8]) -> Result<(), String> {
    let mut indicator_active_tonemap_orange = 0usize;
    let mut indicator_active_tonemap_total = 0usize;

    for i in 0..(WIDTH as usize * HEIGHT as usize) {
        if indicator_pixels[i] > 0 {
            indicator_active_tonemap_total += 1;
            let r = ldr_pixels[i * 4];
            let g = ldr_pixels[i * 4 + 1];
            if r > 50 && g > 20 {
                indicator_active_tonemap_orange += 1;
            }
        }
    }

    log::info!(
        "  Overlay: {}/{} indicator pixels show orange tint",
        indicator_active_tonemap_orange,
        indicator_active_tonemap_total
    );

    if indicator_active_tonemap_total > 0 && indicator_active_tonemap_orange == 0 {
        return Err(
            "Stencil indicator pixels exist but tonemap shows no orange tint — overlay blending may be broken"
                .to_string(),
        );
    }

    Ok(())
}

fn validate_self_occlusion(stencil_pixels: &[u8]) -> Result<(), String> {
    // With the selected sphere excluded from the depth prepass, the stencil
    // distribution should reflect only cross-object occlusion (occluder hiding
    // parts of the selected sphere), not self-occlusion artifacts.
    //
    // s=1: visible parts of selected sphere (not occluded by depth buffer)
    // s=2: occluded parts of selected sphere (behind occluder in depth buffer)
    //
    // If self-occlusion were occurring, s=2 would be inflated because the
    // selected sphere's own depth would occlude its back faces.

    let stencil_1 = stencil_pixels.iter().filter(|&&s| s == 1).count();
    let stencil_2 = stencil_pixels.iter().filter(|&&s| s == 2).count();
    let total_selected_pixels = stencil_1 + stencil_2;

    log::info!(
        "  Self-occlusion check: s=1={} s=2={} total_selected={}",
        stencil_1,
        stencil_2,
        total_selected_pixels
    );

    // With the selected sphere excluded from depth prepass, s=1 should be
    // small (only the visible crescent around the occluder's edge).
    // s=2 should represent the region hidden behind the occluder.
    // The key check: both values should be non-zero and the distribution
    // should be reasonable (not all pixels in one bucket).

    if stencil_1 == 0 && stencil_2 == 0 {
        return Err(
            "No stencil marks found for selected sphere — stencil system not working".to_string(),
        );
    }

    // With the fix applied (selected sphere excluded from depth prepass),
    // s=2 should be the dominant value since most of the selected sphere
    // is behind the occluder. But s=1 should still exist for the visible
    // crescent around the edges.
    if stencil_2 == 0 {
        return Err(
            "No occluded pixels (s=2) — selected sphere may not be behind the occluder \
             or depth prepass exclusion is not working correctly"
                .to_string(),
        );
    }

    // s=1 should be relatively small — just the visible crescent.
    // If s=1 were very large (e.g., > 50% of total), it would suggest
    // the selected sphere is not being properly occluded by the depth buffer.
    let visible_ratio = if total_selected_pixels > 0 {
        stencil_1 as f64 / total_selected_pixels as f64
    } else {
        1.0
    };

    if visible_ratio > 0.8 {
        return Err(format!(
            "Visible ratio too high ({:.1}%) — expected most of the selected sphere \
             to be occluded by the occluder. Self-occlusion may be interfering.",
            visible_ratio * 100.0
        ));
    }

    log::info!(
        "  Self-occlusion: OK (visible={:.1}%, occluded={:.1}%)",
        visible_ratio * 100.0,
        (1.0 - visible_ratio) * 100.0
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("=== GPU Outline Validation ===");

    let context = std::rc::Rc::new(VulkanContext::init_headless(
        ValidationMode::Enabled,
        CString::new("Outline Validation").unwrap(),
        CString::new("Katla Engine").unwrap(),
    ));
    log::info!("Vulkan context created");

    let shader_dir = find_shader_directory();
    log::info!("Shader directory: {:?}", shader_dir);

    // Create the custom tonemap shader for validation
    let tonemap_shader_path = shader_dir.join("outline_validation_tonemap.wgsl");
    if !tonemap_shader_path.exists() {
        log::info!("Creating simplified tonemap shader for validation...");
        let tonemap_wgsl = r#"// Simplified tonemap for outline validation.
// Fullscreen triangle that samples HDR color and stencil indicator,
// applies mix(color, orange, 0.4) where indicator > 0.5.

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let uv = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    out.clip_position = vec4f(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0)
var hdr_tex: texture_2d<f32>;
@group(0) @binding(1)
var hdr_sampler: sampler;

@group(0) @binding(2)
var indicator_tex: texture_2d<f32>;
@group(0) @binding(3)
var indicator_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let hdr_color = textureSample(hdr_tex, hdr_sampler, in.uv).rgb;
    let indicator = textureSample(indicator_tex, indicator_sampler, in.uv).r;

    var color = hdr_color;

    // Gamma correct (simple)
    color = pow(clamp(color, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2));

    // Apply wallhack overlay tint where stencil indicator > 0
    if (indicator > 0.5) {
        let overlay_color = vec3f(1.0, 0.55, 0.0);
        let overlay_alpha = 0.4;
        color = mix(color, overlay_color, overlay_alpha);
    }

    return vec4f(color, 1.0);
}
"#;
        std::fs::write(&tonemap_shader_path, tonemap_wgsl)
            .unwrap_or_else(|e| log::error!("Failed to write tonemap shader: {}", e));
    }

    // Create sphere mesh
    let sphere = create_uv_sphere(1.0, 16, 16);
    log::info!(
        "Sphere mesh: {} vertices, {} indices",
        sphere.positions.len() / 3,
        sphere.indices.len()
    );

    let vertex_buffer = match GpuBuffer::new(
        &context,
        bytemuck::cast_slice(&sphere.positions),
        vk::BufferUsageFlags::VERTEX_BUFFER,
        "sphere_vertices",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create vertex buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    let index_buffer = match GpuBuffer::new(
        &context,
        bytemuck::cast_slice(&sphere.indices),
        vk::BufferUsageFlags::INDEX_BUFFER,
        "sphere_indices",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create index buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    // Camera setup: looking down -Z, positioned at z=5
    let view = mat4_look_at([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let proj = mat4_perspective(
        (60.0_f32).to_radians(),
        WIDTH as f32 / HEIGHT as f32,
        0.1,
        100.0,
    );
    let vp = mat4_mul(&proj, &view);
    let inv_vp = mat4_inverse(&vp);

    let frame_uniforms = FrameUniforms {
        view,
        proj,
        inv_view_proj: inv_vp,
        camera_position: [0.0, 0.0, 5.0, 1.0],
        light_direction: [0.0, -1.0, -1.0, 0.0],
        light_color: [1.0, 1.0, 1.0, 1.0],
        light_intensity: [1.0, 0.0, 0.0, 0.0],
        tiles: [0, 0, 0, 0],
    };

    // Object 0: occluder sphere at z=0 (not selected, rendered in depth prepass only)
    let obj0_model = mat4_translate(0.0, 0.0, 0.0);
    let obj0 = ObjectUniforms {
        model: obj0_model,
        base_color: [0.5, 0.5, 0.5, 1.0],
        material_params: [0.0, 0.5, 1.0, 0.0],
        texture_indices: [0, 0, 0, 0],
    };

    // Object 1: selected sphere at z=-2, x=0.5 (partially behind occluder, partially visible)
    // Camera at z=5 looking at origin. Occluder at z=0 (distance 5).
    // Selected at z=-2 (distance 7) — behind occluder but offset so ~40% sticks out.
    let obj1_model = mat4_translate(0.5, 0.0, -2.0);
    let obj1 = ObjectUniforms {
        model: obj1_model,
        base_color: [0.2, 0.5, 0.8, 1.0],
        material_params: [0.0, 0.5, 1.0, 0.0],
        texture_indices: [0, 0, 0, 0],
    };

    let frame_uniforms_array = [frame_uniforms];
    let frame_data = bytemuck::cast_slice(&frame_uniforms_array);
    let objects_array = [obj0, obj1];
    let object_data = bytemuck::cast_slice(&objects_array);

    let frame_buffer = match GpuBuffer::new(
        &context,
        frame_data,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        "frame_uniforms",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create frame buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    let object_buffer = match GpuBuffer::new(
        &context,
        object_data,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        "object_uniforms",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create object buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    // Create image resources
    let images = match create_image_resources(&context) {
        Ok(i) => {
            log::info!("Image resources created");
            i
        }
        Err(e) => {
            log::error!("Failed to create image resources: {}", e);
            return ExitCode::from(1);
        }
    };

    // Test GPU rendering capability before proceeding
    log::info!("Testing GPU rendering capability...");
    match test_gpu_rendering_capability(&context, &images) {
        Ok(true) => log::info!("GPU rendering capability: OK"),
        Ok(false) => {
            log::error!("GPU rendering capability: FAILED");
            log::error!("  The GPU does not produce rendering output in headless mode.");
            log::error!("  This is a known issue with some Intel integrated graphics drivers.");
            log::error!("  The stencil validation cannot proceed without GPU rendering support.");
            log::error!(
                "  Exit code 2 = GPU headless rendering not supported (not a test failure)"
            );
            return ExitCode::from(2);
        }
        Err(e) => {
            log::error!("GPU rendering capability test error: {}", e);
            return ExitCode::from(1);
        }
    }

    // Create pipelines
    let mut pipelines = match create_pipelines(&context, &shader_dir, images.indicator_view) {
        Ok(p) => {
            log::info!("Pipelines created");
            p
        }
        Err(e) => {
            log::error!("Failed to create pipelines: {}", e);
            return ExitCode::from(1);
        }
    };

    // Update descriptor sets with buffer info
    unsafe {
        let frame_info = [vk::DescriptorBufferInfo::default()
            .buffer(frame_buffer.buffer)
            .offset(0)
            .range(frame_data.len() as u64)];
        let object_info = [vk::DescriptorBufferInfo::default()
            .buffer(object_buffer.buffer)
            .offset(0)
            .range(object_data.len() as u64)];
        context.device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&object_info),
            ],
            &[],
        );
    }

    // Create sampler
    let sampler = match create_sampler(&context) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to create sampler: {}", e);
            return ExitCode::from(1);
        }
    };

    // Render and readback
    log::info!("Rendering frame...");
    let (ldr_pixels, indicator_pixels, depth_pixels, picking_pixels, stencil_pixels) =
        match render_and_readback(
            &context,
            &images,
            &pipelines,
            &frame_buffer,
            &object_buffer,
            &vertex_buffer,
            &index_buffer,
            sphere.indices.len() as u32,
            sampler,
        ) {
            Ok(r) => {
                log::info!("Frame rendered and read back");
                r
            }
            Err(e) => {
                log::error!("Failed to render: {}", e);
                return ExitCode::from(1);
            }
        };

    // Run validations
    let mut failed = false;

    // Stage 1: Verify depth prepass produced real depth values and entity IDs
    match validate_depth_prepass(&depth_pixels, &picking_pixels) {
        Ok(_) => log::info!("PASS: depth_prepass"),
        Err(e) => {
            log::error!("FAIL: depth_prepass: {}", e);
            failed = true;
        }
    }

    // Stage 2: Verify stencil indicator has non-zero pixels (stencil=2 survived)
    match validate_stencil_indicator(&indicator_pixels, &stencil_pixels) {
        Ok(_) => log::info!("PASS: stencil_indicator"),
        Err(e) => {
            log::error!("FAIL: stencil_indicator: {}", e);
            failed = true;
        }
    }

    // Stage 3: Verify outline draw pass produced orange pixels
    match validate_outline_present(&ldr_pixels) {
        Ok(_) => log::info!("PASS: outline_present"),
        Err(e) => {
            log::error!("FAIL: outline_present: {}", e);
            failed = true;
        }
    }

    // Stage 4: Verify tonemap overlay blends indicator into final output
    match validate_overlay_in_tonemap(&ldr_pixels, &indicator_pixels) {
        Ok(_) => log::info!("PASS: overlay_tonemap"),
        Err(e) => {
            log::error!("FAIL: overlay_tonemap: {}", e);
            failed = true;
        }
    }

    // Stage 5: Verify no self-occlusion artifacts in stencil distribution
    match validate_self_occlusion(&stencil_pixels) {
        Ok(_) => log::info!("PASS: self_occlusion"),
        Err(e) => {
            log::error!("FAIL: self_occlusion: {}", e);
            failed = true;
        }
    }

    // ===================================================================
    // Stage 6: Two-plane self-occlusion test
    // ===================================================================
    // Two quads at different depths, both part of the same selected object.
    // NO occluder in the depth prepass. This definitively tests whether
    // self-occlusion is fixed — if the back quad gets marked as occluded
    // (stencil=2), self-occlusion is still happening.
    log::info!("--- Two-plane self-occlusion test ---");

    let quad_pair = create_quad_pair(-1.0, -1.2, 0.8);
    log::info!(
        "Quad pair mesh: {} vertices, {} indices (front z=-1.0, back z=-1.2)",
        quad_pair.positions.len() / 3,
        quad_pair.indices.len()
    );

    let quad_vertex_buffer = match GpuBuffer::new(
        &context,
        bytemuck::cast_slice(&quad_pair.positions),
        vk::BufferUsageFlags::VERTEX_BUFFER,
        "quad_pair_vertices",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create quad vertex buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    let quad_index_buffer = match GpuBuffer::new(
        &context,
        bytemuck::cast_slice(&quad_pair.indices),
        vk::BufferUsageFlags::INDEX_BUFFER,
        "quad_pair_indices",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create quad index buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    // Set up uniforms for the two-plane test.
    // Instance 1 = selected object (two quads). No instance 0 needed.
    let quad_obj1 = ObjectUniforms {
        model: mat4_identity(),
        base_color: [0.5, 0.7, 1.0, 1.0],
        material_params: [0.0, 0.5, 1.0, 0.0],
        texture_indices: [0, 0, 0, 0],
    };
    let quad_objects_array = [quad_obj1];
    let quad_object_data = bytemuck::cast_slice(&quad_objects_array);

    let quad_object_buffer = match GpuBuffer::new(
        &context,
        quad_object_data,
        vk::BufferUsageFlags::STORAGE_BUFFER,
        "quad_object_uniforms",
    ) {
        Ok(b) => b,
        Err(e) => {
            log::error!("Failed to create quad object buffer: {}", e);
            return ExitCode::from(1);
        }
    };

    // Update descriptor sets with the quad object buffer
    unsafe {
        let frame_info = [vk::DescriptorBufferInfo::default()
            .buffer(frame_buffer.buffer)
            .offset(0)
            .range(frame_data.len() as u64)];
        let quad_obj_info = [vk::DescriptorBufferInfo::default()
            .buffer(quad_object_buffer.buffer)
            .offset(0)
            .range(quad_object_data.len() as u64)];
        context.device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.descriptor_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&frame_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(pipelines.descriptor_set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .buffer_info(&quad_obj_info),
            ],
            &[],
        );
    }

    // Restore image layouts (they were left in TRANSFER_SRC after the first test's readback)
    let cmd_buf_layouts = context.begin_single_time_commands();
    let cmd_layouts = cmd_buf_layouts.vk_command_buffer();
    let restore_color_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    let restore_ds_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };
    transition_image_layout(
        cmd_layouts,
        &context.device,
        images.ldr_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        restore_color_range,
    );
    transition_image_layout(
        cmd_layouts,
        &context.device,
        images.indicator_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        restore_color_range,
    );
    transition_image_layout(
        cmd_layouts,
        &context.device,
        images.depth_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        restore_ds_range,
    );
    transition_image_layout(
        cmd_layouts,
        &context.device,
        images.picking_image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        restore_color_range,
    );
    if let Err(e) = submit_and_wait(&context, &cmd_buf_layouts) {
        log::error!("Failed to restore image layouts: {}", e);
        return ExitCode::from(1);
    }

    log::info!("Rendering two-plane self-occlusion test...");
    let (
        _quad_ldr_pixels,
        quad_indicator_pixels,
        _quad_depth_pixels,
        quad_picking_pixels,
        quad_stencil_pixels,
    ) = match render_self_occlusion_test(
        &context,
        &images,
        &pipelines,
        &quad_vertex_buffer,
        &quad_index_buffer,
        quad_pair.indices.len() as u32,
        sampler,
    ) {
        Ok(r) => {
            log::info!("Two-plane test rendered and read back");
            r
        }
        Err(e) => {
            log::error!("Failed to render two-plane test: {}", e);
            return ExitCode::from(1);
        }
    };

    // Verify depth prepass drew nothing (no occluder objects)
    let quad_picking_nonzero = quad_picking_pixels.iter().filter(|&&p| p > 0).count();
    log::info!(
        "  Two-plane picking: {} non-zero pixels (expected 0)",
        quad_picking_nonzero
    );

    if quad_picking_nonzero > 0 {
        log::error!(
            "FAIL: two_plane_self_occlusion: depth prepass should draw nothing, found {} entity pixels",
            quad_picking_nonzero
        );
        failed = true;
    }

    match validate_no_self_occlusion(&quad_indicator_pixels, &quad_stencil_pixels) {
        Ok(_) => log::info!("PASS: two_plane_self_occlusion"),
        Err(e) => {
            log::error!("FAIL: two_plane_self_occlusion: {}", e);
            failed = true;
        }
    }

    quad_vertex_buffer.destroy(&context);
    quad_index_buffer.destroy(&context);
    quad_object_buffer.destroy(&context);

    // Cleanup
    unsafe {
        context.device.destroy_sampler(sampler, None);
        context
            .device
            .destroy_pipeline(pipelines.depth_prepass_pipeline, None);
        pipelines.stencil_mark_pipeline.destroy();
        pipelines.occlusion_mark_pipeline.destroy();
        pipelines.outline_draw_pipeline.destroy();
        pipelines.stencil_indicator_pipeline.destroy();
        context
            .device
            .destroy_pipeline(pipelines.tonemap_pipeline, None);
        context
            .device
            .destroy_pipeline_layout(pipelines.pipeline_layout, None);
        context
            .device
            .destroy_pipeline_layout(pipelines.tonemap_pipeline_layout, None);
        context
            .device
            .destroy_descriptor_set_layout(pipelines.descriptor_set_layout, None);
        context
            .device
            .destroy_descriptor_set_layout(pipelines.tonemap_descriptor_set_layout, None);
        context
            .device
            .destroy_descriptor_pool(pipelines.descriptor_pool, None);
        context
            .device
            .destroy_descriptor_pool(pipelines.tonemap_descriptor_pool, None);
        context.device.destroy_image_view(images.depth_view, None);
        context
            .device
            .destroy_image_view(images.depth_stencil_view, None);
        context.device.destroy_image_view(images.hdr_view, None);
        context
            .device
            .destroy_image_view(images.indicator_view, None);
        context.device.destroy_image_view(images.ldr_view, None);
        context.device.destroy_image_view(images.picking_view, None);
        context.device.destroy_image(images.depth_image, None);
        context.device.destroy_image(images.hdr_image, None);
        context.device.destroy_image(images.indicator_image, None);
        context.device.destroy_image(images.ldr_image, None);
        context.device.destroy_image(images.picking_image, None);
        context.device.destroy_buffer(images.ldr_staging, None);
        context
            .device
            .destroy_buffer(images.indicator_staging, None);
        context.device.destroy_buffer(images.depth_staging, None);
        context.device.destroy_buffer(images.picking_staging, None);
        context.device.destroy_buffer(images.stencil_staging, None);
    }

    context
        .allocator
        .free(images.depth_allocation, "outline depth");
    context.allocator.free(images.hdr_allocation, "outline HDR");
    context
        .allocator
        .free(images.indicator_allocation, "outline indicator");
    context
        .allocator
        .free(images.picking_allocation, "outline picking");
    context.allocator.free(images.ldr_allocation, "outline LDR");
    context
        .allocator
        .free(images.ldr_staging_allocation, "outline LDR staging");
    context.allocator.free(
        images.indicator_staging_allocation,
        "outline indicator staging",
    );
    context
        .allocator
        .free(images.depth_staging_allocation, "outline depth staging");
    context
        .allocator
        .free(images.picking_staging_allocation, "outline picking staging");
    context
        .allocator
        .free(images.stencil_staging_allocation, "outline stencil staging");

    vertex_buffer.destroy(&context);
    index_buffer.destroy(&context);
    frame_buffer.destroy(&context);
    object_buffer.destroy(&context);

    // Clean up the generated shader
    let _ = std::fs::remove_file(tonemap_shader_path);

    if failed {
        log::error!("=== Outline Validation FAILED ===");
        ExitCode::from(1)
    } else {
        log::info!("=== All Outline Validations Passed ===");
        ExitCode::SUCCESS
    }
}
