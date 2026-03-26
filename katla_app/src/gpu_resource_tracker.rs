//! GPU resource reference tracking for automatic cleanup on entity/component destruction.
//!
//! Manages reference counts for GPU resources (meshes, materials, textures, skeletons)
//! so that shared resources are only destroyed when no entity references them anymore.
//! This prevents use-after-free when multiple entities share the same mesh or material.

use std::collections::HashMap;

use katla_gfx::{MaterialHandle, MeshHandle, SkeletonHandle, TextureHandle};

/// Tracks GPU resource reference counts for safe cleanup.
///
/// Each resource type (mesh, material, texture, skeleton) has an independent
/// reference count. Resources are only destroyed when their ref count drops to zero.
/// The default material is protected and never destroyed.
pub struct GpuResourceTracker {
    mesh_refs: HashMap<u32, u32>,
    material_refs: HashMap<u32, u32>,
    texture_refs: HashMap<u32, u32>,
    skeleton_refs: HashMap<u32, u32>,
    /// Material handle that must never be destroyed (default PBR material).
    protected_material: MaterialHandle,
}

impl GpuResourceTracker {
    /// Create a new tracker with the given protected material handle.
    ///
    /// The protected material (typically the default PBR material) will never
    /// be destroyed even if its ref count drops to zero.
    pub fn new(protected_material: MaterialHandle) -> Self {
        Self {
            mesh_refs: HashMap::new(),
            material_refs: HashMap::new(),
            texture_refs: HashMap::new(),
            skeleton_refs: HashMap::new(),
            protected_material,
        }
    }

    /// Update the protected material handle.
    ///
    /// Used when the protected material isn't known at tracker creation time
    /// (e.g., it's compiled during `Application::init()`).
    pub fn set_protected_material(&mut self, material: MaterialHandle) {
        self.protected_material = material;
    }

    /// Track all GPU resources referenced by a `DrawableComponent`.
    ///
    /// Increments ref counts for the component's mesh, material, and skeleton.
    /// This should be called whenever a `DrawableComponent` is added to an entity.
    pub fn track_drawable(
        &mut self,
        mesh: MeshHandle,
        material: MaterialHandle,
        skeleton: SkeletonHandle,
    ) {
        *self.mesh_refs.entry(mesh.index()).or_insert(0) += 1;
        *self.material_refs.entry(material.index()).or_insert(0) += 1;
        if !skeleton.is_none() {
            *self.skeleton_refs.entry(skeleton.index()).or_insert(0) += 1;
        }
    }

    /// Track a texture handle (increment ref count).
    pub fn track_texture(&mut self, texture: TextureHandle) {
        if !texture.is_none() {
            *self.texture_refs.entry(texture.index()).or_insert(0) += 1;
        }
    }

    /// Release all GPU resources referenced by a `DrawableComponent`.
    ///
    /// Decrements ref counts and returns the handles that should be destroyed
    /// (i.e., those whose ref count dropped to zero).
    /// The protected material is never included in the destroy list.
    pub fn release_drawable(
        &mut self,
        mesh: MeshHandle,
        material: MaterialHandle,
        skeleton: SkeletonHandle,
    ) -> GpuResourcesToDestroy {
        let mut to_destroy = GpuResourcesToDestroy::default();

        if Self::release_ref(&mut self.mesh_refs, mesh.index()) {
            to_destroy.meshes.push(mesh);
        }

        let is_protected = material == self.protected_material || material.is_none();
        if !is_protected && Self::release_ref(&mut self.material_refs, material.index()) {
            to_destroy.materials.push(material);
        }

        if !skeleton.is_none() && Self::release_ref(&mut self.skeleton_refs, skeleton.index()) {
            to_destroy.skeletons.push(skeleton);
        }

        to_destroy
    }

    /// Release a texture handle (decrement ref count, return if should destroy).
    pub fn release_texture(&mut self, texture: TextureHandle) -> bool {
        if texture.is_none() {
            return false;
        }
        Self::release_ref(&mut self.texture_refs, texture.index())
    }

