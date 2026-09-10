use objc2::runtime::ProtocolObject;
use objc2_metal::MTLPixelFormat;

use crate::error::RendererError;
use crate::handle::MaterialHandle;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;

use super::metal_renderer::{MetalMaterial, MetalRenderer, read_shader};
use super::shader;

impl MetalRenderer {
    pub(crate) fn compile_material_impl(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::pipeline::CullMode;
        use crate::renderer::pipeline_descriptor::PipelineStages;
        use crate::vertex::VertexLayout;

        descriptor.validate()?;
        if !descriptor.specialization.is_empty() {
            return Err(RendererError::UnsupportedFeature(
                "specialization constants are not yet supported by the Metal backend".to_string(),
            ));
        }
        let PipelineStages::Graphics {
            vertex_entry,
            fragment_entry,
        } = &descriptor.stages
        else {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines are not yet supported by Metal compile_material".to_string(),
            ));
        };

        let shader_path = &descriptor.shader_path;
        let wgsl_source = read_shader(shader_path)?;

        log::debug!(
            "compile_material: shader_path={}, wgsl_size={} bytes",
            shader_path,
            wgsl_source.len()
        );
        if wgsl_source.contains("pbr_lighting") {
            log::debug!("compile_material: WGSL contains PBR lighting code");
        }

        let entry_points = vec![vertex_entry.as_str(), fragment_entry.as_str()];

        let is_ui = descriptor.is_ui_layout();
        let profile = if is_ui {
            shader::ShaderProfile::Ui
        } else {
            shader::ShaderProfile::Graphics
        };

        let compiled = shader::compile_wgsl_to_metal(
            &self.context.device,
            &wgsl_source,
            &entry_points,
            profile.clone(),
        )?;

        let vertex_fn = compiled
            .module
            .entry_points
            .get(vertex_entry.as_str())
            .ok_or_else(|| {
                RendererError::InvalidOperation("Vertex entry point not found".into())
            })?;

        let fragment_fn = compiled.module.entry_points.get(fragment_entry.as_str());

        // Derive the bindless argument-buffer encoder from an actual compiled
        // shader function. This avoids the arbitrary-layout MTLDevice API that
        // raises an Objective-C exception on AppleParavirtDevice.
        if !self.bindless_manager.is_initialized() {
            let fragment_function = fragment_fn.ok_or_else(|| {
                RendererError::InitializationFailed(
                    "The first Metal graphics material has no fragment function for bindless layout reflection"
                        .into(),
                )
            })?;
            self.bindless_manager
                .initialize_from_function(fragment_function.as_ref())?;
        }

        // For UI, also compile instanced entry points for a second pipeline
        let instanced_pipeline = if is_ui {
            let instanced_entry_points = vec!["vs_instanced", "fs_instanced"];
            let instanced_compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl_source,
                &instanced_entry_points,
                profile.clone(),
            )?;
            let instanced_vs = instanced_compiled
                .module
                .entry_points
                .get("vs_instanced")
                .ok_or_else(|| {
                    RendererError::InvalidOperation("Instanced vertex entry point not found".into())
                })?;
            let instanced_fs = instanced_compiled
                .module
                .entry_points
                .get("fs_instanced")
                .ok_or_else(|| {
                    RendererError::InvalidOperation(
                        "Instanced fragment entry point not found".into(),
                    )
                })?;

            let vd = super::context::ui_instanced_vertex_descriptor();
            Some(
                self.context
                    .create_graphics_pipeline_with_vertex_descriptor(
                        instanced_vs.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>,
                        Some(
                            instanced_fs.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>,
                        ),
                        &[MTLPixelFormat::BGRA8Unorm_sRGB],
                        None,
                        false,
                        crate::pipeline::CompareOp::Always,
                        objc2_metal::MTLCullMode::None,
                        objc2_metal::MTLWinding::Clockwise,
                        Some(&vd),
                        true,
                    )?,
            )
        } else {
            None
        };

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

        let is_skinned = descriptor.vertex == VertexLayout::pbr_skinned();
        let is_billboard =
            descriptor.vertex == VertexLayout::pbr() && matches!(descriptor.cull, CullMode::None);

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
            let vd = super::context::default_pbr_vertex_descriptor();
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
        };

        let material = MetalMaterial {
            pipeline: Some(pipeline),
            texture_indices: [0, 1, 2, 0],
            shader_path: Some(shader_path.clone()),
            descriptor: Some(descriptor.clone()),
        };
        let id = self.materials.insert(material);
        let handle = MaterialHandle::new(id);

        if let Some(inst) = instanced_pipeline {
            self.ui_renderer.set_instanced_pipeline(inst);
        }

        Ok(handle)
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
            let descriptor = {
                let Some(mat) = self.materials.get(handle.index()) else {
                    continue;
                };
                match mat.descriptor.as_ref() {
                    Some(d) => d.clone(),
                    None => continue,
                }
            };

            match self.compile_material_impl(&descriptor) {
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
                        "Failed to recompile material for shader '{}': {}",
                        descriptor.shader_path,
                        e
                    );
                }
            }
        }
        count
    }
}
