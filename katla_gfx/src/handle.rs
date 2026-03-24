//! Unified opaque handle system for GPU resources.
//!
//! This module provides a single generic `Handle<T>` type that is used throughout
//! katla_gfx for referencing GPU resources. Handles are:
//!
//! - **Copy types**: Just a `u32` index, cheap to pass around
//! - **Type-safe**: Uses phantom types to prevent mixing different handle types
//! - **Opaque**: No direct access to underlying Vulkan types from outside the crate
//!
//! # Handle Categories
//!
//! ## Public Handles (Application Layer)
//! - `MeshHandle`, `MaterialHandle`, `TextureHandle`, `SkeletonHandle`
//!
//! These are exposed to katla_app and represent high-level resources.
//!
//! ## Internal Handles (Render Layer)
//! - `BufferHandle`, `ImageHandle`, `PipelineHandle`, etc.
//!
//! These are `pub(crate)` and only used internally within katla_gfx.

use std::marker::PhantomData;

// Generic Handle Type

/// Opaque handle to a GPU resource.
///
/// - Copy type (just a u32 index)
/// - Type-safe via phantom types
/// - No access to underlying Vulkan types
/// - Resources accessed through storage types only
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    index: u32,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    pub const NONE: Self = Self {
        index: u32::MAX,
        _marker: PhantomData,
    };

    pub fn is_none(&self) -> bool {
        self.index == u32::MAX
    }

    pub fn is_some(&self) -> bool {
        self.index != u32::MAX
    }

    pub fn new(index: u32) -> Self {
        Self {
            index,
            _marker: PhantomData,
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }
}

impl<T> Default for Handle<T> {
    fn default() -> Self {
        Self::NONE
    }
}

// Public Marker Types (Application Layer)

/// Marker type for mesh handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshMarker;

/// Marker type for material handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialMarker;

/// Marker type for texture handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureMarker;

/// Marker type for skeleton handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkeletonMarker;

/// Marker type for particle emitter handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmitterMarker;

// Public Handle Type Aliases (Application Layer)

/// Handle to a mesh resource.
pub type MeshHandle = Handle<MeshMarker>;

/// Handle to a material resource.
pub type MaterialHandle = Handle<MaterialMarker>;

/// Handle to a texture resource.
pub type TextureHandle = Handle<TextureMarker>;

/// Handle to a skeleton resource.
pub type SkeletonHandle = Handle<SkeletonMarker>;

/// Handle to a particle emitter resource.
pub type EmitterHandle = Handle<EmitterMarker>;

// Internal Marker Types (Render Layer)

/// Marker type for buffer handles.
#[derive(Debug, Clone, Copy)]
pub struct BufferMarker;

/// Marker type for image handles.
#[derive(Debug, Clone, Copy)]
pub struct ImageMarker;

/// Marker type for pipeline handles.
#[derive(Debug, Clone, Copy)]
pub struct PipelineMarker;

/// Marker type for pipeline layout handles.
#[derive(Debug, Clone, Copy)]
pub struct PipelineLayoutMarker;

/// Marker type for descriptor set handles.
#[derive(Debug, Clone, Copy)]
pub struct DescriptorSetMarker;

// Internal Handle Type Aliases (Render Layer)

/// Handle to a buffer resource.
pub type BufferHandle = Handle<BufferMarker>;

/// Handle to an image resource.
pub type ImageHandle = Handle<ImageMarker>;

//=============================================================================
// Public Handle Type Aliases (Used by katla_app)
//=============================================================================

/// Handle to a pipeline resource.
pub type PipelineHandle = Handle<PipelineMarker>;

/// Handle to a pipeline layout resource.
pub type PipelineLayoutHandle = Handle<PipelineLayoutMarker>;

/// Handle to a descriptor set resource.
pub type DescriptorSetHandle = Handle<DescriptorSetMarker>;

// Resource Storage

/// Central storage for GPU resources.
///
/// Provides storage and lookup for resources by handle.
/// Resources are stored in a sparse Vec to support deletion.
pub(crate) struct ResourceStorage<T> {
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

    pub fn get(&self, handle: u32) -> Option<&T> {
        self.resources.get(handle as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, handle: u32) -> Option<&mut T> {
        self.resources.get_mut(handle as usize)?.as_mut()
    }

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

    pub fn contains(&self, handle: u32) -> bool {
        self.resources
            .get(handle as usize)
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.resources.iter().filter(|slot| slot.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.resources.iter().filter_map(|slot| slot.as_ref())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.resources.iter_mut().filter_map(|slot| slot.as_mut())
    }
}

impl<T> Default for ResourceStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_none() {
        let handle: MeshHandle = Handle::NONE;
        assert!(handle.is_none());
        assert!(!handle.is_some());
    }

    #[test]
    fn test_handle_new() {
        let handle: MeshHandle = Handle::new(42);
        assert!(!handle.is_none());
        assert!(handle.is_some());
        assert_eq!(handle.index(), 42);
    }

    #[test]
    fn test_handle_default() {
        let handle: MaterialHandle = MaterialHandle::default();
        assert!(handle.is_none());
    }

    #[test]
    fn test_handle_copy_clone() {
        let handle: TextureHandle = Handle::new(10);
        let copied = handle;
        let cloned = handle;
        assert_eq!(handle, copied);
        assert_eq!(handle, cloned);
    }

    #[test]
    fn test_handle_eq_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let handle: SkeletonHandle = Handle::new(5);
        set.insert(handle);
        assert!(set.contains(&handle));
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
    fn test_resource_storage_iter() {
        let mut storage = ResourceStorage::new();
        storage.insert(1);
        storage.insert(2);
        storage.insert(3);

        let sum: i32 = storage.iter().sum();
        assert_eq!(sum, 6);
    }
}