    /// Release all tracked resources, returning all handles that should be destroyed.
    ///
    /// Used during scene load to clean up all GPU resources before clearing entities.
    pub fn release_all(&mut self) -> GpuResourcesToDestroy {
        let mut to_destroy = GpuResourcesToDestroy::default();

        for &idx in self.mesh_refs.keys() {
            to_destroy.meshes.push(MeshHandle::new(idx));
        }

        for &idx in self.material_refs.keys() {
            let handle = MaterialHandle::new(idx);
            if handle != self.protected_material {
                to_destroy.materials.push(handle);
            }
        }

        for &idx in self.texture_refs.keys() {
            to_destroy.textures.push(TextureHandle::new(idx));
        }

        for &idx in self.skeleton_refs.keys() {
            to_destroy.skeletons.push(SkeletonHandle::new(idx));
        }

        self.mesh_refs.clear();
        self.material_refs.clear();
        self.texture_refs.clear();
        self.skeleton_refs.clear();

        to_destroy
    }

    /// Get the number of tracked mesh references.
    pub fn mesh_count(&self) -> usize {
        self.mesh_refs.len()
    }

    /// Get the number of tracked material references.
    pub fn material_count(&self) -> usize {
        self.material_refs.len()
    }

    /// Get the reference count for a specific mesh.
    pub fn mesh_ref_count(&self, handle: MeshHandle) -> u32 {
        *self.mesh_refs.get(&handle.index()).unwrap_or(&0)
    }

    /// Get the reference count for a specific material.
    pub fn material_ref_count(&self, handle: MaterialHandle) -> u32 {
        *self.material_refs.get(&handle.index()).unwrap_or(&0)
    }

    fn release_ref(refs: &mut HashMap<u32, u32>, idx: u32) -> bool {
        if let Some(count) = refs.get_mut(&idx) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                refs.remove(&idx);
                return true;
            }
        }
        false
    }
}

/// GPU resource handles that should be destroyed.
///
/// Collected by `GpuResourceTracker::release_drawable` or `release_all`.
/// The caller (typically the application or scene loader) is responsible for
/// calling the appropriate `VulkanRenderer::destroy_*` methods.
#[derive(Default)]
pub struct GpuResourcesToDestroy {
    pub meshes: Vec<MeshHandle>,
    pub materials: Vec<MaterialHandle>,
    pub textures: Vec<TextureHandle>,
    pub skeletons: Vec<SkeletonHandle>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protected_mat() -> MaterialHandle {
        MaterialHandle::new(42)
    }

