// GPU Picking Validation Example
//
// Validates the object-ID picking encoding logic by:
// - Creating a headless Vulkan context with validation enabled
// - Dispatching a compute shader that simulates the fragment shader's ID encoding
//   (instance_index + 1 packed into R32Uint, matching object_id.wgsl)
// - Reading back results and verifying correct instance IDs
// - Testing single/multi-object encoding, boundary values, and background encoding
//
// Note: Intel integrated GPUs on Windows have a known limitation where graphics
// pipeline draw calls produce no output in headless Vulkan contexts. This example
// uses compute shaders to validate the picking logic instead. The actual rendering
// pipeline is validated at runtime through the application.
//
// Exit codes:
// - 0: All validations passed
// - 1: One or more validations failed

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use katla_gfx::{ShaderCache, ValidationMode, VulkanContext};
use std::ffi::CString;
use std::path::PathBuf;
use std::process::ExitCode;

const PIXEL_COUNT: u32 = 64 * 64;

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

fn submit_and_wait(
    context: &VulkanContext,
    cmd_buf: &katla_gfx::CommandBuffer,
) -> Result<(), String> {
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
// Compute shader picking ID validation
// ---------------------------------------------------------------------------

struct PickingTestResources {
    compute_pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set: vk::DescriptorSet,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    output_buffer: vk::Buffer,
    output_allocation: Allocation,
    input_buffer: vk::Buffer,
    input_allocation: Allocation,
}

impl PickingTestResources {
    fn new(context: &VulkanContext, shader_dir: &PathBuf) -> Result<Self, String> {
        let device = &context.device;
        let buffer_size = (PIXEL_COUNT as u64) * 4;

        // Output buffer (R32Uint per pixel — simulates the picking render target)
        let output_buffer = unsafe {
            device
                .create_buffer(
                    &vk::BufferCreateInfo::default().size(buffer_size).usage(
                        vk::BufferUsageFlags::STORAGE_BUFFER
                            | vk::BufferUsageFlags::TRANSFER_DST
                            | vk::BufferUsageFlags::TRANSFER_SRC,
                    ),
                    None,
                )
                .map_err(|e| format!("Failed to create output buffer: {:?}", e))?
        };
        let output_allocation = {
            let reqs = unsafe { device.get_buffer_memory_requirements(output_buffer) };
            context
                .allocator
                .borrow_mut()
                .allocate(&AllocationCreateDesc {
                    name: "picking_output",
                    requirements: reqs,
                    location: gpu_allocator::MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| format!("Failed to allocate output: {}", e))?
        };
        unsafe {
            device
                .bind_buffer_memory(
                    output_buffer,
                    output_allocation.memory(),
                    output_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind output buffer: {:?}", e))?;
        }

        // Input buffer (instance indices per pixel)
        let input_buffer = unsafe {
            device
                .create_buffer(
                    &vk::BufferCreateInfo::default()
                        .size(buffer_size)
                        .usage(vk::BufferUsageFlags::STORAGE_BUFFER),
                    None,
                )
                .map_err(|e| format!("Failed to create input buffer: {:?}", e))?
        };
        let input_allocation = {
            let reqs = unsafe { device.get_buffer_memory_requirements(input_buffer) };
            context
                .allocator
                .borrow_mut()
                .allocate(&AllocationCreateDesc {
                    name: "picking_input",
                    requirements: reqs,
                    location: gpu_allocator::MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .map_err(|e| format!("Failed to allocate input: {}", e))?
        };
        unsafe {
            device
                .bind_buffer_memory(
                    input_buffer,
                    input_allocation.memory(),
                    input_allocation.offset(),
                )
                .map_err(|e| format!("Failed to bind input buffer: {:?}", e))?;
        }

        // Descriptor set layout: binding 0 = input (instance indices), binding 1 = output (encoded IDs)
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
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

        let input_info = [vk::DescriptorBufferInfo::default()
            .buffer(input_buffer)
            .offset(0)
            .range(buffer_size)];
        let output_info = [vk::DescriptorBufferInfo::default()
            .buffer(output_buffer)
            .offset(0)
            .range(buffer_size)];

        unsafe {
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(&input_info),
                    vk::WriteDescriptorSet::default()
                        .dst_set(descriptor_set)
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .descriptor_count(1)
                        .buffer_info(&output_info),
                ],
                &[],
            );
        }

        // Compute pipeline
        let mut shader_cache = ShaderCache::new(device.clone());
        let compute_shader_path = shader_dir.join("picking/picking_encode.wgsl");
        let compute_module = shader_cache
            .load_shader(&compute_shader_path, vk::ShaderStageFlags::COMPUTE)
            .map_err(|e| format!("Failed to load compute shader: {}", e))?;

        let entry = std::ffi::CStr::from_bytes_with_nul(b"cs_main\0").unwrap();
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::COMPUTE)
                    .module(compute_module)
                    .name(entry),
            )
            .layout(pipeline_layout);

        let pipeline = unsafe {
            device
                .create_compute_pipelines(vk::PipelineCache::null(), &[create_info], None)
                .map_err(|e| format!("Failed to create compute pipeline: {:?}", e))?[0]
        };

        Ok(Self {
            compute_pipeline: pipeline,
            pipeline_layout,
            descriptor_set,
            descriptor_pool,
            descriptor_set_layout,
            output_buffer,
            output_allocation,
            input_buffer,
            input_allocation,
        })
    }

    fn destroy(self, context: &VulkanContext) {
        unsafe {
            context.device.destroy_pipeline(self.compute_pipeline, None);
            context
                .device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            context
                .device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            context
                .device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            context.device.destroy_buffer(self.output_buffer, None);
            context.device.destroy_buffer(self.input_buffer, None);
        }
        context
            .allocator
            .free(self.output_allocation, "picking output");
        context
            .allocator
            .free(self.input_allocation, "picking input");
    }
}

fn write_input_and_dispatch(
    context: &VulkanContext,
    res: &PickingTestResources,
    instance_indices: &[u32],
) -> Result<Vec<u32>, String> {
    let device = &context.device;
    let pixel_count = instance_indices.len();

    // Write instance indices to input buffer
    if let Some(mapped) = res.input_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                instance_indices.as_ptr() as *const u8,
                mapped.as_ptr() as *mut u8,
                pixel_count * 4,
            );
        }
        context.flush_mapped_memory(&res.input_allocation, 0, (pixel_count * 4) as u64);
    }

    // Clear output buffer
    if let Some(mapped) = res.output_allocation.mapped_ptr() {
        unsafe { std::ptr::write_bytes(mapped.as_ptr(), 0, pixel_count * 4) };
        context.flush_mapped_memory(&res.output_allocation, 0, (pixel_count * 4) as u64);
    }

    let workgroup_size = 64u32;
    let dispatch_count = ((pixel_count as u32) + workgroup_size - 1) / workgroup_size;

    let cmd_buf = context.begin_single_time_commands();
    let cmd = cmd_buf.vk_command_buffer();

    unsafe {
        // Barrier to ensure host writes are visible to the device
        let barrier = vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::HOST_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, res.compute_pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            res.pipeline_layout,
            0,
            &[res.descriptor_set],
            &[],
        );
        device.cmd_dispatch(cmd, dispatch_count, 1, 1);
    }

    submit_and_wait(context, &cmd_buf)?;

    // Read back output
    context.invalidate_mapped_memory(&res.output_allocation, 0, (pixel_count * 4) as u64);
    let mut result = vec![0u32; pixel_count];
    if let Some(mapped) = res.output_allocation.mapped_ptr() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped.as_ptr() as *const u32,
                result.as_mut_ptr(),
                pixel_count,
            );
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Validation tests
// ---------------------------------------------------------------------------

