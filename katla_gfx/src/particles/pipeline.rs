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
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .compute_push_descriptor_layout
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
        self.emit_pipeline = Some(pipeline_handle);

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
            .compute_descriptor_layout
            .ok_or("Compute descriptor layout not created")?;

        let compute_push_layout = self
            .compute_push_descriptor_layout
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
        self.simulate_pipeline = Some(pipeline_handle);

        info!("Created particle simulate pipeline");
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
            .render_descriptor_layout
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
        self.render_pipeline = Some(pipeline_handle);

        // Clean up the temporary layout (pipeline holds its own reference)
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(storage_layout, None);
        }

        info!("Created particle render pipeline");
        Ok(())
    }
}
