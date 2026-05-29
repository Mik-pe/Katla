use objc2_metal::MTLPixelFormat;

use crate::error::RendererError;

use super::metal_renderer::{MetalRenderer, read_shader};
use super::shader::{self, ShaderProfile};

impl MetalRenderer {
    pub fn init_shadow_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Shadow vertex entry point not found".into())
        })?;
        self.shadow.create_pipeline(&self.context, vertex_fn)
    }

    pub fn init_shadow_pipeline_skinned(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Skinned shadow vertex entry point not found".into())
        })?;
        self.shadow
            .create_pipeline_skinned(&self.context, vertex_fn)
    }

    pub fn init_depth_prepass_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Depth prepass vertex entry point not found".into())
        })?;
        self.depth_prepass.create_pipeline(&self.context, vertex_fn)
    }

    pub fn init_depth_prepass_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "Skinned depth prepass vertex entry point not found".into(),
            )
        })?;
        self.depth_prepass
            .create_pipeline_skinned(&self.context, vertex_fn)
    }

    pub fn init_depth_prepass_billboard_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation(
                "Billboard depth prepass vertex entry point not found".into(),
            )
        })?;
        self.depth_prepass.create_pipeline(&self.context, vertex_fn)
    }

    pub fn init_outline_pipelines(
        &mut self,
        stencil_mark_path: &std::path::Path,
        stencil_mark_skinned_path: &std::path::Path,
        outline_draw_path: &std::path::Path,
        outline_draw_skinned_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        {
            let wgsl = read_shader(&stencil_mark_path.to_string_lossy())?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main"],
                ShaderProfile::Graphics,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Stencil mark vertex entry point not found".into())
            })?;
            self.outline
                .create_stencil_mark_pipeline(&self.context, vs)?;
        }
        {
            let wgsl = read_shader(&stencil_mark_skinned_path.to_string_lossy())?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main"],
                ShaderProfile::Graphics,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned stencil mark vertex entry point not found".into(),
                )
            })?;
            self.outline
                .create_stencil_mark_skinned_pipeline(&self.context, vs)?;
        }
        {
            let wgsl = read_shader(&outline_draw_path.to_string_lossy())?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main", "fs_main"],
                ShaderProfile::Outline,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation("Outline draw vertex entry point not found".into())
            })?;
            let fs = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Outline draw fragment entry point not found".into(),
                )
            })?;
            self.outline
                .create_outline_draw_pipeline(&self.context, vs, fs)?;
        }
        {
            let wgsl = read_shader(&outline_draw_skinned_path.to_string_lossy())?;
            let compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl,
                &["vs_main", "fs_main"],
                ShaderProfile::OutlineSkinned,
            )?;
            let vs = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned outline draw vertex entry point not found".into(),
                )
            })?;
            let fs = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
                RendererError::InvalidOperation(
                    "Skinned outline draw fragment entry point not found".into(),
                )
            })?;
            self.outline
                .create_outline_draw_skinned_pipeline(&self.context, vs, fs)?;
        }
        Ok(())
    }

    pub fn init_picking_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Picking vertex entry point not found".into())
        })?;
        let fragment_fn = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Picking fragment entry point not found".into())
        })?;
        self.picking
            .create_pipeline(&self.context, vertex_fn, fragment_fn)
    }

    pub fn init_picking_skinned_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Skinned picking vertex entry point not found".into())
        })?;
        let fragment_fn = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Skinned picking fragment entry point not found".into())
        })?;
        self.picking
            .create_pipeline_skinned(&self.context, vertex_fn, fragment_fn)
    }

    pub fn init_stencil_indicator_pipelines(
        &mut self,
        _shader_path: &std::path::Path,
        _skinned_shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        Ok(())
    }

    pub fn init_sky_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Sky vertex entry point not found".into())
        })?;
        let fragment_fn = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Sky fragment entry point not found".into())
        })?;
        let pipeline = self
            .context
            .create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                Some(fragment_fn),
                &[MTLPixelFormat::RGBA16Float],
                Some(MTLPixelFormat::Depth32Float_Stencil8),
                false,
                crate::pipeline::CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                Some(&super::context::fullscreen_vertex_descriptor()),
                false,
            )?;
        self.sky_pipeline = Some(pipeline);
        Ok(())
    }

    pub fn init_tonemap_pipeline(
        &mut self,
        shader_path: &std::path::Path,
    ) -> Result<(), RendererError> {
        let wgsl_source = read_shader(&shader_path.to_string_lossy())?;
        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        )?;
        let vertex_fn = compiled.module.entry_points.get("vs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Tonemap vertex entry point not found".into())
        })?;
        let fragment_fn = compiled.module.entry_points.get("fs_main").ok_or_else(|| {
            RendererError::InvalidOperation("Tonemap fragment entry point not found".into())
        })?;
        let pipeline = self
            .context
            .create_graphics_pipeline_with_vertex_descriptor(
                vertex_fn,
                Some(fragment_fn),
                &[MTLPixelFormat::BGRA8Unorm_sRGB],
                None,
                false,
                crate::pipeline::CompareOp::Always,
                objc2_metal::MTLCullMode::None,
                objc2_metal::MTLWinding::Clockwise,
                Some(&super::context::fullscreen_vertex_descriptor()),
                false,
            )?;
        self.tonemap_pipeline = Some(pipeline);
        Ok(())
    }
}