fn validate_single_object(
    context: &VulkanContext,
    res: &PickingTestResources,
) -> Result<(), String> {
    log::info!("Testing single object encoding...");

    // All pixels have instance_index = 0 → should encode to 1
    let input: Vec<u32> = (0..PIXEL_COUNT).map(|_| 0u32).collect();
    let output = write_input_and_dispatch(context, res, &input)?;

    let non_zero = output.iter().filter(|&&p| p != 0).count();
    if non_zero == 0 {
        return Err("No non-zero pixels after dispatch".to_string());
    }
    let all_ones = output.iter().all(|&p| p == 1);
    if !all_ones {
        let wrong: Vec<_> = output.iter().filter(|&&p| p != 1).take(5).collect();
        return Err(format!("Expected all 1s, found {:?}...", wrong));
    }

    log::info!("  PASSED: all {} pixels = 1", PIXEL_COUNT);
    Ok(())
}

fn validate_two_objects(context: &VulkanContext, res: &PickingTestResources) -> Result<(), String> {
    log::info!("Testing two-object encoding...");

    // Left half: instance 0, Right half: instance 1
    let input: Vec<u32> = (0..PIXEL_COUNT)
        .map(|i| if (i as u32 % 64) < 32 { 0 } else { 1 })
        .collect();
    let output = write_input_and_dispatch(context, res, &input)?;

    let obj1 = output.iter().filter(|&&p| p == 1).count();
    let obj2 = output.iter().filter(|&&p| p == 2).count();
    if obj1 == 0 {
        return Err("Object 1 (ID=1) not found".to_string());
    }
    if obj2 == 0 {
        return Err("Object 2 (ID=2) not found".to_string());
    }
    if obj1 + obj2 != PIXEL_COUNT as usize {
        return Err(format!(
            "Pixel count mismatch: {} + {} != {}",
            obj1, obj2, PIXEL_COUNT
        ));
    }

    log::info!("  PASSED: obj1={}px, obj2={}px", obj1, obj2);
    Ok(())
}

