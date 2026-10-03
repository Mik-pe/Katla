use super::registry::MaterialTextures;
use super::*;
use crate::renderer::pipeline_variant::PipelineVariantKey;

impl VulkanRenderer {
    /// Register a material from a validated compilation descriptor.
    ///
    /// The descriptor is the material's identity. A concrete color format
    /// compiles that variant immediately; `ImageFormat::Auto` defers all
    /// compilation until a pass first uses the material, then compiles one
    /// pipeline variant per render-target configuration on demand.
    pub(crate) fn compile_material_descriptor(
        &mut self,
        descriptor: &PipelineDescriptor,
    ) -> Result<MaterialHandle, RendererError> {
        self.material_compiler
            .compile(&mut self.asset_registry, descriptor)
            .map_err(RendererError::from)
    }

    /// Derive the canonical variant key for a material at a requested color
    /// format.
    fn material_variant_key(
        &self,
        material: MaterialHandle,
        requested_format: crate::texture::ImageFormat,
    ) -> Result<PipelineVariantKey, RendererError> {
        let descriptor = self
            .asset_registry
            .get_material(material)
            .ok_or(RendererError::InvalidOperation(format!(
                "Material handle {material:?} not found"
            )))?
            .descriptor
            .clone();
        Ok(PipelineVariantKey::resolve(&descriptor, requested_format))
    }

    /// Ensure a pipeline variant of the material is compiled for a color
    /// format.
    ///
    /// Cached variants return immediately; a miss compiles one new pipeline
    /// for this exact target configuration. Called before any pass encodes
    /// draws with the material.
    pub(crate) fn ensure_material_compiled(
        &mut self,
        material: MaterialHandle,
        requested_format: crate::texture::ImageFormat,
    ) -> Result<(), RendererError> {
        let key = self.material_variant_key(material, requested_format)?;
        if self
            .asset_registry
            .material_variant(material, &key)
            .is_some()
        {
            return Ok(());
        }

        log::debug!(
            "Compiling pipeline variant for material {material:?} (color {:?}, depth {:?})",
            key.color_format(),
            key.depth_format()
        );
        self.material_compiler
            .compile_variant(&mut self.asset_registry, material, &key)
            .map_err(RendererError::from)?;
        Ok(())
    }

    /// Look up the compiled variant for a material at a color format.
    ///
    /// `None` means no variant exists for this configuration yet; callers
    /// must run [`ensure_material_compiled`](Self::ensure_material_compiled)
    /// for every pass format before encoding.
    pub(crate) fn material_variant(
        &self,
        material: MaterialHandle,
        requested_format: crate::texture::ImageFormat,
    ) -> Result<Option<crate::renderer::registry::MaterialVariant>, RendererError> {
        if self.asset_registry.get_material(material).is_none() {
            return Err(RendererError::InvalidOperation(format!(
                "Material handle {material:?} not found"
            )));
        }
        let key = self.material_variant_key(material, requested_format)?;
        Ok(self.asset_registry.material_variant(material, &key))
    }

    /// Set the typed texture bindings for a material.
    ///
    /// Each role refers to a texture by handle; `TextureHandle::NONE`
    /// leaves the role on its fallback texture. The backend resolves
    /// handles to its binding-table representation at upload/encode time,
    /// so materials never store raw slot numbers.
    ///
    /// # Arguments
    /// * `material` - Material handle to update
    /// * `textures` - Handles for [albedo, normal, metallic_roughness, occlusion]
    pub fn set_material_textures(&mut self, material: MaterialHandle, textures: MaterialTextures) {
        if let Some(mat) = self.asset_registry.get_material_mut(material) {
            mat.textures = textures;
        }
    }

    /// Resolve a material's typed texture bindings to bindless slots.
    ///
    /// Object preparation uses the same resolution for material and draw-local
    /// bindings. `NONE` and stale handles resolve to the role's default
    /// texture slot, so a dead handle can never sample whatever texture now
    /// occupies a recycled slot. Public for validation against the prepared
    /// binding table (tests and diagnostics).
    pub fn resolve_material_texture_slots(&self, material: MaterialHandle) -> [u32; 4] {
        let textures = self
            .asset_registry
            .get_material(material)
            .map(|m| m.textures)
            .unwrap_or_default();
        self.resolve_texture_slots(textures)
    }

    pub(crate) fn resolve_texture_slots(&self, textures: MaterialTextures) -> [u32; 4] {
        [
            self.resolve_texture_slot(textures.albedo, 0),
            self.resolve_texture_slot(textures.normal, 0),
            self.resolve_texture_slot(textures.metallic_roughness, 0),
            self.resolve_texture_slot(textures.occlusion, 0),
        ]
    }

    fn resolve_texture_slot(&self, handle: TextureHandle, fallback_slot: u32) -> u32 {
        self.texture_manager
            .get_bindless_slot(handle)
            .unwrap_or(fallback_slot)
    }

    /// Prepare every live variant before replacing a material and its interface.
    /// Failed preparation retains the last usable material; submitted pipelines retire.
    pub(crate) fn recompile_materials_for_shader(
        &mut self,
        changed_path: &std::path::Path,
    ) -> usize {
        let matches = self.asset_registry.materials_for_shader(changed_path);
        for (handle, path) in &matches {
            let result = self.prepare_material_replacement(*handle, path);
            match result {
                Ok(replacement) => {
                    let previous =
                        if let Some(material) = self.asset_registry.get_material_mut(*handle) {
                            material.interface = replacement.interface;
                            material.dependencies = replacement.dependencies;
                            std::mem::replace(&mut material.variants, replacement.variants)
                        } else {
                            replacement.variants
                        };
                    for variant in previous.into_values() {
                        for pipeline in [Some(variant.pipeline), variant.instanced_pipeline]
                            .into_iter()
                            .flatten()
                        {
                            if let Some(pipeline) = self.asset_registry.remove_pipeline(pipeline) {
                                self.retire(pipeline);
                            }
                        }
                    }
                    log::info!("pipeline_cache event=reload_swap material={handle:?}");
                }
                Err(error) => log::warn!(
                    "pipeline_cache event=reload_failed material={handle:?} reason={error}"
                ),
            }
        }
        matches.len()
    }

    fn prepare_material_replacement(
        &mut self,
        handle: MaterialHandle,
        path: &std::path::Path,
    ) -> Result<super::registry::MaterialAsset, RendererError> {
        let material = self
            .asset_registry
            .get_material(handle)
            .ok_or_else(|| RendererError::InvalidOperation("Reload material unavailable".into()))?;
        let descriptor = material.descriptor.clone();
        let keys: Vec<_> = material.variants.keys().cloned().collect();
        let source = super::shader_source::ShaderSource::load(path)
            .map_err(|error| RendererError::InvalidOperation(error.to_string()))?;
        let interface =
            super::graphics_interface::GraphicsInterface::reflect(&source.code, &descriptor.stages)
                .map_err(RendererError::InvalidOperation)?;
        let staged = self
            .asset_registry
            .register_material(super::registry::MaterialAsset {
                descriptor,
                interface: Some(interface),
                dependencies: source.dependencies.clone(),
                textures: MaterialTextures::default(),
                variants: Default::default(),
            });
        for key in keys {
            if let Err(error) = self.material_compiler.compile_variant_from_source(
                &mut self.asset_registry,
                staged,
                &key,
                &source,
            ) {
                self.destroy_material(staged);
                return Err(error.into());
            }
        }
        self.asset_registry.remove_material(staged).ok_or_else(|| {
            RendererError::InvalidOperation("Prepared reload material unavailable".into())
        })
    }
}
