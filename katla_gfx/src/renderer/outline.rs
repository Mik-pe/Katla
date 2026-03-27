use crate::RendererError;
use crate::handle::PipelineHandle;
use crate::pipeline::{CompareOp, CullMode, FrontFace};
use crate::texture::ImageFormat;
use crate::vulkan::material::builder::PipelineBuilder;
use crate::vulkan::vertexbinding::VertexFormat;
use ash::vk;
use log::info;

/// Push constants for outline draw pipelines.
///
/// Layout must match `OutlinePushConstants` in outline_draw.wgsl:
/// - offset 0: outline_width (f32) + 3 x padding (f32) = 16 bytes
/// - offset 16: outline_color (vec4) = 16 bytes
///
/// Total: 32 bytes
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct OutlinePushConstants {
    pub outline_width: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    pub outline_color: [f32; 4],
}

impl Default for OutlinePushConstants {
    fn default() -> Self {
        Self {
            outline_width: 0.004,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            outline_color: [1.0, 0.55, 0.0, 1.0],
        }
    }
}

/// Compute a viewport-aware outline width in NDC.
/// The base width (0.004) targets ~1080p; scale inversely with viewport height
/// so outlines remain a consistent pixel width across resolutions.
pub(crate) fn compute_outline_width(viewport_height: f32) -> f32 {
    const BASE_HEIGHT: f32 = 1080.0;
    const BASE_WIDTH: f32 = 0.004;
    BASE_WIDTH * (BASE_HEIGHT / viewport_height)
}

pub(crate) struct OutlineState {
    /// Pipeline for stencil mark pass (writes ref=1 to stencil for visible selected objects).
    pub stencil_mark_pipeline: PipelineHandle,
    /// Skinned stencil mark pipeline.
    pub stencil_mark_skinned_pipeline: PipelineHandle,
    /// Pipeline for occlusion mark pass (promotes stencil 1→2 where selected objects are occluded).
    pub occlusion_mark_pipeline: PipelineHandle,
    /// Skinned occlusion mark pipeline.
    pub occlusion_mark_skinned_pipeline: PipelineHandle,
    /// Pipeline for outline draw pass (inverted culling, stencil != 1).
    pub outline_draw_pipeline: PipelineHandle,
    /// Skinned outline draw pipeline.
    pub outline_draw_skinned_pipeline: PipelineHandle,
    /// Pipeline for stencil indicator pass (writes R8 where stencil == 2).
    pub stencil_indicator_pipeline: PipelineHandle,
    /// Skinned stencil indicator pipeline.
    pub stencil_indicator_skinned_pipeline: PipelineHandle,
}

impl Default for OutlineState {
    fn default() -> Self {
        Self {
            stencil_mark_pipeline: PipelineHandle::NONE,
            stencil_mark_skinned_pipeline: PipelineHandle::NONE,
            occlusion_mark_pipeline: PipelineHandle::NONE,
            occlusion_mark_skinned_pipeline: PipelineHandle::NONE,
            outline_draw_pipeline: PipelineHandle::NONE,
            outline_draw_skinned_pipeline: PipelineHandle::NONE,
            stencil_indicator_pipeline: PipelineHandle::NONE,
            stencil_indicator_skinned_pipeline: PipelineHandle::NONE,
        }
    }
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
        push_constant_size: Option<u32>,
    ) -> Result<PipelineHandle, RendererError> {
        let mut builder = PipelineBuilder::new(self.context.clone())
            .with_shaders(vert, frag)
            .with_soa_attribute(0, VertexFormat::RGB32f)
            .with_depth_test(true, false, depth_compare)
            .with_cull_mode(cull_mode, FrontFace::CounterClockwise)
            .with_stencil_test(stencil_state, stencil_state)
            .with_color_write_mask(color_write_mask)
            .with_rendering_formats(Some(color_format), Some(ImageFormat::D32SfloatS8Uint));

        if let Some(size) = push_constant_size {
            let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
            builder = builder.with_push_constant_range(stages, 0, size);
        }

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
                None,
            )?;
            self.outline.stencil_mark_pipeline = handle;
        }

        // === Skinned Stencil Mark Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(stencil_mark_skinned_path, "skinned stencil mark")?;

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
                Some(self.shared_empty_descriptor_layout),
                None,
            )?;
            self.outline.stencil_mark_skinned_pipeline = handle;
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
                None,
            )?;
            self.outline.occlusion_mark_pipeline = handle;
        }

        // === Skinned Occlusion Mark Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(stencil_mark_skinned_path, "skinned occlusion mark")?;

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
                Some(self.shared_empty_descriptor_layout),
                None,
            )?;
            self.outline.occlusion_mark_skinned_pipeline = handle;
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
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
                None,
                Some(std::mem::size_of::<OutlinePushConstants>() as u32),
            )?;
            self.outline.outline_draw_pipeline = handle;
        }

        // === Skinned Outline Draw Pipeline ===
        {
            let (vert, frag) =
                self.load_outline_shaders(outline_draw_skinned_path, "skinned outline draw")?;

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
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
                Some(self.shared_empty_descriptor_layout),
                Some(std::mem::size_of::<OutlinePushConstants>() as u32),
            )?;
            self.outline.outline_draw_skinned_pipeline = handle;
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
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
                None,
                None,
            )?;
            self.outline.stencil_indicator_pipeline = handle;
        }

        {
            let (vert, frag) =
                self.load_outline_shaders(skinned_shader_path, "skinned stencil indicator")?;

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
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
                Some(self.shared_empty_descriptor_layout),
                None,
            )?;
            self.outline.stencil_indicator_skinned_pipeline = handle;
        }

        info!("Stencil indicator pipelines initialized");

        Ok(())
    }
}
