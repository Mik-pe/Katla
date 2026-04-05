use crate::RendererError;
use ash::vk;
use log::{error, info, warn};
use std::rc::Rc;

use crate::vulkan::context::VulkanContext;
use crate::vulkan::material::compiler::MaterialCompiler;

#[derive(Default)]
/// Owns all light culling GPU state for Forward+ dynamic lighting.
///
/// Lifecycle:
/// - `init()` — creates buffers, compiles compute shader, sets descriptor layout
/// - `resize()` — recreates buffers for new screen dimensions
/// - `destroy()` — drops pipeline and buffers (cleanup handled by Drop)
pub(crate) struct LightSubsystem {
    /// Light culling buffers for Forward+ dynamic lighting.
    buffers: Option<crate::lighting::LightCullingBuffers>,
    /// Light culling compute pipeline (stored directly, not in registry).
    pipeline: Option<crate::vulkan::material::compute_pipeline::ComputePipeline>,
    /// Light culling compute shader path (needed to recreate pipeline on resize).
    shader_path: Option<std::path::PathBuf>,
}

/// Dependencies needed from VulkanRenderer for light subsystem initialization.
pub(crate) struct LightInitContext<'a> {
    pub context: &'a Rc<VulkanContext>,
    pub material_compiler: &'a mut MaterialCompiler,
}

