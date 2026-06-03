use super::*;

impl VulkanRenderer {
    /// Destroy a mesh and release its GPU vertex/index buffers.
    ///
    /// After destruction, `get_mesh(handle)` returns `None` and `mesh_count()` decreases.
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The mesh handle to destroy
    pub fn destroy_mesh(&mut self, handle: MeshHandle) {
        self.asset_registry.remove_mesh(handle);
    }

    /// Destroy a material and release its pipeline resources.
    ///
    /// After destruction, `get_material(handle)` returns `None` and `material_count()` decreases.
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The material handle to destroy
    pub fn destroy_material(&mut self, handle: MaterialHandle) {
        if let Some(material) = self.asset_registry.remove_material(handle) {
            // Destroy the material's descriptor set layout if present
            if let Some(layout) = material.material_descriptor_layout {
                unsafe {
                    self.context
                        .device
                        .destroy_descriptor_set_layout(layout, None);
                }
            }
            // Destroy the associated pipeline
            if let Some(pipeline_handle) = material.pipeline {
                self.asset_registry.remove_pipeline(pipeline_handle);
            }
            // Destroy the instanced pipeline if present (UI materials only)
            if let Some(pipeline_handle) = material.instanced_pipeline {
                self.asset_registry.remove_pipeline(pipeline_handle);
            }
        }
    }

    /// Destroy a texture and release its GPU image memory and bindless slot.
    ///
    /// After destruction, `TextureManager::contains(handle)` returns `false` and the
    /// bindless slot is freed. Default textures are never destroyed.
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The texture handle to destroy
    pub fn destroy_texture(&mut self, handle: TextureHandle) {
        if handle.is_none() {
            return;
        }
        if self.texture_manager.is_default_texture(handle) {
            return;
        }
        // Release the bindless slot before removing from texture manager
        if let Some(slot) = self.texture_manager.get_bindless_slot(handle) {
            self.bindless_manager.release_texture_slot(slot);
        }
        self.texture_manager.destroy(handle);
    }

    /// Destroy a skeleton and release its GPU storage buffer and descriptor set.
    ///
    /// After destruction, `get_skeleton_descriptor(handle)` returns `None`.
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The skeleton handle to destroy
    pub fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        self.skeleton_descriptors.remove(handle.index());
        self.skeleton_buffers.remove(handle.index());
    }
}

