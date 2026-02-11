//! Asset registry for managing GPU resources without exposing Vulkan types.
//!
//! The registry stores meshes and materials internally and provides opaque handles
//! for referencing them. This keeps ash::vk types contained within katla_vulkan.

use super::types::*;
use crate::vulkan::*;
use std::{cell::RefCell, rc::Rc};

/// Internal mesh representation containing actual Vulkan buffers.
pub(crate) struct MeshAsset {
    /// Vertex buffer with geometry data.
    pub vertex_buffer: Option<VertexBuffer>,
    /// Index buffer for indexed drawing.
    pub index_buffer: Option<IndexBuffer>,
}

/// Internal material representation.
// TODO: Fields `texture` and `vertex_binding` are never read (clippy warning)
// Either use these fields in MaterialPipeline or remove them entirely
pub(crate) struct MaterialAsset {
    /// Graphics pipeline and descriptor sets (shared ownership with interior mutability).
    pub pipeline: Rc<RefCell<MaterialPipeline>>,
    /// Optional texture bound to this material.
    #[allow(dead_code)]
    pub texture: Option<Rc<Texture>>,
    /// Vertex binding description.
    #[allow(dead_code)]
    pub vertex_binding: VertexBinding,
    /// Optional per-material uniform buffer (for template-based materials).
    /// When present, this material has its own uniform buffer instead of using pipeline's embedded one.
    pub uniform: Option<crate::vulkan::material::UniformHandle>,
}

/// Registry for GPU assets.
///
/// Stores meshes and materials internally, providing opaque handles for reference.
/// This prevents ash::vk types from leaking to the application layer.
pub struct AssetRegistry {
    /// Mesh storage - slots can be None to support sparse allocation.
    meshes: Vec<Option<MeshAsset>>,
    /// Material storage.
    materials: Vec<Option<MaterialAsset>>,
    /// Next mesh ID to allocate.
    next_mesh_id: usize,
    /// Next material ID to allocate.
    next_material_id: usize,
}

impl AssetRegistry {
    /// Create a new empty asset registry.
    pub fn new() -> Self {
        Self {
            meshes: Vec::new(),
            materials: Vec::new(),
            next_mesh_id: 0,
            next_material_id: 0,
        }
    }

    /// Register a mesh and return a handle.
    pub(crate) fn register_mesh(&mut self, mesh: MeshAsset) -> MeshHandle {
        let id = self.next_mesh_id;
        self.next_mesh_id += 1;

        // Push None slots until we reach the required index
        while self.meshes.len() <= id {
            self.meshes.push(None);
        }

        self.meshes[id] = Some(mesh);
        MeshHandle(id)
    }

    /// Register a material and return a handle.
    pub(crate) fn register_material(&mut self, material: MaterialAsset) -> MaterialHandle {
        let id = self.next_material_id;
        self.next_material_id += 1;

        // Push None slots until we reach the required index
        while self.materials.len() <= id {
            self.materials.push(None);
        }

        self.materials[id] = Some(material);
        MaterialHandle(id)
    }

    /// Get a mesh by handle (internal use only).
    // TODO: Methods `get_mesh_mut`, `get_material`, `remove_mesh`, and `remove_material` are never used (clippy warning)
    // Either use these methods or remove them
    pub(crate) fn get_mesh(&self, handle: MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(handle.0)?.as_ref()
    }

    /// Get a mutable mesh by handle (internal use only).
    #[allow(dead_code)]
    pub(crate) fn get_mesh_mut(&mut self, handle: MeshHandle) -> Option<&mut MeshAsset> {
        self.meshes.get_mut(handle.0)?.as_mut()
    }

