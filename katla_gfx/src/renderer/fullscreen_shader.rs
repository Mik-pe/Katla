use super::*;

impl VulkanRenderer {
    /// Compile a fullscreen/post-processing shader and return its pipeline handle.
    ///
    /// This is intended for post-processing effects like tonemapping, bloom, etc.
    /// The shader should generate a fullscreen triangle using `@builtin(vertex_index)`
    /// and sample from input textures.
    ///
    /// # Arguments
    ///
    /// * `shader_path` - Path to the WGSL shader file (contains both vertex and fragment)
    ///
    /// # Returns
    ///
    /// A `PipelineHandle` that can be passed to `FullscreenPass::pipeline()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tonemap_pipeline = renderer.compile_fullscreen_shader(PathBuf::from("shaders/tonemapping.wgsl"))?;
    ///
    /// let graph = renderer.create_frame_graph()
    ///     .add_pass(FullscreenPass::new("tonemap")
    ///         .read("hdr_color")
    ///         .write_backbuffer()
    ///         .pipeline(tonemap_pipeline))
    ///     .build()?;
    /// ```
    pub fn compile_fullscreen_shader(
        &mut self,
        shader_path: std::path::PathBuf,
    ) -> Result<crate::handle::PipelineHandle, RendererError> {
        self.compile_fullscreen_shader_with_format(
            shader_path,
            crate::texture::ImageFormat::B8G8R8A8Srgb,
        )
    }

    /// Compile a fullscreen/post-processing shader with custom color format.
    ///
    /// Unlike `compile_fullscreen_shader()` which uses swapchain format,
    /// this allows specifying a custom color format for rendering to
    /// intermediate textures (e.g., HDR render targets).
    ///
    /// # Arguments
    /// * `shader_path` - Path to the WGSL shader file (contains both vertex and fragment)
    /// * `color_format` - Color attachment format for rendering
    ///
    /// # Returns
    ///
    /// A `PipelineHandle` that can be passed to `FullscreenPass::pipeline()`.
    pub fn compile_fullscreen_shader_with_format(
        &mut self,
        shader_path: std::path::PathBuf,
        color_format: crate::texture::ImageFormat,
    ) -> Result<crate::handle::PipelineHandle, RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;

        use ash::vk;

        // Create storage descriptor layout for fullscreen pass
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
                .map_err(|e| {
                    RendererError::InitializationFailed(format!("Descriptor layout: {:?}", e))
                })?
        };

        let bindless_layout = self.bindless_manager.descriptor_set_layout();

        // Load shaders (fullscreen shaders use same module for both stages)
        let mut cache = self.material_compiler.shader_cache.borrow_mut();

        let vert_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| RendererError::InitializationFailed(format!("Vertex shader: {:?}", e)))?;
        let frag_module = cache
            .load_shader(&shader_path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Fragment shader: {:?}", e))
            })?;
        drop(cache);

        // Build pipeline with fullscreen-specific settings
        let builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert_module, frag_module)
            .with_descriptor_layouts(vec![storage_layout, bindless_layout])
            // No vertex binding - fullscreen triangle generated in shader
            .with_depth_test(false, false, crate::pipeline::CompareOp::Always)
            .with_cull_mode(CullMode::None, FrontFace::CounterClockwise)
            // Use specified color format
            .with_rendering_formats(
                Some(color_format),
                None, // No depth attachment for fullscreen passes
            );

        let pipeline = builder.build_dynamic().map_err(|e| {
            RendererError::InitializationFailed(format!("Pipeline creation: {:?}", e))
        })?;

        // The pipeline now holds the descriptor layouts, so we can destroy our temporary copy
        unsafe {
            self.context
                .device
                .destroy_descriptor_set_layout(storage_layout, None);
        }

        Ok(self.asset_registry.register_pipeline(pipeline))
    }
}
