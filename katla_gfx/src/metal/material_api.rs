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
        let profile = if is_ui {
            shader::ShaderProfile::Ui
        } else {
            shader::ShaderProfile::Graphics
        };

        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &entry_points,
            profile,
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
        let is_billboard = vertex_type == "billboard";

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
        } else if is_billboard {
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
                    objc2_metal::MTLCullMode::None,
                    objc2_metal::MTLWinding::Clockwise,
                    None,
                    true,
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
            shader_path: Some(shader_path.to_string()),
            vertex_type: Some(vertex_type.to_string()),
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
        self.default_material.unwrap_or_default()
    }

    pub(crate) fn destroy_material_impl(&mut self, handle: MaterialHandle) {
        self.materials.remove(handle.index());
    }

    /// Recompile all materials whose shader path matches the given file.
    ///
    /// Iterates all stored materials, finds those compiled from the changed
    /// shader, and recompiles their pipelines in-place (keeping the same handle).
    pub(crate) fn recompile_materials_for_shader_impl(
        &mut self,
        changed_path: &std::path::Path,
    ) -> usize {
        let file_name = changed_path.file_name().and_then(|n| n.to_str());
        let Some(file_name) = file_name else {
            return 0;
        };

        let handles: Vec<MaterialHandle> = self
            .materials
            .iter_enumerated()
            .filter_map(|(idx, mat)| {
                let sp = mat.shader_path.as_ref()?;
                let mat_file = std::path::Path::new(sp).file_name()?.to_str()?;
                if mat_file == file_name {
                    Some(MaterialHandle::new(idx))
                } else {
                    None
                }
            })
            .collect();

        let count = handles.len();
        for handle in handles {
            let (shader_path, vertex_type) = {
                let Some(mat) = self.materials.get(handle.index()) else {
                    continue;
                };
                let sp = match mat.shader_path.as_ref() {
                    Some(p) => p.clone(),
                    None => continue,
                };
                let vt = match mat.vertex_type.as_ref() {
                    Some(v) => v.clone(),
                    None => continue,
                };
                (sp, vt)
            };

            match self.compile_material_impl(&shader_path, &vertex_type) {
                Ok(new_handle) => {
                    let new_pipeline = self
                        .materials
                        .get(new_handle.index())
                        .and_then(|m| m.pipeline.clone());
                    if let Some(old_mat) = self.materials.get_mut(handle.index()) {
                        old_mat.pipeline = new_pipeline;
                    }
                    self.materials.remove(new_handle.index());
                }
                Err(e) => {
                    log::warn!(
                        "Failed to recompile material '{}' for shader '{}': {}",
                        vertex_type,
                        shader_path,
                        e
                    );
                }
            }
        }
        count
    }
}
