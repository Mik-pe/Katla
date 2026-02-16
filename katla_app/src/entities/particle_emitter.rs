//! Particle emitter entity factory.
//!
//! Creates a particle emitter entity with all required GPU resources.

use std::rc::Rc;

use ash::vk;
use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{
    BufferDescriptorSetBuilder, ComputePipelineBuilder, DeviceAddressBuffer, EmitterConfig,
    MaterialPipeline, ParticleBuffer, Pipeline, PipelineBuilder, ShaderModule, VulkanContext,
};

use crate::components::{NameComponent, ParticleEmitter, TransformComponent};

/// Create a particle emitter entity.
///
/// This creates all GPU resources needed for particle simulation and rendering.
pub fn create_particle_emitter(
    world: &mut World,
    context: Rc<VulkanContext>,
    position: Vec3,
    emit_rate: f32,
) -> katla_ecs::EntityId {
    // Create particle buffer (64K particles max)
    let particle_buffer = ParticleBuffer::with_max_capacity(context.clone())
        .expect("Failed to create particle buffer");

    // Create frame data uniform buffer (16 bytes)
    let frame_data_buffer =
        DeviceAddressBuffer::new_persistent(context.clone(), 16)
            .expect("Failed to create frame data buffer");

    // === COMPUTE PIPELINE ===
    // Descriptor layout: binding 0 = particles (storage), binding 1 = frame data (uniform)
    let compute_bindings = [
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE),
    ];

    let compute_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&compute_bindings);

    let compute_descriptor_layout = unsafe {
        context
            .device
            .create_descriptor_set_layout(&compute_layout_info, None)
            .expect("Failed to create compute descriptor layout")
    };

    // Create compute descriptor set
    let compute_descriptor_set = BufferDescriptorSetBuilder::new(&context)
        .add_entire_buffer(&particle_buffer, 0)
        .with_descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .add_entire_buffer(&frame_data_buffer, 1)
        .build(compute_descriptor_layout)
        .expect("Failed to create compute descriptor set");

    // Load and compile compute shader
    let compute_shader_code = include_str!("../../../resources/shaders/particles/particle_sim.wgsl");
    let compute_shader = ShaderModule::from_wgsl_string(
        context.device.clone(),
        compute_shader_code,
        vk::ShaderStageFlags::COMPUTE,
        "cs_main",
    )
    .expect("Failed to compile particle compute shader");

    // Create compute pipeline with push constant range for frame data
    // Push constants: delta_time, emit_count, max_particles, random_seed (4 x f32 = 16 bytes)
    let compute_pipeline = ComputePipelineBuilder::new(context.clone())
        .with_shader(compute_shader.module)
        .with_descriptor_layouts(vec![compute_descriptor_layout])
        .add_push_constant_range(vk::ShaderStageFlags::COMPUTE, 0, 16)
        .build()
        .expect("Failed to create compute pipeline");

    // === RENDER PIPELINE ===
    // Descriptor layout for rendering: set 0 = frame uniforms (storage), set 1 = particles (storage)
    // Note: Set 0 is bound by the renderer from storage_manager

    // Particle buffer descriptor layout (set 1)
    let render_particle_bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX)];

    let render_particle_layout_info =
        vk::DescriptorSetLayoutCreateInfo::default().bindings(&render_particle_bindings);

    let render_particle_descriptor_layout = unsafe {
        context
            .device
            .create_descriptor_set_layout(&render_particle_layout_info, None)
            .expect("Failed to create render particle descriptor layout")
    };

    // Create render particle descriptor set
    let render_particle_descriptor = BufferDescriptorSetBuilder::new(&context)
        .add_entire_buffer(&particle_buffer, 0)
        .build(render_particle_descriptor_layout)
        .expect("Failed to create render particle descriptor set");

    // Frame uniforms descriptor layout (set 0) - MUST match StorageUniforms layout
    // The renderer binds storage_descriptor_set which has TWO bindings:
    // - Binding 0: frame_data (FrameUniforms)
    // - Binding 1: objects array (ObjectUniforms[])
    // We only use binding 0, but the layout must match for compatibility.
    let frame_bindings = [
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

    let frame_layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&frame_bindings);

    let frame_descriptor_layout = unsafe {
        context
            .device
            .create_descriptor_set_layout(&frame_layout_info, None)
            .expect("Failed to create frame descriptor layout")
    };

    // Load and compile render shaders
    let render_shader_code = include_str!("../../../resources/shaders/particles/particle_render.wgsl");
    let vertex_shader = ShaderModule::from_wgsl_string(
        context.device.clone(),
        render_shader_code,
        vk::ShaderStageFlags::VERTEX,
        "vs_main",
    )
    .expect("Failed to compile particle vertex shader");

    let fragment_shader = ShaderModule::from_wgsl_string(
        context.device.clone(),
        render_shader_code,
        vk::ShaderStageFlags::FRAGMENT,
        "fs_main",
    )
    .expect("Failed to compile particle fragment shader");

    // Create render pipeline with additive blending for fire effect
    let render_pipeline = PipelineBuilder::new(context.clone())
        .with_shaders(vertex_shader.module, fragment_shader.module)
        .with_descriptor_layouts(vec![frame_descriptor_layout, render_particle_descriptor_layout])
        .with_additive_blending()
        .with_depth_test(true, false, vk::CompareOp::LESS) // depth test but no write
        .with_cull_mode(vk::CullModeFlags::NONE, vk::FrontFace::COUNTER_CLOCKWISE)
        .with_rendering_formats(Some(vk::Format::B8G8R8A8_SRGB), Some(vk::Format::D32_SFLOAT_S8_UINT))
        .build(vk::RenderPass::null()) // Dynamic rendering
        .expect("Failed to create render pipeline");

    let render_pipeline = MaterialPipeline::new(render_pipeline, frame_descriptor_layout, context.clone());

    // Create emitter config
    let config = EmitterConfig {
        position: [position.x(), position.y(), position.z()],
        emit_count: 0,
        velocity_direction: [0.0, 1.0, 0.0],
        base_lifetime: 5.0,
        velocity_magnitude: 2.0,
        velocity_cone_angle: 0.3,
        base_scale: 0.2,
        color: [1.0, 0.6, 0.2, 1.0], // Orange fire color
    };

    // Create particle emitter component
    let emitter = ParticleEmitter::new(
        particle_buffer,
        frame_data_buffer,
        compute_pipeline,
        compute_descriptor_set,
        render_pipeline,
        render_particle_descriptor,
        render_particle_descriptor_layout,
        config,
        emit_rate,
    );

    // Create entity with transform and emitter
    world.spawn((
        TransformComponent::new(Transform::new_from_position(position)),
        emitter,
        NameComponent::new("Particle Emitter"),
    ))
}
