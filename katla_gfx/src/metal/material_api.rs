use objc2::runtime::ProtocolObject;
use objc2_metal::MTLPixelFormat;

use crate::error::RendererError;
use crate::handle::MaterialHandle;

use super::metal_renderer::{MetalMaterial, MetalRenderer, read_shader};
use super::shader;

impl MetalRenderer {
    pub(crate) fn compile_material_impl(
        &mut self,
        shader_path: &str,
        vertex_type: &str,
    ) -> Result<MaterialHandle, RendererError> {
        let wgsl_source = read_shader(shader_path)?;

        log::debug!(
            "compile_material: shader_path={}, wgsl_size={} bytes",
            shader_path,
            wgsl_source.len()
        );
        if wgsl_source.contains("pbr_lighting") {
            log::debug!("compile_material: WGSL contains PBR lighting code");
        }

        let entry_points = match vertex_type {
            "compute" => vec!["cs_main"],
            _ => vec!["vs_main", "fs_main"],
        };

        let is_ui = vertex_type == "ui";

        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &entry_points,
            is_ui,
        )?;

        let vertex_fn = compiled
            .module
            .entry_points
            .get("vs_main")
            .or_else(|| compiled.module.entry_points.get("cs_main"))
            .ok_or_else(|| {
                RendererError::InvalidOperation("Vertex entry point not found".into())
            })?;

        let fragment_fn = compiled.module.entry_points.get("fs_main");

        let color_formats = if is_ui {
            &[MTLPixelFormat::BGRA8Unorm_sRGB]
        } else {
            &[MTLPixelFormat::RGBA16Float]
        };
        let depth_format = if is_ui {
            None
        } else {
            Some(MTLPixelFormat::Depth32Float_Stencil8)
        };

        let is_skinned = vertex_type == "skinned";

        let pipeline = if is_ui {
            let vd = super::context::ui_vertex_descriptor();
            self.context
                .create_graphics_pipeline_with_vertex_descriptor(
                    vertex_fn,
                    fragment_fn
                        .as_ref()
                        .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                    color_formats,
                    depth_format,
                    false,
                    crate::pipeline::CompareOp::Always,
                    objc2_metal::MTLCullMode::None,
                    objc2_metal::MTLWinding::Clockwise,
                    Some(&vd),
                    true,
                )?
        } else if is_skinned {
            let vd = super::context::pbr_skinned_vertex_descriptor();
            self.context
                .create_graphics_pipeline_with_vertex_descriptor(
                    vertex_fn,
                    fragment_fn
                        .as_ref()
                        .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                    color_formats,
                    depth_format,
                    true,
                    crate::pipeline::CompareOp::GreaterOrEqual,
                    objc2_metal::MTLCullMode::Back,
                    objc2_metal::MTLWinding::Clockwise,
                    Some(&vd),
                    false,
                )?
        } else {
            self.context.create_graphics_pipeline(
                vertex_fn,
                fragment_fn
                    .as_ref()
                    .map(|f| f.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
                color_formats,
                depth_format,
                true,
                crate::pipeline::CompareOp::GreaterOrEqual,
                objc2_metal::MTLCullMode::Back,
                objc2_metal::MTLWinding::Clockwise,
            )?
        };

        let material = MetalMaterial {
            pipeline: Some(pipeline),
            texture_indices: [0, 1, 2, 0],
        };
        let id = self.materials.insert(material);
        Ok(MaterialHandle::new(id))
    }

    pub(crate) fn set_material_texture_indices_impl(
        &mut self,
        material: MaterialHandle,
        indices: [u32; 4],
    ) {
        if let Some(mat) = self.materials.get_mut(material.index()) {
            mat.texture_indices = indices;
        }
    }

    pub(crate) fn default_material_impl(&self) -> MaterialHandle {
        self.default_material.unwrap_or(MaterialHandle::default())
    }

    pub(crate) fn destroy_material_impl(&mut self, handle: MaterialHandle) {
        self.materials.remove(handle.index());
    }
}
