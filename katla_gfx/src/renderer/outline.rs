use crate::RendererError;
use crate::handle::PipelineHandle;
use ash::vk;
use log::info;

#[derive(Default)]
pub(crate) struct OutlineState {
    /// Pipeline for stencil mark pass (writes ref=1 to stencil for selected objects).
    pub stencil_mark_pipeline: Option<PipelineHandle>,
    /// Skinned stencil mark pipeline.
    pub stencil_mark_skinned_pipeline: Option<PipelineHandle>,
    /// Pipeline for outline draw pass (inverted culling, stencil != 1).
    pub outline_draw_pipeline: Option<PipelineHandle>,
    /// Skinned outline draw pipeline.
    pub outline_draw_skinned_pipeline: Option<PipelineHandle>,
    /// Pipeline for wallhack overlay pass (stencil == 1, alpha blended, depth always).
    pub overlay_pipeline: Option<PipelineHandle>,
    /// Skinned overlay pipeline.
    pub overlay_skinned_pipeline: Option<PipelineHandle>,
    /// Empty descriptor set layout (Set 1 placeholder for skinned pipelines).
    pub skinned_empty_layout: Option<vk::DescriptorSetLayout>,
}

impl super::VulkanRenderer {
    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
        overlay_path: &std::path::Path,
        overlay_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        use crate::pipeline::{CullMode, FrontFace};
        use crate::vulkan::material::builder::PipelineBuilder;
        use crate::vulkan::vertexbinding::VertexFormat;

        let storage_layout = self.storage_descriptor_sets[0].layout();

        // === Stencil Mark Pipeline ===
        // Renders selected objects with color write OFF, depth test EQUALS,
        // stencil ALWAYS write ref=1.
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(stencil_mark_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load stencil mark vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(stencil_mark_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load stencil mark fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            // Stencil: ref=1 on depth pass (visible), ref=2 on depth fail (occluded).
            // depth_fail_op INCR increments from ref=1 to 2 when behind geometry.
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::REPLACE,
                depth_fail_op: vk::StencilOp::INCREMENT_AND_CLAMP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0xFF,
                reference: 1,
            };

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![storage_layout])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                .with_depth_test(true, false, crate::pipeline::CompareOp::GreaterOrEqual)
                .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_color_write_mask(vk::ColorComponentFlags::empty())
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build stencil mark pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.stencil_mark_pipeline = Some(handle);
        }

        // === Skinned Stencil Mark Pipeline ===
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(stencil_mark_skinned_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned stencil mark vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(stencil_mark_skinned_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned stencil mark fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();

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
                depth_fail_op: vk::StencilOp::INCREMENT_AND_CLAMP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0xFF,
                write_mask: 0xFF,
                reference: 1,
            };

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![
                    storage_layout,
                    empty_descriptor_layout,
                    skeleton_layout,
                ])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                .with_soa_attribute(4, VertexFormat::RGBA16u)
                .with_soa_attribute(5, VertexFormat::RGBA32f)
                .with_depth_test(true, false, crate::pipeline::CompareOp::GreaterOrEqual)
                .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_color_write_mask(vk::ColorComponentFlags::empty())
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build skinned stencil mark pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.stencil_mark_skinned_pipeline = Some(handle);
        }

        // === Outline Draw Pipeline ===
        // Inverted culling (front faces only) with depth test ALWAYS, depth write OFF.
        // Stencil test NOT EQUAL 1: only draws where the selected object was NOT rendered.
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(outline_draw_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load outline draw vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(outline_draw_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load outline draw fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            // Stencil: only draw where stencil != 1 (i.e., NOT where selected object was drawn)
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::NOT_EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 1,
            };

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![storage_layout])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                // Depth test ALWAYS with depth write OFF — we don't want the outline
                // to occlude or be occluded by scene geometry.
                .with_depth_test(true, false, crate::pipeline::CompareOp::Always)
                // Inverted culling: render front faces (normally culled).
                // Combined with the extruded vertices this creates the outline shell.
                .with_cull_mode(CullMode::Front, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build outline draw pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.outline_draw_pipeline = Some(handle);
        }

        // === Skinned Outline Draw Pipeline ===
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(outline_draw_skinned_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned outline draw vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(outline_draw_skinned_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned outline draw fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::NOT_EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 1,
            };

            let empty_descriptor_layout = self.outline.skinned_empty_layout.unwrap();

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![
                    storage_layout,
                    empty_descriptor_layout,
                    skeleton_layout,
                ])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                .with_soa_attribute(4, VertexFormat::RGBA16u)
                .with_soa_attribute(5, VertexFormat::RGBA32f)
                .with_depth_test(true, false, crate::pipeline::CompareOp::Always)
                .with_cull_mode(CullMode::Front, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build skinned outline draw pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.outline_draw_skinned_pipeline = Some(handle);
        }

        // === Overlay Pipeline (wallhack) ===
        // Renders selected object with alpha blending where stencil == 2 (occluded areas).
        // Depth test ALWAYS so the overlay shows through other geometry.
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(overlay_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load overlay vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(overlay_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load overlay fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            // Stencil: only draw where stencil == 2 (occluded parts of selected object)
            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![storage_layout])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                .with_depth_test(true, false, crate::pipeline::CompareOp::Always)
                .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_alpha_blending()
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build overlay pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.overlay_pipeline = Some(handle);
        }

        // === Skinned Overlay Pipeline ===
        {
            let mut cache = self.material_compiler.shader_cache.borrow_mut();
            let vert = cache
                .load_shader(overlay_skinned_path, vk::ShaderStageFlags::VERTEX)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned overlay vertex shader: {:?}",
                        e
                    ))
                })?;
            let frag = cache
                .load_shader(overlay_skinned_path, vk::ShaderStageFlags::FRAGMENT)
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to load skinned overlay fragment shader: {:?}",
                        e
                    ))
                })?;
            drop(cache);

            let skeleton_layout = self.material_compiler.skeleton_descriptor_layout();

            let stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::EQUAL,
                compare_mask: 0xFF,
                write_mask: 0x00,
                reference: 2,
            };

            let empty_descriptor_layout = self.outline.skinned_empty_layout.unwrap();

            let pipeline = PipelineBuilder::new(self.context.clone())
                .with_shaders(vert, frag)
                .with_descriptor_layouts(vec![
                    storage_layout,
                    empty_descriptor_layout,
                    skeleton_layout,
                ])
                .with_soa_attribute(0, VertexFormat::RGB32f)
                .with_soa_attribute(4, VertexFormat::RGBA16u)
                .with_soa_attribute(5, VertexFormat::RGBA32f)
                .with_depth_test(true, false, crate::pipeline::CompareOp::Always)
                .with_cull_mode(CullMode::Back, FrontFace::CounterClockwise)
                .with_stencil_test(stencil_state, stencil_state)
                .with_alpha_blending()
                .with_rendering_formats(
                    Some(crate::texture::ImageFormat::R16G16B16A16Sfloat),
                    Some(crate::texture::ImageFormat::D32SfloatS8Uint),
                )
                .build_dynamic()
                .map_err(|e| {
                    RendererError::InitializationFailed(format!(
                        "Failed to build skinned overlay pipeline: {:?}",
                        e
                    ))
                })?;

            let handle = self.asset_registry.register_pipeline(pipeline);
            self.outline.overlay_skinned_pipeline = Some(handle);
        }

        info!("Outline pipelines initialized (stencil-based selection highlight)");

        Ok(())
    }
}
