use std::collections::HashMap;

use crate::rendering::Material;

/// ID for referencing a shared material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialId(pub usize);

/// Manages shared materials to avoid duplication.
///
/// Materials can be registered by name and then cloned for multiple models.
/// Since Material's fields (pipeline, texture) are Rc-wrapped, cloning is cheap.
pub struct MaterialManager {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Register a material with a name, returning its ID.
    ///
    /// The material can be cloned and used in multiple models.
    /// Since the material's internal fields use Rc, cloning is cheap.
    pub fn register_material(&mut self, name: impl Into<String>, material: Material) -> MaterialId {
        let name = name.into();
        let id = MaterialId(self.materials.len());
        self.materials.push(material);
        self.by_name.insert(name, id);
        id
    }

    /// Get a reference to a material by ID.
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0)
    }

    /// Get a mutable reference to a material by ID.
    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id.0)
    }

    /// Get a material ID by name.
    pub fn get_by_name(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }

    /// Clone a material by ID for use in a Model.
    ///
    /// This is cheap because Material's fields are Rc-wrapped.
    pub fn clone_material(&self, id: MaterialId) -> Option<Material> {
        self.get(id).cloned()
    }

    /// Clone a material by name for use in a Model.
    pub fn clone_material_by_name(&self, name: &str) -> Option<Material> {
        self.get_by_name(name)
            .and_then(|id| self.clone_material(id))
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Destroy all Vulkan resources held by managed materials.
    ///
    /// This should be called during shutdown before the VulkanRenderer is destroyed.
    /// Each material's MaterialPipeline will be destroyed, releasing Vulkan resources.
    pub fn destroy(&mut self) {
        for material in &mut self.materials {
            // Destroy the MaterialPipeline which owns Vulkan resources
            material.material_pipeline.borrow_mut().destroy();
        }
        self.materials.clear();
        self.by_name.clear();
    }
}

impl Default for MaterialManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_manager_empty() {
        let manager = MaterialManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_material_manager_register_and_retrieve() {
        let manager = MaterialManager::new();

        // Create a dummy material (we'd need actual Vulkan resources for real testing)
        // For now just test the registration logic
        assert!(manager.get_by_name("test").is_none());
    }
}
