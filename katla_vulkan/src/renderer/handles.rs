//! Opaque resource handles for GPU resources.
//!
//! This module provides opaque handle types that reference resources stored
//! in central storage. This replaces `Rc<RefCell<...>>` patterns with simple
//! indices, making the API cleaner and enabling `Send + Sync`.
//!
//! # Design
//!
//! - Handles are Copy types (just indices)
//! - Resources are stored in central `ResourceStorage<T>`
//! - Storage is owned by `VulkanRenderer`
//! - No runtime borrow checking needed

use std::marker::PhantomData;

/// Opaque handle to a pipeline resource.
///
/// This replaces `Rc<RefCell<MaterialPipeline>>` with a simple index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PipelineHandle(pub u32);

impl PipelineHandle {
    pub const NONE: PipelineHandle = PipelineHandle(u32::MAX);

    pub fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    pub fn is_some(self) -> bool {
        self.0 != u32::MAX
    }
}

impl Default for PipelineHandle {
    fn default() -> Self {
        Self::NONE
    }
}

/// Opaque handle to a texture resource.
///
/// Textures are stored centrally and referenced by index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

impl TextureHandle {
    pub const NONE: TextureHandle = TextureHandle(u32::MAX);

    pub fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    pub fn is_some(self) -> bool {
        self.0 != u32::MAX
    }
}

impl Default for TextureHandle {
    fn default() -> Self {
        Self::NONE
    }
}

/// Central storage for GPU resources.
///
/// Provides storage and lookup for resources by handle.
/// Resources are stored in a sparse Vec to support deletion.
pub struct ResourceStorage<T> {
    resources: Vec<Option<T>>,
    free_indices: Vec<u32>,
    _marker: PhantomData<T>,
}

impl<T> ResourceStorage<T> {
    pub fn new() -> Self {
        Self {
            resources: Vec::new(),
            free_indices: Vec::new(),
            _marker: PhantomData,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            resources: Vec::with_capacity(capacity),
            free_indices: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Insert a resource and return its handle.
    pub fn insert(&mut self, resource: T) -> u32 {
        if let Some(index) = self.free_indices.pop() {
            self.resources[index as usize] = Some(resource);
            index
        } else {
            let index = self.resources.len() as u32;
            self.resources.push(Some(resource));
            index
        }
    }

    /// Get a resource by handle.
    pub fn get(&self, handle: u32) -> Option<&T> {
        self.resources.get(handle as usize)?.as_ref()
    }

    /// Get a mutable reference to a resource by handle.
    pub fn get_mut(&mut self, handle: u32) -> Option<&mut T> {
        self.resources.get_mut(handle as usize)?.as_mut()
    }

    /// Remove a resource by handle.
    pub fn remove(&mut self, handle: u32) -> Option<T> {
        if let Some(slot) = self.resources.get_mut(handle as usize) {
            let resource = slot.take();
            if resource.is_some() {
                self.free_indices.push(handle);
            }
            resource
        } else {
            None
        }
    }

    /// Check if a handle is valid.
    pub fn contains(&self, handle: u32) -> bool {
        self.resources
            .get(handle as usize)
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// Get the number of stored resources.
    pub fn len(&self) -> usize {
        self.resources.iter().filter(|slot| slot.is_some()).count()
    }

    /// Check if storage is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all resources.
    pub fn clear(&mut self) {
        self.resources.clear();
        self.free_indices.clear();
    }

    /// Iterate over all stored resources.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.resources.iter().filter_map(|slot| slot.as_ref())
    }

    /// Iterate mutably over all stored resources.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.resources.iter_mut().filter_map(|slot| slot.as_mut())
    }
}

impl<T> Default for ResourceStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_handle() {
        let handle = PipelineHandle(42);
        assert_eq!(handle.0, 42);
        assert!(handle.is_some());
        assert!(!handle.is_none());

        let none = PipelineHandle::NONE;
        assert!(none.is_none());
        assert!(!none.is_some());
    }

    #[test]
    fn test_resource_storage() {
        let mut storage = ResourceStorage::new();

        let h1 = storage.insert("first");
        let h2 = storage.insert("second");
        let h3 = storage.insert("third");

        assert_eq!(storage.get(h1), Some(&"first"));
        assert_eq!(storage.get(h2), Some(&"second"));
        assert_eq!(storage.get(h3), Some(&"third"));
        assert_eq!(storage.len(), 3);

        storage.remove(h2);
        assert_eq!(storage.get(h2), None);
        assert_eq!(storage.len(), 2);

        let h4 = storage.insert("fourth");
        assert_eq!(h4, h2);
        assert_eq!(storage.get(h4), Some(&"fourth"));
        assert_eq!(storage.len(), 3);
    }

    #[test]
    fn test_resource_storage_clear() {
        let mut storage = ResourceStorage::new();
        storage.insert(1);
        storage.insert(2);
        storage.insert(3);

        assert_eq!(storage.len(), 3);

        storage.clear();
        assert_eq!(storage.len(), 0);
        assert!(storage.is_empty());
    }
}
