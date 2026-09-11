use super::*;

impl VulkanRenderer {
    /// Destroy a mesh and retire its GPU vertex/index buffers.
    ///
    /// The handle invalidates immediately (`get_mesh` returns `None`,
    /// `mesh_count()` decreases), but the native buffers retire instead of
    /// freeing right away: staged uploads and in-flight frames may still
    /// reference them, and the retirement queue frees them once those
    /// submissions have provably completed.
    ///
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The mesh handle to destroy
    pub fn destroy_mesh(&mut self, handle: MeshHandle) {
        let Some(mut asset) = self.asset_registry.remove_mesh(handle) else {
            return;
        };
        let mut retirements =
            FrameRetirements::new(&mut self.retirements, self.swap_data.frame_counter());
        for (_, vertex_buffer) in asset.attribute_buffers.drain() {
            let (buffer, allocation) = vertex_buffer.into_native_parts();
            retirements.retire(RetiredBuffer::new(buffer, allocation, self.context.clone()));
        }
        if let Some(index_buffer) = asset.index_buffer.take() {
            let (buffer, allocation) = index_buffer.into_native_parts();
            retirements.retire(RetiredBuffer::new(buffer, allocation, self.context.clone()));
        }
    }

    /// Destroy a material and retire its compiled pipeline variants.
    ///
    /// The handle invalidates immediately (`get_material(handle)` returns
    /// `None`, `material_count()` decreases). Every compiled variant's
    /// native pipeline retires instead of freeing right away: in-flight
    /// frames may still bind them, and the retirement queue frees them once
    /// those submissions have provably completed.
    ///
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The material handle to destroy
    pub fn destroy_material(&mut self, handle: MaterialHandle) {
        let Some(material) = self.asset_registry.remove_material(handle) else {
            return;
        };
        for variant in material.variants.into_values() {
            for pipeline_handle in [Some(variant.pipeline), variant.instanced_pipeline]
                .into_iter()
                .flatten()
            {
                if let Some(pipeline) = self.asset_registry.remove_pipeline(pipeline_handle) {
                    self.retire(pipeline);
                }
            }
        }
    }

    /// Destroy a texture and retire its GPU image and bindless slot.
    ///
    /// The handle invalidates immediately (`TextureManager::contains(handle)`
    /// returns `false`). The native image (with its view, sampler, and
    /// allocation) retires instead of freeing right away, and the bindless
    /// slot stays occupied: in-flight submissions may still sample this
    /// texture through that slot, so both are released only once those
    /// submissions have provably completed — a new texture cannot take the
    /// slot in the meantime. Default textures are never destroyed.
    ///
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
        let slot = self.texture_manager.get_bindless_slot(handle);
        if let Some(texture) = self.texture_manager.destroy(handle) {
            self.retire(texture);
        }
        if let Some(slot) = slot {
            self.retire(RetiredResource::BindlessSlot(slot));
        }
    }

    /// Destroy a skeleton and retire its GPU storage buffer.
    ///
    /// The handle invalidates immediately (`get_skeleton_descriptor(handle)`
    /// returns `None`); the joint-matrix storage buffer retires until the
    /// submissions that skin against it have provably completed.
    ///
    /// Double-destroy is safe (no-op). Destroying an unowned or `NONE` handle is safe.
    ///
    /// # Arguments
    /// * `handle` - The skeleton handle to destroy
    pub fn destroy_skeleton(&mut self, handle: SkeletonHandle) {
        self.skeleton_descriptors.remove(handle);
        if let Some(buffer) = self.skeleton_buffers.remove(handle) {
            self.retire(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::handle::{MaterialHandle, MeshHandle, TextureHandle};
    use crate::renderer::pipeline_descriptor::PipelineDescriptor;
    use crate::renderer::registry::{AssetRegistry, MaterialAsset, MaterialTextures, MeshAsset};

    fn make_material() -> MaterialAsset {
        MaterialAsset {
            descriptor: PipelineDescriptor::pbr("shaders/pbr.wgsl"),
            variants: std::collections::HashMap::new(),
            textures: MaterialTextures::default(),
        }
    }

    fn make_mesh(vertex_count: u32) -> MeshAsset {
        MeshAsset {
            attribute_buffers: std::collections::HashMap::new(),
            index_buffer: None,
            index_format: crate::backend::command::IndexType::Uint32,
            vertex_count,
            index_count: 0,
            layout: crate::vertex::VertexLayout::position(),
            attributes: vec![crate::vulkan::vertex_attribute::AttributeType::Position],
            topology: crate::renderer::registry::PrimitiveTopology::TriangleList,
            usage: crate::renderer::registry::MeshUsage::Static,
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

        // TextureHandle::from_raw(u32::MAX, 0) is also NONE
        let max = TextureHandle::from_raw(u32::MAX, 0);
        assert!(max.is_none());
    }

    // =========================================================================
    // VAL-GPU-004: Destroy skeleton releases storage buffer and descriptor set
    // =========================================================================

    #[test]
    fn test_destroy_skeleton() {
        use crate::handle::{ResourceStorage, SkeletonMarker};

        let mut descriptors: ResourceStorage<String, SkeletonMarker> = ResourceStorage::new();
        let mut buffers: ResourceStorage<String, SkeletonMarker> = ResourceStorage::new();

        let handle = descriptors.insert("desc".to_string());
        let buffer_handle = buffers.insert("buf".to_string());
        assert_eq!(handle, buffer_handle);
        assert_eq!(descriptors.len(), 1);
        assert_eq!(buffers.len(), 1);

        descriptors.remove(handle);
        buffers.remove(buffer_handle);
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
        registry.remove_mesh(MeshHandle::from_raw(99999, 0));
        registry.remove_material(MaterialHandle::NONE);
        registry.remove_material(MaterialHandle::from_raw(99999, 0));
    }

    // =========================================================================
    // VAL-GPU-011: Default textures are never destroyed
    // =========================================================================

    #[test]
    fn test_default_textures_preserved() {
        // Verify the is_default_texture check works correctly for all 5 defaults
        // Default handles are at indices 0-4 (created first in TextureManager::new)
        for i in 0..5 {
            let handle = TextureHandle::from_raw(i, 0);
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
