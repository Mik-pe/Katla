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
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
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
            .map_err(|e| {
                RendererError::InitializationFailed(format!("Material compilation failed: {}", e))
            })
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

    /// Set the HDR texture index for tonemapping.
    ///
    /// Sets object[0].texture_indices.x to the HDR texture bindless index.
    /// The tonemap shader reads from objects[0] to get the HDR texture index.
    ///
    /// # Arguments
    /// * `hdr_texture_index` - Bindless texture index for HDR color attachment
    ///
    /// Set the HDR texture index for tonemapping.
    ///
    /// This method sets up object[0] in the storage buffer to pass the HDR texture index
    /// to fullscreen shaders (like the tonemap pass). The tonemap shader reads from
    /// `objects[0].texture_indices.x` to get the bindless texture slot.
    ///
    /// # Contract
    /// - Object index 0 is reserved for fullscreen/post-processing shader parameters
    /// - The HDR texture must already be registered with the bindless system
    /// - Tonemap shaders must read from `objects[0].texture_indices.x`
    ///
    /// # Arguments
    /// * `hdr_texture_index` - Bindless texture slot index for the HDR color attachment
    ///
    /// # Example
    /// ```ignore
    /// // Register HDR texture with bindless
    /// let hdr_slot = frame_graph.register_transient_texture_bindless(&mut renderer, "hdr_color")?;
    ///
    /// // Set up tonemap shader to sample from HDR texture
    /// renderer.set_hdr_texture_index(hdr_slot);
    /// ```
    pub fn set_hdr_texture_index(&mut self, hdr_texture_index: u32) {
        // Set object[0] texture indices (HDR texture in x, others unused)
        //
        // Note: Object index 0 is reserved for fullscreen/post-processing shader parameters.
        // This is a documented contract between the renderer and fullscreen shaders.
        let frame_idx = self.current_frame();
        self.storage_manager.update_object_bindless(
            frame_idx,
            0, // object index 0 is reserved for tonemap params
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ], // identity matrix (not used)
            &[1.0, 1.0, 1.0, 1.0], // white color (not used)
            0.0, // metallic (not used)
            0.0, // roughness (not used)
            1.0, // ao (not used)
            0.0, // emission index (not used)
            [hdr_texture_index, 0, 0, 0], // HDR texture index in x
        );
    }

    /// Create a material with custom options using the builder pattern.
    ///
    /// This is the advanced API for materials requiring custom configuration
    /// (alpha blending, double-sided rendering, wireframe mode, etc.).
    ///
    /// # Arguments
    /// * `shader_path` - Path to WGSL shader file
    ///
    /// # Returns
    /// A MaterialBuilder for configuring the material.
    ///
    /// # When to use this
    ///
    /// Most applications should use `compile_material()` with `MaterialOptions`.
    /// This method is intended for:
    /// - GLTF model loaders that need custom vertex types (Skinned)
    /// - Advanced material configuration beyond PBR defaults
    /// - Custom render targets with specific color formats
    ///
    /// # Example (GLTF loading with skinned meshes)
    /// ```ignore
    /// let material = renderer
    ///     .material_builder(&shader_path)
    ///     .with_vertex_type(VertexType::Skinned)
    ///     .with_color_format(ImageFormat::R16G16B16A16Sfloat)
    ///     .build()?;
    /// ```
    pub fn material_builder(
        &mut self,
        shader_path: impl AsRef<std::path::Path>,
    ) -> MaterialBuilder<'_> {
        MaterialBuilder::new(self, shader_path.as_ref().to_path_buf())
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
}