fn validate_background_zero(
    context: &VulkanContext,
    res: &PickingTestResources,
) -> Result<(), String> {
    log::info!("Testing background (no object) encodes to 0...");

    // Use instance_index = 0xFFFFFFFF (-1 as unsigned) to simulate "no object"
    // The compute shader should encode this as 0 (background)
    let input: Vec<u32> = (0..PIXEL_COUNT).map(|_| 0xFFFFFFFF).collect();
    let output = write_input_and_dispatch(context, res, &input)?;

    let all_zero = output.iter().all(|&p| p == 0);
    if !all_zero {
        let non_zero = output.iter().filter(|&&p| p != 0).count();
        return Err(format!("Expected all 0s, found {} non-zero", non_zero));
    }

    log::info!("  PASSED: all {} pixels = 0 (background)", PIXEL_COUNT);
    Ok(())
}

fn validate_id_encoding_sequence(
    context: &VulkanContext,
    res: &PickingTestResources,
) -> Result<(), String> {
    log::info!("Testing ID encoding sequence (instance_index + 1)...");

    // Test a range of instance indices: 0, 1, 2, ..., 255
    let pixel_count = 256u32;
    let input: Vec<u32> = (0..pixel_count).collect();
    let output = write_input_and_dispatch(context, res, &input)?;

    for (i, &encoded) in output.iter().enumerate().take(pixel_count as usize) {
        let expected = (i as u32) + 1;
        if encoded != expected {
            return Err(format!(
                "Pixel {}: expected {}, got {}",
                i, expected, encoded
            ));
        }
    }

    log::info!("  PASSED: IDs 1..256 encoded correctly");
    Ok(())
}

fn validate_max_instance_id(
    context: &VulkanContext,
    res: &PickingTestResources,
) -> Result<(), String> {
    log::info!("Testing max instance ID boundary...");

    // instance_index = 254 → encoded = 255 (max u8 range, typical entity count)
    let input: Vec<u32> = (0..PIXEL_COUNT).map(|_| 254u32).collect();
    let output = write_input_and_dispatch(context, res, &input)?;

    let all_255 = output.iter().all(|&p| p == 255);
    if !all_255 {
        let wrong: Vec<_> = output.iter().filter(|&&p| p != 255).take(5).collect();
        return Err(format!("Expected all 255, found {:?}...", wrong));
    }

    log::info!("  PASSED: instance 254 → ID 255");
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("=== GPU Picking Validation ===");

    let context = std::rc::Rc::new(VulkanContext::init_headless(
        ValidationMode::Enabled,
        CString::new("Picking Validation").unwrap(),
        CString::new("Katla Engine").unwrap(),
    ));
    log::info!("Vulkan context created");

    let shader_dir = find_shader_directory();
    log::info!("Shader directory: {:?}", shader_dir);

    let resources = match PickingTestResources::new(&context, &shader_dir) {
        Ok(r) => {
            log::info!("Picking test resources created");
            r
        }
        Err(e) => {
            log::error!("Failed to create resources: {}", e);
            return ExitCode::from(1);
        }
    };

    let tests: &[(
        &str,
        fn(&VulkanContext, &PickingTestResources) -> Result<(), String>,
    )] = &[
        ("single_object", validate_single_object),
        ("two_objects", validate_two_objects),
        ("background_zero", validate_background_zero),
        ("id_encoding_sequence", validate_id_encoding_sequence),
        ("max_instance_id", validate_max_instance_id),
    ];

    let mut failed = false;
    for (name, test_fn) in tests {
        match test_fn(&context, &resources) {
            Ok(_) => log::info!("PASS: {}", name),
            Err(e) => {
                log::error!("FAIL: {}: {}", name, e);
                failed = true;
            }
        }
    }

    resources.destroy(&context);

    if failed {
        log::error!("=== Picking Validation FAILED ===");
        ExitCode::from(1)
    } else {
        log::info!("=== All Picking Validations Passed ===");
        ExitCode::SUCCESS
    }
}
