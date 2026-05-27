use super::*;

impl VulkanRenderer {
    /// Create a PBR material with configurable color format.
    ///
    /// This is a convenience method for creating standard PBR materials with
    /// sensible defaults: depth testing enabled, backface culling enabled,
    /// opaque rendering.
    ///
    /// Uses swapchain color format (B8G8R8A8Srgb) by default. Specify HDR format
    /// for rendering to intermediate textures (e.g., for tonemapping passes).
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    /// * `color_format` - Optional color attachment format. None = swapchain format (LDR),
    ///   Some(ImageFormat::R16G16B16A16Sfloat) = HDR rendering
    ///
    /// # Returns
    /// A MaterialHandle for the created material.
    ///
    /// # Example
    /// ```ignore
    /// use katla_gfx::vulkan::material::compiler::{MaterialOptions, VertexType};
    ///
    /// // PBR material (default settings)
    /// let pbr = renderer.compile_material("shaders/pbr.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Pbr,
    ///     ..Default::default()
    /// })?;
    ///
    /// // UI material with alpha blending
    /// let ui = renderer.compile_material("shaders/ui.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Ui,
    ///     alpha_blended: true,
    ///     ..Default::default()
    /// })?;
    ///
    /// // Skinned mesh material for GLTF models
    /// let skinned = renderer.compile_material("shaders/skinned.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Skinned,
    ///     ..Default::default()
    /// })?;
    ///
    /// // HDR material for intermediate render targets
    /// let hdr = renderer.compile_material("shaders/pbr.wgsl", MaterialOptions {
    ///     vertex_type: VertexType::Pbr,
    ///     color_format: ImageFormat::R16G16B16A16Sfloat,
    ///     ..Default::default()
    /// })?;
    /// ```
    pub fn compile_material(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
        options: crate::vulkan::material::compiler::MaterialOptions,
    ) -> Result<MaterialHandle, RendererError> {
        use crate::vulkan::material::compiler::MaterialType;

        let material_type = match options.vertex_type {
            crate::vulkan::material::compiler::VertexType::Pbr => MaterialType::Pbr,
            crate::vulkan::material::compiler::VertexType::Ui => MaterialType::Ui,
            _ => MaterialType::Auto,
        };

        self.material_compiler
            .compile(
                &mut self.asset_registry,
                shader_path.as_ref(),
                material_type,
                options,
            )
            .map_err(RendererError::from)
    }

    /// Ensure a material is compiled for a specific format.
    ///
    /// If the material was created with `ImageFormat::Auto`, this will compile
    /// it for the specified format. If already compiled, this does nothing.
    ///
    /// This is called automatically by the frame graph before execution.
    pub(crate) fn ensure_material_compiled(
        &mut self,
        material: MaterialHandle,
        format: crate::texture::ImageFormat,
    ) -> Result<(), RendererError> {
        self.material_compiler
            .compile_deferred_material(&mut self.asset_registry, material, format)
            .map_err(RendererError::from)
    }

    /// Invalidate all compiled materials, forcing recompilation on next use.
    ///
    /// Called after descriptor layout changes (e.g., light culling resize)
    /// to ensure pipelines reference valid descriptor set layouts.
    pub(crate) fn recompile_deferred_materials(&mut self) {
        let count = self.asset_registry.material_count();
        log::info!(
            "Invalidating {} compiled materials for recompilation after descriptor layout change",
            count
        );
        self.asset_registry.invalidate_compiled_materials();
    }