    #[test]
    fn test_track_and_release_single() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);
        let skel = SkeletonHandle::new(3);

        tracker.track_drawable(mesh, mat, skel);

        assert_eq!(tracker.mesh_ref_count(mesh), 1);
        assert_eq!(tracker.material_ref_count(mat), 1);
        assert_eq!(tracker.mesh_count(), 1);

        let to_destroy = tracker.release_drawable(mesh, mat, skel);

        assert_eq!(to_destroy.meshes.len(), 1);
        assert_eq!(to_destroy.materials.len(), 1);
        assert_eq!(to_destroy.skeletons.len(), 1);
        assert_eq!(tracker.mesh_count(), 0);
    }

    #[test]
    fn test_shared_resources_not_destroyed_by_single_release() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);

        // Two entities share the same mesh and material
        tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);
        tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);

        assert_eq!(tracker.mesh_ref_count(mesh), 2);
        assert_eq!(tracker.material_ref_count(mat), 2);

        // First release should NOT destroy shared resources
        let to_destroy = tracker.release_drawable(mesh, mat, SkeletonHandle::NONE);
        assert!(
            to_destroy.meshes.is_empty(),
            "Shared mesh should not be destroyed"
        );
        assert!(
            to_destroy.materials.is_empty(),
            "Shared material should not be destroyed"
        );
        assert_eq!(tracker.mesh_ref_count(mesh), 1);

        // Second release should destroy
        let to_destroy = tracker.release_drawable(mesh, mat, SkeletonHandle::NONE);
        assert_eq!(to_destroy.meshes.len(), 1);
        assert_eq!(to_destroy.materials.len(), 1);
        assert_eq!(tracker.mesh_count(), 0);
    }

    #[test]
    fn test_protected_material_never_destroyed() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        tracker.track_drawable(MeshHandle::new(1), protected_mat(), SkeletonHandle::NONE);
        let to_destroy =
            tracker.release_drawable(MeshHandle::new(1), protected_mat(), SkeletonHandle::NONE);

        assert_eq!(to_destroy.meshes.len(), 1);
        assert!(
            to_destroy.materials.is_empty(),
            "Protected material should never be destroyed"
        );
    }

    #[test]
    fn test_release_all_returns_all_resources() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        tracker.track_drawable(
            MeshHandle::new(1),
            MaterialHandle::new(2),
            SkeletonHandle::new(3),
        );
        tracker.track_drawable(
            MeshHandle::new(4),
            MaterialHandle::new(5),
            SkeletonHandle::NONE,
        );
        tracker.track_texture(TextureHandle::new(10));

        let to_destroy = tracker.release_all();

        assert_eq!(to_destroy.meshes.len(), 2);
        assert_eq!(to_destroy.materials.len(), 2);
        assert_eq!(to_destroy.textures.len(), 1);
        assert_eq!(to_destroy.skeletons.len(), 1);
        assert_eq!(tracker.mesh_count(), 0);
        assert_eq!(tracker.material_count(), 0);
    }

    #[test]
    fn test_release_all_excludes_protected_material() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        tracker.track_drawable(MeshHandle::new(1), protected_mat(), SkeletonHandle::NONE);
        tracker.track_drawable(
            MeshHandle::new(2),
            MaterialHandle::new(99),
            SkeletonHandle::NONE,
        );

        let to_destroy = tracker.release_all();

        // Protected material (42) should NOT be in the destroy list
        assert!(!to_destroy.materials.iter().any(|m| m.index() == 42));
        assert!(to_destroy.materials.iter().any(|m| m.index() == 99));
    }

    #[test]
    fn test_skeleton_none_not_tracked() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        tracker.track_drawable(
            MeshHandle::new(1),
            MaterialHandle::new(2),
            SkeletonHandle::NONE,
        );
        let to_destroy = tracker.release_drawable(
            MeshHandle::new(1),
            MaterialHandle::new(2),
            SkeletonHandle::NONE,
        );

        assert!(to_destroy.skeletons.is_empty());
    }

    #[test]
    fn test_double_release_safe() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        let mesh = MeshHandle::new(1);
        let mat = MaterialHandle::new(2);

        tracker.track_drawable(mesh, mat, SkeletonHandle::NONE);
        let _ = tracker.release_drawable(mesh, mat, SkeletonHandle::NONE);

        // Second release should be safe (no panic, no entries)
        let to_destroy = tracker.release_drawable(mesh, mat, SkeletonHandle::NONE);
        assert!(to_destroy.meshes.is_empty());
        assert!(to_destroy.materials.is_empty());
    }

    #[test]
    fn test_create_destroy_sequence_counts() {
        let mut tracker = GpuResourceTracker::new(protected_mat());

        let m1 = MeshHandle::new(1);
        let m2 = MeshHandle::new(2);
        let m3 = MeshHandle::new(3);
        let mat = MaterialHandle::new(10);

        tracker.track_drawable(m1, mat, SkeletonHandle::NONE);
        tracker.track_drawable(m2, mat, SkeletonHandle::NONE);
        tracker.track_drawable(m3, mat, SkeletonHandle::NONE);
        assert_eq!(tracker.mesh_count(), 3);

        let d1 = tracker.release_drawable(m2, mat, SkeletonHandle::NONE);
        assert_eq!(d1.meshes.len(), 1, "m2 mesh should be destroyed");
        assert!(d1.materials.is_empty(), "Shared mat still held by m1, m3");
        assert_eq!(tracker.mesh_count(), 2);

        let d2 = tracker.release_drawable(m1, mat, SkeletonHandle::NONE);
        assert_eq!(d2.meshes.len(), 1, "m1 mesh should be destroyed");
        assert!(d2.materials.is_empty(), "Shared mat still held by m3");
        assert_eq!(tracker.mesh_count(), 1);

        let d3 = tracker.release_drawable(m3, mat, SkeletonHandle::NONE);
        assert_eq!(d3.meshes.len(), 1);
        assert_eq!(d3.materials.len(), 1);
        assert_eq!(tracker.mesh_count(), 0);
    }
}