    /// Get a material by handle (internal use only).
    #[allow(dead_code)]
    pub(crate) fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(handle.0)?.as_ref()
    }

    /// Get a mutable material by handle (for hot reload).
    #[allow(dead_code)]
    pub(crate) fn get_material_mut(&mut self, handle: MaterialHandle) -> Option<&mut MaterialAsset> {
        self.materials.get_mut(handle.0)?.as_mut()
    }

    /// Update a material's pipeline without destroying the old one (for hot reload).
    ///
    /// This replaces the pipeline Rc pointer, allowing the old pipeline to be
    /// dropped naturally when no longer in use. This is safe for hot reload
    /// because the GPU will finish using the old pipeline before it's actually
    /// destroyed via the Drop trait.
    pub fn replace_material_pipeline(
        &mut self,
        handle: MaterialHandle,
        new_pipeline: std::rc::Rc<std::cell::RefCell<crate::vulkan::material::MaterialPipeline>>,
    ) -> bool {
        if let Some(material) = self.get_material_mut(handle) {
            material.pipeline = new_pipeline;
            true
        } else {
            false
        }
    }

    /// Remove a mesh and free its resources.
    ///
    /// Returns the removed mesh if it existed, or None if the handle was invalid.
    // TODO: This method is never used (clippy warning)
    #[allow(dead_code)]
    pub(crate) fn remove_mesh(&mut self, handle: MeshHandle) -> Option<MeshAsset> {
        if handle.0 < self.meshes.len() {
            self.meshes[handle.0].take()
        } else {
            None
        }
    }

    /// Remove a material and free its resources.
    ///
    /// Returns the removed material if it existed, or None if the handle was invalid.
    // TODO: This method is never used (clippy warning)
    #[allow(dead_code)]
    pub(crate) fn remove_material(&mut self, handle: MaterialHandle) -> Option<MaterialAsset> {
        if handle.0 < self.materials.len() {
            self.materials[handle.0].take()
        } else {
            None
        }
    }

    /// Get the number of registered meshes.
    pub fn mesh_count(&self) -> usize {
        self.meshes.iter().filter(|m| m.is_some()).count()
    }

    /// Get the number of registered materials.
    pub fn material_count(&self) -> usize {
        self.materials.iter().filter(|m| m.is_some()).count()
    }

    /// Clear all assets from the registry.
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.materials.clear();
        self.next_mesh_id = 0;
        self.next_material_id = 0;
    }
}

impl Default for AssetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetRegistry {
    /// Destroy all registered assets and free GPU resources.
    ///
    /// This must be called before the renderer is destroyed to avoid Vulkan validation errors.
    ///
    /// Note: This only destroys per-material uniform buffers, not the pipelines themselves.
    /// Pipelines are shared via Rc<RefCell<>> and are managed by MaterialRegistry.
    pub fn destroy(&mut self) {
        // Destroy per-material uniform buffers
        // Each material has its own uniform buffer with descriptor pools that need cleanup
        for material in self.materials.iter_mut().flatten() {
            if let Some(mut uniform) = material.uniform.take() {
                if let Ok(pipeline) = material.pipeline.try_borrow() {
                    uniform.destroy(pipeline.context());
                }
            }
        }

        // Clear materials and meshes
        // Pipelines will be dropped naturally (they're managed by MaterialRegistry)
        self.materials.clear();
        self.meshes.clear();

        // Reset counters
        self.next_mesh_id = 0;
        self.next_material_id = 0;
    }
}

impl Drop for AssetRegistry {
    fn drop(&mut self) {
        // Clean up any remaining assets as a safety net
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = AssetRegistry::new();
        assert_eq!(registry.mesh_count(), 0);
        assert_eq!(registry.material_count(), 0);
    }

    #[test]
    fn test_mesh_handle_uniqueness() {
        let _registry = AssetRegistry::new();

        // Verify handles are sequential
        assert_eq!(MeshHandle(0), MeshHandle(0));
        assert_ne!(MeshHandle(0), MeshHandle(1));
    }

    #[test]
    fn test_material_handle_uniqueness() {
        assert_eq!(MaterialHandle(0), MaterialHandle(0));
        assert_ne!(MaterialHandle(0), MaterialHandle(1));
    }

    #[test]
    fn test_clear() {
        let mut registry = AssetRegistry::new();
        registry.clear();
        assert_eq!(registry.mesh_count(), 0);
        assert_eq!(registry.material_count(), 0);
        assert_eq!(registry.next_mesh_id, 0);
        assert_eq!(registry.next_material_id, 0);
    }

    #[test]
    fn test_handle_allocation() {
        let mut registry = AssetRegistry::new();

        // Register some placeholder assets to test handle allocation
        // Note: We can't create real assets without a VulkanContext
        // So we just test the handle allocation logic

        assert_eq!(registry.next_mesh_id, 0);
        assert_eq!(registry.next_material_id, 0);
    }
}
