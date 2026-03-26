use crate::RendererError;
use crate::handle::PipelineHandle;
use crate::pipeline::{CompareOp, CullMode, FrontFace};
use crate::texture::ImageFormat;
use crate::vulkan::material::builder::PipelineBuilder;
use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;
use log::info;

#[derive(Default)]
pub(crate) struct OutlineState {
    /// Pipeline for stencil mark pass (writes ref=1 to stencil for visible selected objects).
    pub stencil_mark_pipeline: Option<PipelineHandle>,
    /// Skinned stencil mark pipeline.
    pub stencil_mark_skinned_pipeline: Option<PipelineHandle>,
    /// Pipeline for occlusion mark pass (promotes stencil 1→2 where selected objects are occluded).
    pub occlusion_mark_pipeline: Option<PipelineHandle>,
    /// Skinned occlusion mark pipeline.
    pub occlusion_mark_skinned_pipeline: Option<PipelineHandle>,
    /// Pipeline for outline draw pass (inverted culling, stencil != 1).
    pub outline_draw_pipeline: Option<PipelineHandle>,
    /// Skinned outline draw pipeline.
    pub outline_draw_skinned_pipeline: Option<PipelineHandle>,
    /// Pipeline for stencil indicator pass (writes R8 where stencil == 2).
    pub stencil_indicator_pipeline: Option<PipelineHandle>,
    /// Skinned stencil indicator pipeline.
    pub stencil_indicator_skinned_pipeline: Option<PipelineHandle>,
    /// Empty descriptor set layout (Set 1 placeholder for skinned pipelines).
    pub skinned_empty_layout: Option<vk::DescriptorSetLayout>,
}

impl super::VulkanRenderer {
    #[allow(clippy::too_many_arguments)]
    fn build_outline_pipeline(
        &mut self,
        vert: vk::ShaderModule,
        frag: vk::ShaderModule,
        storage_layout: vk::DescriptorSetLayout,
        stencil_state: vk::StencilOpState,
        depth_compare: CompareOp,
        cull_mode: CullMode,
        color_format: ImageFormat,
        color_write_mask: vk::ColorComponentFlags,
        empty_layout: Option<vk::DescriptorSetLayout>,
    ) -> Result<PipelineHandle, RendererError> {
        let mut builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert, frag)
            .with_soa_attribute(0, VertexFormat::RGB32f)
            .with_depth_test(true, false, depth_compare)
            .with_cull_mode(cull_mode, FrontFace::CounterClockwise)
            .with_stencil_test(stencil_state, stencil_state)
            .with_color_write_mask(color_write_mask)
            .with_rendering_formats(Some(color_format), Some(ImageFormat::D32SfloatS8Uint));

        if let Some(empty_layout) = empty_layout {
            let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();
            builder = builder
                .with_descriptor_layouts(vec![storage_layout, empty_layout, skeleton_layout])
                .with_soa_attribute(4, VertexFormat::RGBA16u)
                .with_soa_attribute(5, VertexFormat::RGBA32f);
        } else {
            builder = builder.with_descriptor_layouts(vec![storage_layout]);
        }

        let pipeline = builder.build_dynamic().map_err(|e| {
            RendererError::InitializationFailed(format!(
                "Failed to build outline pipeline: {:?}",
                e
            ))
        })?;