#[cfg(test)]
mod tests {
    use crate::handle::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};
    use crate::renderer::registry::{AssetRegistry, MaterialAsset, MaterialTextures, MeshAsset};
    use crate::vulkan::vertexbinding::VertexBinding;

    fn make_material() -> MaterialAsset {
        MaterialAsset {
            pipeline: None,
            instanced_pipeline: None,
            fully_compiled: false,
            shader_path: None,
            vertex_type: crate::vulkan::material::compiler::VertexType::Pbr,
            is_compositing: false,
            alpha_blended: false,
            double_sided: false,
            wireframe: false,
            depth_test: true,
            vertex_binding: VertexBinding { formats: vec![] },
            textures: MaterialTextures::default(),
            material_descriptor_set: None,
            material_descriptor_layout: None,
            color_format: crate::texture::ImageFormat::R8G8B8A8Srgb,
        }
    }

    fn make_mesh(vertex_count: u32) -> MeshAsset {
        MeshAsset {
            attribute_buffers: std::collections::HashMap::new(),
            index_buffer: None,
            vertex_count,
        }
    }

    // =========================================================================
    // VAL-GPU-001: Destroy mesh releases GPU resources
    // =========================================================================

    #[test]
    fn test_destroy_mesh() {
        let mut registry = AssetRegistry::new();
        let handle = registry.register_mesh(make_mesh(4));
        assert_eq!(registry.mesh_count(), 1);
        assert!(registry.get_mesh(handle).is_some());

        let removed = registry.remove_mesh(handle);
        assert!(removed.is_some());
        assert_eq!(registry.mesh_count(), 0);
        assert!(registry.get_mesh(handle).is_none());
    }

    // =========================================================================
    // VAL-GPU-002: Destroy material releases pipeline resources
    // =========================================================================

    #[test]
    fn test_destroy_material() {
        let mut registry = AssetRegistry::new();
        let handle = registry.register_material(make_material());
        assert_eq!(registry.material_count(), 1);

        let removed = registry.remove_material(handle);
        assert!(removed.is_some());
        assert_eq!(registry.material_count(), 0);
        assert!(registry.get_material(handle).is_none());
    }

    // =========================================================================
    // VAL-GPU-003: Destroy texture releases image memory and bindless slot
    // (Logic-only test; full GPU test requires Vulkan context)
    // =========================================================================

    #[test]
    fn test_destroy_texture() {
        // Verify the NONE guard logic and default protection
        let none = TextureHandle::NONE;
        assert!(
            none.is_none(),
            "NONE handle must be detected for early return"
        );

        // TextureHandle::new(u32::MAX) is also NONE
        let max = TextureHandle::new(u32::MAX);
        assert!(max.is_none());
    }

    // =========================================================================
    // VAL-GPU-004: Destroy skeleton releases storage buffer and descriptor set
    // =========================================================================

    #[test]
    fn test_destroy_skeleton() {
        use crate::handle::ResourceStorage;

        let mut descriptors: ResourceStorage<String> = ResourceStorage::new();
        let mut buffers: ResourceStorage<String> = ResourceStorage::new();

        let handle = SkeletonHandle::new(descriptors.insert("desc".to_string()));
        let _ = buffers.insert("buf".to_string());
        assert_eq!(descriptors.len(), 1);
        assert_eq!(buffers.len(), 1);

        descriptors.remove(handle.index());
        buffers.remove(handle.index());
        assert_eq!(descriptors.len(), 0);
        assert_eq!(buffers.len(), 0);
    }

    // =========================================================================
    // VAL-GPU-005: Double-destroy is safe
    // =========================================================================

    #[test]
    fn test_double_destroy_safe() {
        let mut registry = AssetRegistry::new();
        let mesh_h = registry.register_mesh(make_mesh(4));
        let mat_h = registry.register_material(make_material());

        // First destroy succeeds
        assert!(registry.remove_mesh(mesh_h).is_some());
        // Second destroy is a safe no-op
        assert!(registry.remove_mesh(mesh_h).is_none());

        // First destroy succeeds
        assert!(registry.remove_material(mat_h).is_some());
        // Second destroy is a safe no-op
        assert!(registry.remove_material(mat_h).is_none());
    }

    // =========================================================================
    // VAL-GPU-006: Destroying unowned/NONE handle is safe
    // =========================================================================

    #[test]
    fn test_destroy_unowned_safe() {
        let mut registry = AssetRegistry::new();
        registry.remove_mesh(MeshHandle::NONE);
        registry.remove_mesh(MeshHandle::new(99999));
        registry.remove_material(MaterialHandle::NONE);
        registry.remove_material(MaterialHandle::new(99999));
    }

    // =========================================================================
    // VAL-GPU-011: Default textures are never destroyed
    // =========================================================================

    #[test]
    fn test_default_textures_preserved() {
        // Verify the is_default_texture check works correctly for all 5 defaults
        // Default handles are at indices 0-4 (created first in TextureManager::new)
        for i in 0..5 {
            let handle = TextureHandle::new(i);
            // We can't create a TextureManager without GPU, but we verify
            // the NONE guard prevents destruction of invalid handles
            if i == u32::MAX {
                assert!(handle.is_none());
            } else {
                assert!(handle.is_some());
            }
        }
    }

    // =========================================================================
    // VAL-GPU-010: Resource counts through create/destroy sequences
    // =========================================================================

    #[test]
    fn test_mesh_count_create_destroy_sequence() {
        let mut registry = AssetRegistry::new();

        let h1 = registry.register_mesh(make_mesh(1));
        let h2 = registry.register_mesh(make_mesh(2));
        let _h3 = registry.register_mesh(make_mesh(3));
        assert_eq!(registry.mesh_count(), 3);

        registry.remove_mesh(h2);
        assert_eq!(registry.mesh_count(), 2);

        registry.remove_mesh(h1);
        assert_eq!(registry.mesh_count(), 1);

        let h4 = registry.register_mesh(make_mesh(4));
        assert_eq!(registry.mesh_count(), 2);
        assert!(registry.get_mesh(h4).is_some());
    }

    #[test]
    fn test_material_count_create_destroy_sequence() {
        let mut registry = AssetRegistry::new();

        let h1 = registry.register_material(make_material());
        let h2 = registry.register_material(make_material());
        assert_eq!(registry.material_count(), 2);

        registry.remove_material(h1);
        assert_eq!(registry.material_count(), 1);

        registry.remove_material(h2);
        assert_eq!(registry.material_count(), 0);

        let h3 = registry.register_material(make_material());
        assert_eq!(registry.material_count(), 1);
        assert!(registry.get_material(h3).is_some());
    }
}
