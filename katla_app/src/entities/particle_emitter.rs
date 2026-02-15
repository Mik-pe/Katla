//! Particle emitter entity factory.
//!
//! Creates a particle emitter entity with all required GPU resources.

use std::rc::Rc;

use ash::vk;
use katla_ecs::World;
use katla_math::{Transform, Vec3};
use katla_vulkan::{
    BufferDescriptorSetBuilder, ComputePipelineBuilder, DeviceAddressBuffer, EmitterConfig,
    ParticleBuffer, VulkanContext,
};

use crate::components::{ParticleEmitter, TransformComponent};

/// Create a particle emitter entity.
///
/// This creates:
/// - A ParticleBuffer for GPU particle storage
/// - A FrameDataBuffer for per-frame uniform data
/// - A ComputePipeline for particle simulation
/// - A BufferDescriptorSet for binding
/// - A ParticleEmitter component with the configuration
///
/// # Arguments
/// * `world` - ECS world to add the entity to
/// * `context` - Vulkan context for GPU resource creation
/// * `position` - World position of the emitter
/// * `emit_rate` - Particles per second to emit
///
/// # Returns
/// The entity ID of the created particle emitter.
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
        DeviceAddressBuffer::new_persistent(context.clone(), 16).expect("Failed to create frame data buffer");

    // Create descriptor set layout for particle buffer + frame data
    let bindings = [
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

    let layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

    let descriptor_layout = unsafe {
        context
            .device
            .create_descriptor_set_layout(&layout_info, None)
            .expect("Failed to create descriptor set layout")
    };

    // Create descriptor set with both buffers
    let descriptor_set = BufferDescriptorSetBuilder::new(&context)
        .add_entire_buffer(&particle_buffer, 0)
        .with_descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .add_entire_buffer(&frame_data_buffer, 1)
        .build(descriptor_layout)
        .expect("Failed to create particle descriptor set");

    // Load compute shader
    let shader_code = include_str!("../../../resources/shaders/particles/particle_sim.wgsl");
    let shader_module = katla_vulkan::ShaderModule::from_wgsl_string(
        context.device.clone(),
        shader_code,
        vk::ShaderStageFlags::COMPUTE,
        "cs_main",
    )
    .expect("Failed to compile particle compute shader");

    // Create compute pipeline
    let compute_pipeline = ComputePipelineBuilder::new(context.clone())
        .with_shader(shader_module.module)
        .with_descriptor_layouts(vec![descriptor_layout])
        .build()
        .expect("Failed to create compute pipeline");

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
        descriptor_set,
        config,
        emit_rate,
    );

    // Create entity with transform and emitter
    let entity = world.create_entity();
    world.add_component(
        entity,
        TransformComponent::new(Transform::new_from_position(position)),
    );
    world.add_component(entity, emitter);

    entity
}
