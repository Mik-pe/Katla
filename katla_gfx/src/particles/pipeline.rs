use ash::vk;
use log::info;

use crate::renderer::registry::AssetRegistry;
use crate::sync::VkShaderModule;

use super::GlobalParticleSystem;

impl GlobalParticleSystem {
    /// Create emit pipeline for particle emission.
    pub fn create_emit_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

        let compute_layout = self
            .descriptors
            .compute_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .descriptors
            .compute_push_layout
            .ok_or("Compute push descriptor layout not created")?;

        let emit_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![
                crate::sync::VkDescriptorSetLayout(compute_layout),
                crate::sync::VkDescriptorSetLayout(compute_push_layout),
            ])
            .build()
            .map_err(|e| format!("Failed to build emit pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(emit_pipeline);
        self.pipelines.emit = Some(pipeline_handle);

        info!("Created particle emit pipeline");
        Ok(())
    }

    /// Create simulate pipeline for particle simulation.
    pub fn create_simulate_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

        let compute_layout = self
            .descriptors
            .compute_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .descriptors
            .compute_push_layout
            .ok_or("Compute push descriptor layout not created")?;

        let simulate_pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![
                crate::sync::VkDescriptorSetLayout(compute_layout),
                crate::sync::VkDescriptorSetLayout(compute_push_layout),
            ])
            .build()
            .map_err(|e| format!("Failed to build simulate pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_compute_pipeline(simulate_pipeline);
        self.pipelines.simulate = Some(pipeline_handle);

        info!("Created particle simulate pipeline");
        Ok(())
    }

    /// Create draw command finalization pipeline.
    ///
    /// This pipeline reads the post-simulate alive_count and writes the
    /// indirect draw command. It must be dispatched as a single 1x1x1
    /// workgroup AFTER the simulate dispatch with a pipeline barrier.
    pub fn create_draw_command_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        shader_module: VkShaderModule,
    ) -> Result<(), String> {
        use crate::vulkan::material::compute_pipeline::ComputePipelineBuilder;

        let device = &self.context.device;

        // Descriptor layout: 2 storage buffer bindings (push descriptors)
        // Binding 0: counters (ParticleCounters)
        // Binding 1: indirect draw command (DrawIndirectCommand, 16 bytes)
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

        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .bindings(&bindings)
            .flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR);

        let descriptor_layout = unsafe {
            device
                .create_descriptor_set_layout(&layout_info, None)
                .map_err(|e| format!("Failed to create draw command descriptor layout: {:?}", e))?
        };

        let pipeline = ComputePipelineBuilder::new(self.context.clone())
            .with_shader(shader_module)
            .with_descriptor_layouts(vec![crate::sync::VkDescriptorSetLayout(descriptor_layout)])
            .build()
            .map_err(|e| format!("Failed to build draw command pipeline: {}", e))?;

        self.descriptors.draw_command_set = None;
        self.descriptors.draw_command_layout = Some(descriptor_layout);
        self.descriptors._draw_command_pool = vk::DescriptorPool::null();

        let pipeline_handle = asset_registry.register_compute_pipeline(pipeline);
        self.pipelines.draw_command = Some(pipeline_handle);

        info!("Created particle draw command pipeline (push descriptors)");
        Ok(())
    }

    /// Create render pipeline for particle rendering.
    ///
    /// Note: Particle rendering uses 2 descriptor sets:
    /// - Set 0: Particle buffers (particles, alive_list, etc.)
    /// - Set 1: Standard renderer storage uniforms (view/proj matrices)
    ///   The render graph will bind Set 1 automatically during particle rendering.
    pub fn create_render_pipeline(
        &mut self,
        asset_registry: &mut AssetRegistry,
        vertex_shader: VkShaderModule,
        fragment_shader: VkShaderModule,
    ) -> Result<(), String> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::texture::ImageFormat;
        use crate::vulkan::material::builder::PipelineBuilder;

        let render_layout = self
            .descriptors
            .render_layout
            .ok_or("Render descriptor layout not created")?;

        // Create a storage descriptor layout matching the renderer's storage uniforms
        // This must match exactly what StorageDescriptorSet creates
        let storage_bindings = [
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

        let storage_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&storage_bindings);

        let storage_layout = unsafe {
            self.context
                .device
                .create_descriptor_set_layout(&storage_layout_info, None)
                .map_err(|e| format!("Failed to create storage descriptor layout: {:?}", e))?
        };

        let pipeline = PipelineBuilder::new(self.context.clone())
            .with_shaders(vertex_shader.vk(), fragment_shader.vk())
            .with_descriptor_layouts(vec![render_layout, storage_layout])
            .with_depth_test(true, false, crate::pipeline::CompareOp::Greater)
            .with_alpha_blending()
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            .with_rendering_formats(
                Some(ImageFormat::R16G16B16A16Sfloat),
                Some(ImageFormat::D32SfloatS8Uint),
            );

        let pipeline = pipeline
            .build_dynamic()
            .map_err(|e| format!("Failed to build render pipeline: {}", e))?;

        let pipeline_handle = asset_registry.register_pipeline(pipeline);
        self.pipelines.render = Some(pipeline_handle);

        // The pipeline layout holds a reference to the storage layout internally,
        // so we store it for the lifetime of the particle system and clean up in destroy()
        self.descriptors.particle_render_storage_layout = Some(storage_layout);

        info!("Created particle render pipeline");
        Ok(())
    }
}
