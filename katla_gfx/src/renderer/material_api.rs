use super::registry::MaterialTextures;
use super::*;
use crate::renderer::pipeline_variant::PipelineVariantKey;
use crate::texture::{
    DEFAULT_ALBEDO_SLOT, DEFAULT_MR_SLOT, DEFAULT_NORMAL_SLOT, DEFAULT_OCCLUSION_SLOT,
};

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
        if !self.asset_registry.get_material(material).is_some() {
            return Err(RendererError::InvalidOperation(format!(
                "Material handle {material:?} not found"
            )));
        }
        let key = self.material_variant_key(material, requested_format)?;
        Ok(self.asset_registry.material_variant(material, &key))
    }

    /// Drop every compiled variant of every material and retire the
    /// pipelines.
    ///
    /// Called after descriptor layout changes (e.g., light culling resize):
    /// each variant key includes the affected layouts, so no variant
    /// survives and the next use recompiles against the new layouts.
    pub(crate) fn invalidate_compiled_materials(&mut self) {
        let variants = self.asset_registry.material_variant_count();
        let materials = self.asset_registry.material_count();
        log::info!(
            "Invalidating {variants} compiled pipeline variants across {materials} materials \
             after descriptor layout change"
        );
        for pipeline in self.asset_registry.invalidate_all_material_variants() {
            self.retire(pipeline);
        }
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
    /// This is the only place material texture handles become shader-visible
    /// numbers. `NONE` and stale handles resolve to the role's default
    /// texture slot, so a dead handle can never sample whatever texture now
    /// occupies a recycled slot. Public for validation against the prepared
    /// binding table (tests and diagnostics).
    pub fn resolve_material_texture_slots(&self, material: MaterialHandle) -> [u32; 4] {
        let textures = self
            .asset_registry
            .get_material(material)
            .map(|m| m.textures)
            .unwrap_or_default();
        [
            self.resolve_texture_slot(textures.albedo, DEFAULT_ALBEDO_SLOT),
            self.resolve_texture_slot(textures.normal, DEFAULT_NORMAL_SLOT),
            self.resolve_texture_slot(textures.metallic_roughness, DEFAULT_MR_SLOT),
            self.resolve_texture_slot(textures.occlusion, DEFAULT_OCCLUSION_SLOT),
        ]
    }

    fn resolve_texture_slot(&self, handle: TextureHandle, fallback_slot: u32) -> u32 {
        self.texture_manager
            .get_bindless_slot(handle)
            .unwrap_or(fallback_slot)
    }

    /// Returns the default white PBR material handle.
    ///
    /// The default material is a simple bindless PBR material that renders
    /// geometry with white albedo and default PBR parameters.
    ///
    /// # Panics
    /// Panics if `init_default_material()` has not been called.
    ///
    /// # Example
    /// ```ignore
    /// // Initialize first (typically during application startup)
    /// renderer.init_default_material(binding, PathBuf::from("shaders/pbr.wgsl"));
    ///
    /// // Then use the default material
    /// let material = renderer.default_material();
    /// let draw = DrawCall::new(mesh, material);
    /// ```
    pub fn default_material(&self) -> MaterialHandle {
        self.default_material_handle
            .expect("default_material() called before init_default_material()")
    }

    /// Drop every compiled variant of materials whose shader matches the
    /// changed file, invalidating cached shader modules for the path.
    ///
    /// The next use of each affected material recompiles the variants it
    /// needs from disk. Returns the number of affected materials.
    pub(crate) fn recompile_materials_for_shader(
        &mut self,
        changed_path: &std::path::Path,
    ) -> usize {
        let matches = self.asset_registry.materials_for_shader(changed_path);
        if matches.is_empty() {
            return 0;
        }

        log::info!(
            "Invalidating pipeline variants of {} material(s) for shader: {}",
            matches.len(),
            changed_path.display()
        );

        self.material_compiler.invalidate_shader_cache(changed_path);
        for (handle, _) in &matches {
            for pipeline in self.asset_registry.take_material_variants(*handle) {
                self.retire(pipeline);
            }
        }
        matches.len()
    }
}
