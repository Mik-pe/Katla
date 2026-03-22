use crate::RendererError;
use ash::vk;
use log::{error, info, warn};

/// Bundles all light culling state from VulkanRenderer.
pub(crate) struct LightCullingState {
    /// Light culling buffers for Forward+ dynamic lighting.
    pub buffers: Option<crate::lighting::LightCullingBuffers>,
    /// Light culling compute pipeline (stored directly, not in registry).
    pub pipeline: Option<crate::vulkan::material::compute_pipeline::ComputePipeline>,
    /// Light culling compute shader path (needed to recreate pipeline on resize).
    pub shader_path: Option<std::path::PathBuf>,
}

impl Default for LightCullingState {
    fn default() -> Self {
        Self {
            buffers: None,
            pipeline: None,
            shader_path: None,
        }
    }
}

impl super::VulkanRenderer {
    /// Initialize the Forward+ light culling system.
    ///
    /// Creates GPU buffers for light data and tile culling results, compiles
    /// the light culling compute shader, and sets the light culling descriptor
    /// layout in the material compiler for PBR pipeline compilation.
    ///
    /// Must be called before compiling any PBR materials.
    pub fn init_light_culling(
        &mut self,
        screen_width: u32,
        screen_height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let light_culling_buffers = crate::lighting::LightCullingBuffers::new(
            self.context.clone(),
            screen_width,
            screen_height,
        )
        .map_err(|e| RendererError::InitializationFailed(format!("Light culling init: {}", e)))?;

        if let Some(layout) = light_culling_buffers.fragment_descriptor_layout() {
            self.material_compiler
                .set_light_culling_descriptor_layout(layout);
        }

        self.light_culling.buffers = Some(light_culling_buffers);
        self.light_culling.shader_path = Some(shader_path.to_path_buf());
        self.rebuild_light_culling_pipeline()?;

        info!(
            "Forward+ light culling initialized: {}x{}, {}x{} tiles",
            screen_width,
            screen_height,
            screen_width.div_ceil(16),
            screen_height.div_ceil(16),
        );

        Ok(())
    }

