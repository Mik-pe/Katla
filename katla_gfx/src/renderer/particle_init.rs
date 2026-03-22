use crate::RendererError;
use ash::vk;
use log::warn;

impl super::VulkanRenderer {
    /// Initialize particle emit pipeline (must be called after renderer is fully initialized).
    pub fn init_particle_emit_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle emit shader
            match self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
            {
                Ok(shader_module) => {
                    let shader_module_wrapper = crate::sync::VkShaderModule(shader_module);
                    ps.create_emit_pipeline(&mut self.asset_registry, shader_module_wrapper)
                        .map_err(|e| {
                            RendererError::InitializationFailed(format!(
                                "Failed to create particle emit pipeline: {}",
                                e
                            ))
                        })?;
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to load particle emit shader: {}", e);
                    Err(RendererError::InitializationFailed(format!(
                        "Failed to load particle emit shader: {}",
                        e
                    )))
                }
            }
        } else {
            warn!("Particle system not initialized, skipping emit pipeline creation");
            Ok(())
        }
    }

    /// Initialize particle simulate pipeline (must be called after renderer is fully initialized).
    pub fn init_particle_simulate_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle simulate shader
            match self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
            {
                Ok(shader_module) => {
                    let shader_module_wrapper = crate::sync::VkShaderModule(shader_module);
                    ps.create_simulate_pipeline(&mut self.asset_registry, shader_module_wrapper)
                        .map_err(|e| {
                            RendererError::InitializationFailed(format!(
                                "Failed to create particle simulate pipeline: {}",
                                e
                            ))
                        })?;
                    Ok(())
                }
                Err(e) => {
                    warn!("Failed to load particle simulate shader: {}", e);
                    Err(RendererError::InitializationFailed(format!(
                        "Failed to load particle simulate shader: {}",
                        e
                    )))
                }
            }
        } else {
            warn!("Particle system not initialized, skipping simulate pipeline creation");
            Ok(())
        }
    }

    /// Initialize particle render pipeline.
    ///
    /// Loads particle vertex and fragment shaders and creates the render pipeline.
    pub fn init_particle_render_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        if let Some(ref mut ps) = self.particle_system {
            // Load particle vertex shader
            let vert_shader = self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load particle vertex shader: {}",
                        e
                    ))
                })?;

            // Load particle fragment shader
            let frag_shader = self
                .material_compiler
                .shader_cache
                .borrow_mut()
                .load_shader(shader_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load particle fragment shader: {}",
                        e
                    ))
                })?;

            let vert_shader_wrapper = crate::sync::VkShaderModule(vert_shader);
            let frag_shader_wrapper = crate::sync::VkShaderModule(frag_shader);

            ps.create_render_pipeline(
                &mut self.asset_registry,
                vert_shader_wrapper,
                frag_shader_wrapper,
            )
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to create particle render pipeline: {}",
                    e
                ))
            })?;

            Ok(())
        } else {
            warn!("Particle system not initialized, skipping render pipeline creation");
            Ok(())
        }
    }
}
