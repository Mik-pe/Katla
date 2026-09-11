use objc2::runtime::ProtocolObject;
use objc2_metal::MTLPixelFormat;

use crate::error::RendererError;
use crate::handle::MaterialHandle;
use crate::renderer::pipeline_descriptor::PipelineDescriptor;
use crate::renderer::pipeline_variant::PipelineVariantKey;
use crate::texture::ImageFormat;

use super::metal_renderer::{MetalMaterial, MetalRenderer, read_shader};
use super::shader;

impl MetalRenderer {
    pub(crate) fn compile_material_impl(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        descriptor.validate()?;
        if !descriptor.specialization.is_empty() {
            return Err(RendererError::UnsupportedFeature(
                "specialization constants are not yet supported by the Metal backend".to_string(),
            ));
        }
        if descriptor.stages.is_compute() {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines are not yet supported by Metal compile_material".to_string(),
            ));
        }

        // Compile the declared configuration eagerly; further configurations
        // compile on demand when a pass first uses the material
        // (ensure_material_variant_impl). `Auto` keeps the legacy default
        // (HDR geometry for the app's tonemap chain, sRGB UI).
        let declared_format = match descriptor.color_format {
            ImageFormat::Auto if !descriptor.is_ui_layout() => ImageFormat::R16G16B16A16Sfloat,
            ImageFormat::Auto => ImageFormat::B8G8R8A8Srgb,
            format => format,
        };
        let key = PipelineVariantKey::resolve(descriptor, declared_format);
        let pipeline = self.build_pipeline_for_key(descriptor, &key)?;

        // For UI, also compile the instanced entry points into the shared
        // instanced pipeline used by the UI record encoder. Everything is
        // built before the material is registered so a failure here leaves
        // no half-registered material behind.
        let instanced = if descriptor.is_ui_layout() {
            let wgsl_source = read_shader(&descriptor.shader_path)?;
            let instanced_entry_points = vec!["vs_instanced", "fs_instanced"];
            let instanced_compiled = shader::compile_wgsl_to_metal(
                &self.context.device,
                &wgsl_source,
                &instanced_entry_points,
                shader::ShaderProfile::Ui,
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
                        Some(instanced_fs.as_ref() as &ProtocolObject<dyn objc2_metal::MTLFunction>),
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

        let handle = self.materials.insert(MetalMaterial {
            descriptor: descriptor.clone(),
            variants: std::collections::HashMap::from([(key, pipeline)]),
            textures: crate::renderer::registry::MaterialTextures::default(),
        });

        if let Some(instanced) = instanced {
            self.ui_renderer.set_instanced_pipeline(instanced);
        }

        Ok(handle)
    }

    /// Compile one pipeline variant of a material for its canonical key.
    ///
    /// Attachment formats come from the key (the same shared derivation the
    /// Vulkan backend resolves), so both backends build identical logical
    /// variants for the same configuration.
    fn build_pipeline_for_key(
        &mut self,
        descriptor: &PipelineDescriptor,
        key: &PipelineVariantKey,
    ) -> Result<super::pipeline::MetalGraphicsPipeline, RendererError> {
        use crate::pipeline::CullMode;
        use crate::renderer::pipeline_descriptor::PipelineStages;
        use crate::vertex::VertexLayout;

        let PipelineStages::Graphics {
            vertex_entry,
            fragment_entry,
        } = &descriptor.stages
        else {
            return Err(RendererError::UnsupportedFeature(
                "compute pipelines have no graphics variants".to_string(),
            ));
        };

        let wgsl_source = read_shader(&descriptor.shader_path)?;

        log::debug!(
            "compile_material: shader_path={}, wgsl_size={} bytes",
            descriptor.shader_path,
            wgsl_source.len()
        );
        if wgsl_source.contains("pbr_lighting") {
            log::debug!("compile_material: WGSL contains PBR lighting code");
        }

        let entry_points = vec![vertex_entry.as_str(), fragment_entry.as_str()];

        let profile = if descriptor.is_ui_layout() {
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

        // Attachment formats come from the variant key, not from renderer
        // state: the resolved color format plus the shared depth derivation.
        let color_formats = &[super::format::to_mtl_pixel_format(key.color_format())];
        let depth_format = key.depth_format().map(super::format::to_mtl_pixel_format);

        let is_skinned = descriptor.vertex == VertexLayout::pbr_skinned();
        let is_billboard =
            descriptor.vertex == VertexLayout::pbr() && matches!(descriptor.cull, CullMode::None);

        let pipeline = if descriptor.is_ui_layout() {
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

        Ok(pipeline)
    }

    /// Ensure a pipeline variant of the material is compiled for a color
    /// format.
    ///
    /// Cached variants return immediately; a miss compiles one new pipeline
    /// for this exact target configuration. Called with every pass's
    /// declared output format before Metal encodes any draw lists.
    pub(crate) fn ensure_material_variant_impl(
        &mut self,
        material: MaterialHandle,
        requested_format: ImageFormat,
    ) -> Result<(), RendererError> {
        let descriptor = {
            let mat = self.materials.get(material).ok_or_else(|| {
                RendererError::InvalidOperation(format!("Material handle {material:?} not found"))
            })?;
            let key = PipelineVariantKey::resolve(&mat.descriptor, requested_format);
            if mat.variants.contains_key(&key) {
                return Ok(());
            }
            mat.descriptor.clone()
        };

        let key = PipelineVariantKey::resolve(&descriptor, requested_format);
        log::debug!(
            "Compiling pipeline variant for material {material:?} (color {:?}, depth {:?})",
            key.color_format(),
            key.depth_format()
        );
        let pipeline = self.build_pipeline_for_key(&descriptor, &key)?;
        if let Some(mat) = self.materials.get_mut(material) {
            mat.variants.insert(key, pipeline);
        }
        Ok(())
    }

    /// Look up the compiled variant pipeline for a material at a color
    /// format. Encoding runs on `&self`, so every pass's variants must have
    /// been ensured before encoding begins.
    pub(crate) fn material_pipeline(
        &self,
        material: MaterialHandle,
        requested_format: ImageFormat,
    ) -> Result<super::pipeline::MetalGraphicsPipeline, RendererError> {
        let mat = self.materials.get(material).ok_or_else(|| {
            RendererError::InvalidOperation(format!("Material handle {material:?} not found"))
        })?;
        let key = PipelineVariantKey::resolve(&mat.descriptor, requested_format);
        mat.variants.get(&key).cloned().ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Material {material:?} has no pipeline variant for color {:?} / depth {:?}",
                key.color_format(),
                key.depth_format()
            ))
        })
    }

    /// Whether the handle references a registered material.
    ///
    /// The collect-time variant pre-compilation skips unknown handles with
    /// a warning, matching the encoder's per-draw resilience; compile
    /// failures for known materials stay fatal.
    pub(crate) fn has_material_impl(&self, material: MaterialHandle) -> bool {
        self.materials.get(material).is_some()
    }

    pub(crate) fn set_material_textures_impl(
        &mut self,
        material: MaterialHandle,
        textures: crate::renderer::registry::MaterialTextures,
    ) {
        if let Some(mat) = self.materials.get_mut(material) {
            mat.textures = textures;
        }
    }

    /// Resolve a material's typed texture bindings to argument-table
    /// indices. `NONE` and stale handles resolve to the role's default
    /// texture slot; this is the only place material texture handles
    /// become shader-visible numbers on Metal.
    pub(crate) fn resolve_material_texture_slots_impl(&self, material: MaterialHandle) -> [u32; 4] {
        use crate::texture::{
            DEFAULT_ALBEDO_SLOT, DEFAULT_MR_SLOT, DEFAULT_NORMAL_SLOT, DEFAULT_OCCLUSION_SLOT,
        };
        let textures = self
            .materials
            .get(material)
            .map(|mat| mat.textures)
            .unwrap_or_default();
        [
            self.get_bindless_slot_impl(textures.albedo)
                .unwrap_or(DEFAULT_ALBEDO_SLOT),
            self.get_bindless_slot_impl(textures.normal)
                .unwrap_or(DEFAULT_NORMAL_SLOT),
            self.get_bindless_slot_impl(textures.metallic_roughness)
                .unwrap_or(DEFAULT_MR_SLOT),
            self.get_bindless_slot_impl(textures.occlusion)
                .unwrap_or(DEFAULT_OCCLUSION_SLOT),
        ]
    }

    pub(crate) fn default_material_impl(&self) -> MaterialHandle {
        self.default_material.unwrap_or_default()
    }

    pub(crate) fn destroy_material_impl(&mut self, handle: MaterialHandle) {
        self.materials.remove(handle);
    }

    /// Drop every compiled variant of materials whose shader matches the
    /// changed file. The next use of each affected material recompiles the
    /// variants it needs from disk. Returns the number of affected
    /// materials.
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
            .filter_map(|(handle, mat)| {
                let mat_file = std::path::Path::new(&mat.descriptor.shader_path)
                    .file_name()?
                    .to_str()?;
                (mat_file == file_name).then_some(handle)
            })
            .collect();

        let count = handles.len();
        for handle in handles {
            if let Some(mat) = self.materials.get_mut(handle) {
                log::info!(
                    "Invalidating pipeline variants of material {handle:?} for shader '{}'",
                    mat.descriptor.shader_path
                );
                mat.variants.clear();
            }
        }
        count
    }
}