        Ok(self.asset_registry.register_pipeline(pipeline))
    }

    fn load_outline_shaders(
        &self,
        path: &std::path::Path,
        name: &str,
    ) -> Result<(vk::ShaderModule, vk::ShaderModule), RendererError> {
        let mut cache = self.material_compiler.shader_cache.borrow_mut();
        let vert = cache
            .load_shader(path, vk::ShaderStageFlags::VERTEX)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load {} vertex shader: {:?}",
                    name, e
                ))
            })?;
        let frag = cache
            .load_shader(path, vk::ShaderStageFlags::FRAGMENT)
            .map_err(|e| {
                RendererError::InitializationFailed(format!(
                    "Failed to load {} fragment shader: {:?}",
                    name, e
                ))
            })?;
        Ok((vert, frag))
    }

    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let storage_layout = self.storage_descriptor_sets[0].layout();

        // === Stencil Mark Pipeline ===
        // Renders selected objects with color write OFF, depth test GREATER_OR_EQUAL.
        // Stencil: REPLACE ref=1 on depth pass (visible), KEEP on depth fail.
        // write_mask=0x01: only writes bit 0 of the stencil value. This allows the
        // occlusion mark pass to use bit 1 independently, preventing self-occlusion
        // artifacts on non-convex meshes.
        {
            let (vert, frag) = self.load_outline_shaders(stencil_mark_path, "stencil mark")?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::REPLACE,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0x01,
                reference: 1,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Back,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::empty(),
                None,
            )?;
            self.outline.stencil_mark_pipeline = Some(handle);
        }

        // === Skinned Stencil Mark Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(stencil_mark_skinned_path, "skinned stencil mark")?;

            let empty_descriptor_layout = unsafe {
                self.context
                    .device
                    .create_descriptor_set_layout(
                        &vk::DescriptorSetLayoutCreateInfo::default(),
                        None,
                    )
                    .map_err(|e| {
                        RendererError::InitializationFailed(format!(
                            "Failed to create empty descriptor layout for skinned outline: {:?}",
                            e
                        ))
                    })?
            };
            self.outline.skinned_empty_layout = Some(empty_descriptor_layout);

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::REPLACE,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0x01,
                reference: 1,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Back,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::empty(),
                Some(empty_descriptor_layout),
            )?;
            self.outline.stencil_mark_skinned_pipeline = Some(handle);
        }

        // === Occlusion Mark Pipeline ===
        // Second stencil pass: writes stencil bit 1 (value 2) where selected objects
        // are occluded by other scene geometry (depth test fail).
        // compare EQUAL 0 with compare_mask=0x01: only processes pixels where bit 0
        // is clear (no visible front face from stencil mark). This prevents
        // self-occlusion on non-convex meshes — back faces behind front faces of the
        // same object have stencil bit 0 set and are skipped.
        // write_mask=0x02: only writes bit 1, preserving bit 0 from stencil mark.
        // depth_fail_op: REPLACE ref=2 writes 0b10 (bit 1 set) on depth fail.
        {
            let (vert, frag) = self.load_outline_shaders(stencil_mark_path, "occlusion mark")?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::REPLACE,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0x01,
                write_mask: 0x02,
                reference: 2,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Back,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::empty(),
                None,
            )?;
            self.outline.occlusion_mark_pipeline = Some(handle);
        }

        // === Skinned Occlusion Mark Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(stencil_mark_skinned_path, "skinned occlusion mark")?;
            let empty_descriptor_layout =
                self.outline
                    .skinned_empty_layout
                    .ok_or(RendererError::InitializationFailed(
                        "Skinned empty layout not initialized — call init_outline_pipelines first"
                            .into(),
                    ))?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::REPLACE,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0x01,
                write_mask: 0x02,
                reference: 2,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Back,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::empty(),
                Some(empty_descriptor_layout),
            )?;
            self.outline.occlusion_mark_skinned_pipeline = Some(handle);
        }

        // === Outline Draw Pipeline ===
        // Inverted culling (front faces only) with depth test GREATER_OR_EQUAL,
        // depth write OFF. Stencil test EQUAL 0: only draws on background.
        // Occluded parts are handled by the stencil indicator + tonemap overlay.
        {
            let (vert, frag) = self.load_outline_shaders(outline_draw_path, "outline draw")?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 0,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Front,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::default(),
                None,
            )?;
            self.outline.outline_draw_pipeline = Some(handle);
        }

        // === Skinned Outline Draw Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(outline_draw_skinned_path, "skinned outline draw")?;
            let empty_descriptor_layout =
                self.outline
                    .skinned_empty_layout
                    .ok_or(RendererError::InitializationFailed(
                        "Skinned empty layout not initialized — call init_outline_pipelines first"
                            .into(),
                    ))?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 0,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::GreaterOrEqual,
                CullMode::Front,
                ImageFormat::R16G16B16A16Sfloat,
                vk::ColorComponentFlags::default(),
                Some(empty_descriptor_layout),
            )?;
            self.outline.outline_draw_skinned_pipeline = Some(handle);
        }

        info!("Outline pipelines initialized (stencil-based selection highlight)");

        Ok(())
    }

    pub fn init_stencil_indicator_pipelines(
        &mut self,
        shader_path: &std::path::Path,
        skinned_shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let storage_layout = self.storage_descriptor_sets[0].layout();

        {
            let (vert, frag) = self.load_outline_shaders(shader_path, "stencil indicator")?;
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::Always,
                CullMode::Back,
                ImageFormat::R8Unorm,
                vk::ColorComponentFlags::default(),
                None,
            )?;
            self.outline.stencil_indicator_pipeline = Some(handle);
        }

        {
            let (vert, frag) =
                self.load_outline_shaders(skinned_shader_path, "skinned stencil indicator")?;
            let empty_descriptor_layout =
                self.outline
                    .skinned_empty_layout
                    .ok_or(RendererError::InitializationFailed(
                        "Skinned empty layout not initialized — call init_outline_pipelines first"
                            .into(),
                    ))?;

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };
            let handle = self.build_outline_pipeline(
                vert,
                frag,
                storage_layout,
                stencil_state,
                CompareOp::Always,
                CullMode::Back,
                ImageFormat::R8Unorm,
                vk::ColorComponentFlags::default(),
                Some(empty_descriptor_layout),
            )?;
            self.outline.stencil_indicator_skinned_pipeline = Some(handle);
        }

        info!("Stencil indicator pipelines initialized");

        Ok(())
    }
}