impl LightSubsystem {
    /// Initialize the Forward+ light culling system.
    ///
    /// Creates GPU buffers for light data and tile culling results, compiles
    /// the light culling compute shader, and sets the light culling descriptor
    /// layout in the material compiler for PBR pipeline compilation.
    ///
    /// Must be called before compiling any PBR materials.
    pub fn init(
        &mut self,
        ctx: &mut LightInitContext,
        screen_width: u32,
        screen_height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let light_culling_buffers = crate::lighting::LightCullingBuffers::new(
            ctx.context.clone(),
            screen_width,
            screen_height,
        )
        .map_err(|e| RendererError::InitializationFailed(format!("Light culling init: {}", e)))?;

        if let Some(layout) = light_culling_buffers.fragment_descriptor_layout() {
            ctx.material_compiler
                .set_light_culling_descriptor_layout(layout);
        }

        self.buffers = Some(light_culling_buffers);
        self.shader_path = Some(shader_path.to_path_buf());
        self.rebuild_pipeline(ctx)?;

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
    pub(crate) fn rebuild_pipeline(
        &mut self,
        ctx: &mut LightInitContext,
    ) -> Result<(), RendererError> {
        let lc = self.buffers.as_ref().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Cannot rebuild light culling pipeline: buffers not initialized".to_string(),
            )
        })?;

        let shader_path = self.shader_path.as_ref().ok_or_else(|| {
            RendererError::InitializationFailed(
                "Cannot rebuild light culling pipeline: shader path not set".to_string(),
            )
        })?;

        let compute_shader = ctx
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
            ctx.context.clone(),
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

        self.pipeline = Some(pipeline);
        Ok(())
    }

    /// Upload point light data for the current frame.
    ///
    /// Call this once per frame before rendering to update the GPU light buffer.
    pub fn upload_lights(&mut self, lights: &[crate::lighting::PointLightGPU]) {
        if let Some(ref mut lc) = self.buffers {
            lc.upload_lights(lights);
        }
    }

    /// Clear tile headers and dispatch the light culling compute shader.
    ///
    /// Call after uploading lights and before the geometry pass.
    /// The view and proj matrices are needed for projecting light positions to screen space.
    pub fn dispatch_light_culling(
        &mut self,
        context: &Rc<VulkanContext>,
        cmd: vk::CommandBuffer,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
    ) {
        let lc = match self.buffers.as_mut() {
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
            _pad2: 0,
        };
        lc.write_frame_data(&frame_data);

        // Bind compute pipeline
        let compute_pipeline = match self.pipeline.as_ref() {
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
            context.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[barrier],
                &[],
                &[],
            );

            context.device.cmd_bind_pipeline(
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
            context
                .device
                .cmd_dispatch(cmd, lc.tiles_x(), lc.tiles_y(), 1);
        }

        // Memory barrier to ensure compute writes are visible to fragment shader
        unsafe {
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);

            context.device.cmd_pipeline_barrier(
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
        self.buffers.is_some()
    }

    /// Get the light culling buffers for push descriptor binding during geometry pass.
    pub fn buffers(&self) -> Option<&crate::lighting::LightCullingBuffers> {
        self.buffers.as_ref()
    }

    /// Recreate light culling buffers for a new screen size.
    ///
    /// Drops the old compute pipeline and buffers, creates new ones with updated
    /// dimensions, and rebuilds the compute pipeline.
    ///
    /// Call this after swapchain recreation to keep tile dimensions in sync.
    pub fn resize(&mut self, ctx: &mut LightInitContext, screen_width: u32, screen_height: u32) {
        if self.buffers.is_none() {
            return;
        }

        // Drop the old compute pipeline FIRST — its pipeline layout references
        // the compute descriptor layout owned by LightCullingBuffers, which we're
        // about to destroy. The pipeline must not outlive the layout it references.
        self.pipeline = None;

        // Drop old buffers (destroys old descriptor layouts)
        self.buffers = None;

        // Create new buffers with updated dimensions
        match crate::lighting::LightCullingBuffers::new(
            ctx.context.clone(),
            screen_width,
            screen_height,
        ) {
            Ok(new_buffers) => {
                // Update the fragment descriptor layout in the material compiler
                // so newly compiled materials use the correct layout
                if let Some(layout) = new_buffers.fragment_descriptor_layout() {
                    ctx.material_compiler
                        .set_light_culling_descriptor_layout(layout);
                }

                self.buffers = Some(new_buffers);

                // Rebuild compute pipeline with new compute descriptor layout
                if let Err(e) = self.rebuild_pipeline(ctx) {
                    error!(
                        "Failed to rebuild light culling compute pipeline after resize: {}",
                        e
                    );
                }

                info!(
                    "Light culling buffers resized to {}x{} ({}x{} tiles)",
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

    /// Destroy light culling resources.
    ///
    /// Drop order: pipeline first (doesn't own descriptor layouts),
    /// then buffers (owns descriptor layouts and GPU buffers).
    pub fn destroy(&mut self) {
        self.pipeline = None;
        self.buffers = None;
    }
}

impl super::VulkanRenderer {
    /// Initialize the Forward+ light culling system.
    ///
    /// Delegates to [`LightSubsystem::init`].
    pub fn init_light_culling(
        &mut self,
        screen_width: u32,
        screen_height: u32,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let mut ctx = super::light_culling::LightInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
        };
        self.light_culling
            .init(&mut ctx, screen_width, screen_height, shader_path)
    }

    /// Upload point light data for the current frame.
    ///
    /// Delegates to [`LightSubsystem::upload_lights`].
    pub fn upload_lights(&mut self, lights: &[crate::lighting::PointLightGPU]) {
        self.light_culling.upload_lights(lights);
    }

    /// Clear tile headers and dispatch the light culling compute shader.
    ///
    /// Delegates to [`LightSubsystem::dispatch_light_culling`].
    pub fn dispatch_light_culling(
        &mut self,
        cmd: vk::CommandBuffer,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
    ) {
        self.light_culling
            .dispatch_light_culling(&self.context, cmd, view_matrix, proj_matrix);
    }

    /// Whether the light culling system is active.
    ///
    /// Delegates to [`LightSubsystem::has_light_culling`].
    pub fn has_light_culling(&self) -> bool {
        self.light_culling.has_light_culling()
    }

    /// Get the light culling buffers for push descriptor binding during geometry pass.
    ///
    /// Delegates to [`LightSubsystem::buffers`].
    pub fn light_culling_buffers(&self) -> Option<&crate::lighting::LightCullingBuffers> {
        self.light_culling.buffers()
    }

    /// Recreate light culling buffers for a new screen size.
    ///
    /// Delegates to [`LightSubsystem::resize`]. Also invalidates compiled materials
    /// since they reference the old light culling descriptor set layout.
    pub fn resize_light_culling(&mut self, screen_width: u32, screen_height: u32) {
        // Invalidate material pipelines BEFORE resizing light culling.
        // Old material pipelines reference the old light culling descriptor set layout,
        // so they must be destroyed first to avoid use-after-free in pipeline layout cleanup.
        if self.light_culling.has_light_culling() {
            self.recompile_deferred_materials();
        }

        let mut ctx = super::light_culling::LightInitContext {
            context: &self.context,
            material_compiler: &mut self.material_compiler,
        };
        self.light_culling
            .resize(&mut ctx, screen_width, screen_height);
    }
}
