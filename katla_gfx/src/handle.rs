//! Unified opaque handle system for GPU resources.
//!
//! This module provides a single generic `Handle<T>` type that is used throughout
//! katla_gfx for referencing GPU resources. Handles are:
//!
//! - **Copy types**: An index and a generation, cheap to pass around
//! - **Type-safe**: Uses phantom types to prevent mixing different handle types
//! - **Generational**: Destroying a resource permanently invalidates every
//!   handle to it, even after the slot is reused for a new resource
//! - **Opaque**: No direct access to underlying Vulkan types from outside the crate
//!
//! # Identity model
//!
//! A handle identifies `(slot, generation)`. Storage assigns slot indices with
//! reuse, but every removal bumps the slot's generation, so a stale handle can
//! never alias the resource that later occupies the same slot. Bindless
//! descriptor/table indices are a separate GPU-side addressing scheme and are
//! never used as CPU resource identity.
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
/// - Copy type (slot index + generation)
/// - Type-safe via phantom types
/// - No access to underlying Vulkan types
/// - Resources accessed through storage types only
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    index: u32,
    generation: u32,
    _marker: PhantomData<fn() -> T>,
}

// Manual impls: `PhantomData<fn() -> T>` is copyable for every `T`, so
// handles are copy values regardless of the marker type.
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Handle<T> {
    pub const NONE: Self = Self {
        index: u32::MAX,
        generation: u32::MAX,
        _marker: PhantomData,
    };

    pub fn is_none(&self) -> bool {
        self.index == u32::MAX
    }

    pub fn is_some(&self) -> bool {
        self.index != u32::MAX
    }

    /// Fabricate a handle from raw parts.
    ///
    /// The only way to construct a handle outside resource creation. Storages
    /// validate both the slot and the generation on lookup, so a fabricated
    /// handle simply fails validation unless it matches a live resource.
    /// Intended for tests and diagnostics.
    pub fn from_raw(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation,
            _marker: PhantomData,
        }
    }

    /// Slot index this handle addresses. Diagnostic use only; identity is
    /// `(index, generation)` and lookups must go through the owning storage.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Generation of the slot this handle was issued against.
    pub fn generation(&self) -> u32 {
        self.generation
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
/// Provides storage and lookup for resources by generational handle. Slots are
/// reused after removal, but each removal bumps the slot's generation, so
/// handles issued before the removal can never resolve to the slot's new
/// occupant. The generation counter skips zero on wrap-around, so a slot that
/// was ever removed never re-issues its initial generation.
pub(crate) struct ResourceStorage<T, M> {
    resources: Vec<Option<T>>,
    generations: Vec<u32>,
    free_indices: Vec<u32>,
    _marker: PhantomData<fn() -> M>,
}

impl<T, M> ResourceStorage<T, M> {
    pub fn new() -> Self {
        Self {
            resources: Vec::new(),
            generations: Vec::new(),
            free_indices: Vec::new(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn insert(&mut self, resource: T) -> Handle<M> {
        if let Some(index) = self.free_indices.pop() {
            self.resources[index as usize] = Some(resource);
            Handle::from_raw(index, self.generations[index as usize])
        } else {
            let index = self.resources.len() as u32;
            self.resources.push(Some(resource));
            self.generations.push(0);
            Handle::from_raw(index, 0)
        }
    }

    #[inline]
    pub fn get(&self, handle: Handle<M>) -> Option<&T> {
        let slot = self.live_slot(handle)?;
        self.resources[slot].as_ref()
    }

    #[inline]
    pub fn get_mut(&mut self, handle: Handle<M>) -> Option<&mut T> {
        let slot = self.live_slot(handle)?;
        self.resources[slot].as_mut()
    }

    pub fn remove(&mut self, handle: Handle<M>) -> Option<T> {
        let slot = self.live_slot(handle)?;
        let resource = self.resources[slot].take()?;
        // Advance the generation so handles issued for the removed resource
        // stay invalid even after the slot is reused.
        let generation = self.generations[slot].wrapping_add(1);
        self.generations[slot] = if generation == 0 { 1 } else { generation };
        self.free_indices.push(handle.index());
        Some(resource)
    }

    pub fn contains(&self, handle: Handle<M>) -> bool {
        self.live_slot(handle)
            .map(|slot| self.resources[slot].is_some())
            .unwrap_or(false)
    }

    /// Resolve a handle to a live slot index, rejecting out-of-range slots,
    /// `NONE` handles, and wrong generations.
    #[inline]
    fn live_slot(&self, handle: Handle<M>) -> Option<usize> {
        let slot = handle.index() as usize;
        if handle.is_none()
            || slot >= self.resources.len()
            || self.generations[slot] != handle.generation()
        {
            return None;
        }
        Some(slot)
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

    /// Iterate live resources with their valid handles.
    pub fn iter_enumerated(&self) -> impl Iterator<Item = (Handle<M>, &T)> {
        self.resources
            .iter()
            .enumerate()
            .filter_map(|(slot, resource)| {
                resource
                    .as_ref()
                    .map(|value| (Handle::from_raw(slot as u32, self.generations[slot]), value))
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.resources.iter_mut().filter_map(|slot| slot.as_mut())
    }
}

impl<T, M> Default for ResourceStorage<T, M> {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    type TestStorage = ResourceStorage<&'static str, MeshMarker>;
    type TestHandle = MeshHandle;

    #[test]
    fn test_handle_none() {
        let handle: TestHandle = Handle::NONE;
        assert!(handle.is_none());
        assert!(!handle.is_some());
    }

    #[test]
    fn test_handle_from_raw() {
        let handle: TestHandle = Handle::from_raw(42, 7);
        assert!(!handle.is_none());
        assert!(handle.is_some());
        assert_eq!(handle.index(), 42);
        assert_eq!(handle.generation(), 7);
    }

    #[test]
    fn test_handle_copy_clone() {
        let handle: TextureHandle = Handle::from_raw(10, 0);
        let copied = handle;
        let cloned = handle;
        assert_eq!(handle, copied);
        assert_eq!(handle, cloned);
    }

    #[test]
    fn test_handle_eq_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let handle: SkeletonHandle = Handle::from_raw(5, 0);
        set.insert(handle);
        assert!(set.contains(&handle));
    }

    #[test]
    fn test_same_index_different_generation_is_not_equal() {
        let first: TestHandle = Handle::from_raw(3, 0);
        let second: TestHandle = Handle::from_raw(3, 1);
        assert_ne!(first, second);
    }

    #[test]
    fn test_resource_storage() {
        let mut storage = TestStorage::new();

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
        assert_eq!(h4.index(), h2.index());
        assert_ne!(h4, h2, "reused slot must issue a new-generation handle");
        assert_eq!(storage.get(h4), Some(&"fourth"));
        assert_eq!(
            storage.get(h2),
            None,
            "stale handle must not alias the new resource"
        );
        assert_eq!(storage.len(), 3);
    }

    #[test]
    fn test_resource_storage_iter() {
        let mut storage = ResourceStorage::<i32, TextureMarker>::new();
        storage.insert(1);
        storage.insert(2);
        storage.insert(3);

        let sum: i32 = storage.iter().sum();
        assert_eq!(sum, 6);
    }

    #[test]
    fn test_stale_handle_rejected_after_slot_reuse() {
        let mut storage = TestStorage::new();
        let first = storage.insert("first");
        assert_eq!(storage.remove(first), Some("first"));

        let second = storage.insert("second");
        assert_eq!(second.index(), first.index());

        // Every stale-handle operation rejects instead of aliasing.
        assert_eq!(storage.get(first), None);
        assert!(storage.get_mut(first).is_none());
        assert!(!storage.contains(first));
        assert_eq!(storage.remove(first), None);
        assert_eq!(storage.get(second), Some(&"second"), "replacement survives");
    }

    #[test]
    fn test_double_remove_is_harmless() {
        let mut storage = TestStorage::new();
        let handle = storage.insert("value");
        assert_eq!(storage.remove(handle), Some("value"));
        assert_eq!(storage.remove(handle), None, "second remove is a no-op");

        let replacement = storage.insert("replacement");
        // The stale double-remove must not have destroyed the replacement.
        assert_eq!(storage.remove(handle), None);
        assert_eq!(storage.get(replacement), Some(&"replacement"));
    }

    #[test]
    fn test_repeated_reuse_cycles_generations() {
        let mut storage = TestStorage::new();
        let mut previous = storage.insert("v0");
        let mut handles = vec![previous];
        for _ in 1..=8 {
            assert!(storage.remove(previous).is_some());
            let handle = storage.insert("v");
            assert_eq!(handle.index(), previous.index());
            assert_eq!(
                handle.generation(),
                previous.generation() + 1,
                "each reuse cycle bumps the generation by one"
            );
            assert!(storage.get(previous).is_none());
            handles.push(handle);
            previous = handle;
        }
        // Only the newest generation is live.
        for (age, handle) in handles.iter().rev().enumerate() {
            assert_eq!(storage.contains(*handle), age == 0);
        }
    }

    #[test]
    fn test_iter_enumerated_yields_live_handles() {
        let mut storage = TestStorage::new();
        let h1 = storage.insert("first");
        let h2 = storage.insert("second");
        storage.remove(h1);
        let h3 = storage.insert("third");

        let live: Vec<_> = storage.iter_enumerated().collect();
        assert_eq!(live.len(), 2);
        assert!(live.iter().any(|(handle, _)| *handle == h2));
        assert!(live.iter().any(|(handle, _)| *handle == h3));
        assert!(
            !live.iter().any(|(handle, _)| *handle == h1),
            "removed handle must not be re-derived by iteration"
        );
        // Every yielded handle must resolve back to its resource.
        for (handle, value) in live {
            assert_eq!(storage.get(handle), Some(value));
        }
    }

    #[test]
    fn test_out_of_range_and_none_handles_rejected() {
        let mut storage = TestStorage::new();
        storage.insert("only");
        let none: TestHandle = Handle::NONE;
        let beyond: TestHandle = Handle::from_raw(99, 0);
        assert_eq!(storage.get(none), None);
        assert!(storage.remove(none).is_none());
        assert_eq!(storage.get(beyond), None);
        assert!(storage.remove(beyond).is_none());
    }
}