    /// Set texture indices for a material.
    ///
    /// Updates the material's texture indices for bindless sampling.
    /// Texture indices are obtained from `create_texture_*` methods.
    ///
    /// # Arguments
    /// * `material` - Material handle to update
    /// * `indices` - [albedo, normal, metallic_roughness, ao] texture indices
    pub fn set_material_texture_indices(&mut self, material: MaterialHandle, indices: [u32; 4]) {
        if let Some(mat) = self.asset_registry.get_material_mut(material) {
            mat.textures.texture_indices = indices;
        }
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

    /// Recompile all materials whose shader path matches the given file.
    ///
    /// Invalidates cached shader modules for the path, then recompiles each
    /// matching material in-place (keeping the same handle/slot index).
    /// Returns the number of materials recompiled.
    pub(crate) fn recompile_materials_for_shader(
        &mut self,
        changed_path: &std::path::Path,
    ) -> usize {
        let matches = self.asset_registry.materials_for_shader(changed_path);
        if matches.is_empty() {
            return 0;
        }

        log::info!(
            "Recompiling {} material(s) for shader: {}",
            matches.len(),
            changed_path.display()
        );

        let count = matches.len();
        for (handle, stored_path) in &matches {
            // Invalidate cached shader modules so load_shader re-reads from disk
            self.material_compiler.invalidate_shader_cache(stored_path);

            if let Err(e) = self.recompile_single_material(*handle) {
                log::warn!(
                    "Failed to recompile material {:?} for shader '{}': {}",
                    handle,
                    stored_path.display(),
                    e
                );
            }
        }
        count
    }

    /// Recompile a single material in-place using its stored shader path and options.
    fn recompile_single_material(&mut self, handle: MaterialHandle) -> Result<(), RendererError> {
        use crate::vulkan::material::compiler::{MaterialOptions, MaterialType};

        // Extract stored material info (immutable borrow of registry)
        let (
            shader_path,
            vertex_type,
            is_compositing,
            alpha_blended,
            double_sided,
            wireframe,
            depth_test,
            vertex_binding,
            color_format,
            old_pipeline_handle,
            textures,
        ) = {
            let mat = self.asset_registry.get_material(handle).ok_or_else(|| {
                RendererError::InvalidOperation(format!("Material handle {:?} not found", handle))
            })?;

            let shader_path = mat.shader_path.clone().ok_or_else(|| {
                RendererError::InvalidOperation(format!("Material {:?} has no shader path", handle))
            })?;

            (
                shader_path,
                mat.vertex_type,
                mat.is_compositing,
                mat.alpha_blended,
                mat.double_sided,
                mat.wireframe,
                mat.depth_test,
                mat.vertex_binding.clone(),
                mat.color_format,
                mat.pipeline,
                mat.textures,
            )
        };

        // Load shaders (cache was invalidated, so this reads from disk)
        let vert_module = self
            .material_compiler
            .load_shader(&shader_path, ash::vk::ShaderStageFlags::VERTEX)?;
        let frag_module = self
            .material_compiler
            .load_shader(&shader_path, ash::vk::ShaderStageFlags::FRAGMENT)?;

        let material_type = match vertex_type {
            crate::vulkan::material::compiler::VertexType::Pbr => MaterialType::Pbr,
            crate::vulkan::material::compiler::VertexType::Ui => MaterialType::Ui,
            _ => MaterialType::Auto,
        };

        let options = MaterialOptions {
            color_format,
            vertex_type,
            is_compositing,
            alpha_blended,
            double_sided,
            wireframe,
            depth_test,
        };

        // Build new pipeline via material_compiler
        let pipeline = self.material_compiler.build_pipeline_from_modules(
            &options,
            material_type,
            vert_module,
            frag_module,
            &vertex_binding,
        )?;

        // Register new pipeline and update material in-place
        let new_pipeline_handle = self.asset_registry.register_pipeline(pipeline);

        if let Some(mat) = self.asset_registry.get_material_mut(handle) {
            mat.pipeline = Some(new_pipeline_handle);
            mat.fully_compiled = true;
        }

        // Destroy old pipeline
        if let Some(old) = old_pipeline_handle {
            self.asset_registry.remove_pipeline(old);
        }

        // Restore texture indices
        if let Some(mat) = self.asset_registry.get_material_mut(handle) {
            mat.textures = textures;
        }

        Ok(())
    }
}