    /// Rebuild the light culling compute pipeline from the current buffers and cached shader.
    ///
    /// Called during init and after resize to create a compute pipeline that references
    /// the current compute descriptor layout (owned by LightCullingBuffers).
    pub(crate) fn rebuild_light_culling_pipeline(&mut self) -> Result<(), RendererError> {
        let lc = self.light_culling.buffers.as_ref().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Cannot rebuild light culling pipeline: buffers not initialized".to_string(),
            )
        })?;

        let shader_path = self.light_culling.shader_path.as_ref().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Cannot rebuild light culling pipeline: shader path not set".to_string(),
            )
        })?;

        let compute_shader = self
            .material_compiler
            .shader_cache
            .borrow_mut()
            .load_shader(shader_path, vk::ShaderStageFlags::COMPUTE)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load light culling compute shader: {}",
                    e
                ))
            })?;

        let compute_layout = lc.compute_descriptor_layout().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Light culling compute descriptor layout not created".to_string(),
            )
        })?;

        let pipeline = crate::vulkan::material::compute_pipeline::ComputePipelineBuilder::new(
            self.context.clone(),
        )
        .with_shader(crate::sync::VkShaderModule(compute_shader))
        .add_descriptor_layout(crate::sync::VkDescriptorSetLayout(compute_layout))
        .build()
        .map_err(|e| {
            RendererError::InitializationFailed(format!(
                "Failed to create light culling compute pipeline: {:?}",
                e
            ))
        })?;

        self.light_culling.pipeline = Some(pipeline);
        Ok(())
    }

    /// Upload point light data for the current frame.
    ///
    /// Call this once per frame before rendering to update the GPU light buffer.
    pub fn upload_lights(&mut self, lights: &[crate::lighting::PointLightGPU]) {
        if let Some(ref mut lc) = self.light_culling.buffers {
            lc.upload_lights(lights);
        }
    }

    /// Clear tile headers and dispatch the light culling compute shader.
    ///
    /// Call after uploading lights and before the geometry pass.
    /// The view and proj matrices are needed for projecting light positions to screen space.
    pub fn dispatch_light_culling(
        &mut self,
        cmd: vk::CommandBuffer,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
    ) {
        let lc = match self.light_culling.buffers.as_mut() {
            Some(lc) => lc,
            None => return,
        };

        let light_count = lc.light_count();
        if light_count == 0 {
            return;
        }

        // Write frame data to uniform buffer for push descriptor
        let frame_data = crate::lighting::LightCullFrameData {
            view_matrix: *view_matrix,
            proj_matrix: *proj_matrix,
            light_count,
            tiles_x: lc.tiles_x(),
            tiles_y: lc.tiles_y(),
            screen_width: lc.screen_width(),
            screen_height: lc.screen_height(),
            _pad0: 0,
            _pad1: 0,
        };
        lc.write_frame_data(&frame_data);

        // Bind compute pipeline
        let compute_pipeline = match self.light_culling.pipeline.as_ref() {
            Some(p) => p,
            None => return,
        };

        unsafe {
            // Clear tile headers on the GPU (avoids CPU-GPU sync issues)
            lc.record_clear_tile_headers(cmd);

            // Memory barrier: fill -> compute shader read/write
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
            self.context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );

            self.context.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                compute_pipeline.pipeline().vk(),
            );

            // Push compute descriptors (Set 0: light/tile/frame buffers)
            if let Err(e) =
                lc.push_compute_descriptors(cmd, compute_pipeline.pipeline_layout().vk())
            {
                warn!("Failed to push light culling compute descriptors: {}", e);
                return;
            }

            // Dispatch: one workgroup per tile
            self.context
                .device
                .cmd_dispatch(cmd, lc.tiles_x(), lc.tiles_y(), 1);
        }

        // Memory barrier to ensure compute writes are visible to fragment shader
        unsafe {
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            self.context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );
        }
    }

    /// Whether the light culling system is active.
    pub fn has_light_culling(&self) -> bool {
        self.light_culling.buffers.is_some()
    }

    /// Get the light culling buffers for push descriptor binding during geometry pass.
    pub fn light_culling_buffers(&self) -> Option<&crate::lighting::LightCullingBuffers> {
        self.light_culling.buffers.as_ref()
    }

    /// Recreate light culling buffers for a new screen size.
    ///
    /// The compute pipeline is reused since it reads tile dimensions from
    /// the uniform buffer at dispatch time. Only the GPU buffers are recreated.
    /// Call this after swapchain recreation to keep tile dimensions in sync.
    pub fn resize_light_culling(&mut self, screen_width: u32, screen_height: u32) {
        if self.light_culling.buffers.is_none() {
            return;
        }

        // Drop the old compute pipeline FIRST — its pipeline layout references
        // the compute descriptor layout owned by LightCullingBuffers, which we're
        // about to destroy. The pipeline must not outlive the layout it references.
        self.light_culling.pipeline = None;

        // Invalidate material pipelines BEFORE dropping old light culling buffers.
        // Old material pipelines reference the old light culling descriptor set layout,
        // so they must be destroyed first to avoid use-after-free in pipeline layout cleanup.
        self.recompile_deferred_materials();

        // Drop old buffers (destroys old descriptor layouts)
        self.light_culling.buffers = None;

        // Create new buffers with updated dimensions
        match crate::lighting::LightCullingBuffers::new(
            self.context.clone(),
            screen_width,
            screen_height,
        ) {
            Ok(new_buffers) => {
                // Update the fragment descriptor layout in the material compiler
                // so newly compiled materials use the correct layout
                if let Some(layout) = new_buffers.fragment_descriptor_layout() {
                    self.material_compiler
                        .set_light_culling_descriptor_layout(layout);
                }

                self.light_culling.buffers = Some(new_buffers);

                // Rebuild compute pipeline with new compute descriptor layout
                if let Err(e) = self.rebuild_light_culling_pipeline() {
                    error!(
                        "Failed to rebuild light culling compute pipeline after resize: {}",
                        e
                    );
                }

                info!(
                    "Light culling buffers resized to {}x{} ({}x{} tiles), invalidated compiled materials",
                    screen_width,
                    screen_height,
                    screen_width.div_ceil(16),
                    screen_height.div_ceil(16),
                );
            }
            Err(e) => {
                error!("Failed to recreate light culling buffers: {}", e);
            }
        }
    }
}
