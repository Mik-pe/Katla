//! Viewport camera component for linking cameras to viewport slots.
//!
//! This module provides components and systems for managing cameras
//! that are associated with specific viewport slots in the multi-viewport grid.

use katla_ecs::Component;
use katla_math::{Mat4, Vec3};

/// Component that links a camera entity to a specific viewport slot.
///
/// Each viewport in the grid (0-3) can have its own camera.
/// The camera's view matrix is computed from its Transform.
#[derive(Component, Debug)]
pub struct ViewportCamera {
    /// The viewport slot this camera is assigned to (0-3).
    /// 0 = top-left, 1 = top-right, 2 = bottom-left, 3 = bottom-right.
    pub viewport_slot: usize,

    /// Camera position in world space.
    pub position: [f32; 3],

    /// Camera target (look-at point) in world space.
    pub target: [f32; 3],

    /// Up vector.
    pub up: [f32; 3],

    /// Field of view in degrees.
    pub fov_degrees: f32,

    /// Near plane distance.
    pub near: f32,

    /// Aspect ratio (width / height).
    pub aspect_ratio: f32,
}

impl Default for ViewportCamera {
    fn default() -> Self {
        Self {
            viewport_slot: 0,
            position: [0.0, 5.0, 10.0],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            fov_degrees: 60.0,
            near: 0.1,
            aspect_ratio: 16.0 / 9.0,
        }
    }
}

impl ViewportCamera {
    /// Creates a new viewport camera for the given slot.
    pub fn new(viewport_slot: usize) -> Self {
        Self {
            viewport_slot,
            ..Default::default()
        }
    }

    /// Creates a viewport camera with custom position and target.
    pub fn with_position_and_target(
        viewport_slot: usize,
        position: [f32; 3],
        target: [f32; 3],
    ) -> Self {
        Self {
            viewport_slot,
            position,
            target,
            ..Default::default()
        }
    }

    /// Sets the aspect ratio.
    pub fn with_aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    /// Sets the field of view in degrees.
    pub fn with_fov(mut self, fov_degrees: f32) -> Self {
        self.fov_degrees = fov_degrees;
        self
    }

    /// Computes the view matrix for this camera.
    pub fn compute_view_matrix(&self) -> Mat4 {
        Mat4::create_lookat(
            Vec3::new(self.position[0], self.position[1], self.position[2]),
            Vec3::new(self.target[0], self.target[1], self.target[2]),
            Vec3::new(self.up[0], self.up[1], self.up[2]),
        )
    }

    /// Computes the projection matrix for this camera.
    pub fn compute_projection_matrix(&self) -> Mat4 {
        Mat4::create_proj(self.fov_degrees, self.aspect_ratio, self.near)
    }

    /// Computes the combined view-projection matrix.
    pub fn compute_view_projection_matrix(&self) -> Mat4 {
        let view = self.compute_view_matrix();
        let proj = self.compute_projection_matrix();
        proj.mul(&view)
    }
}

/// Resource storing the camera matrices for each viewport slot.
///
/// This is updated by the `ViewportCameraSystem` each frame.
#[derive(Debug)]
pub struct ViewportCameraMatrices {
    /// View matrix for each slot.
    pub view_matrices: [Option<Mat4>; 4],
    /// Projection matrix for each slot.
    pub projection_matrices: [Option<Mat4>; 4],
}

impl Default for ViewportCameraMatrices {
    fn default() -> Self {
        Self {
            view_matrices: [None, None, None, None],
            projection_matrices: [None, None, None, None],
        }
    }
}

impl ViewportCameraMatrices {
    /// Creates a new empty matrices resource.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the matrices for a viewport slot.
    pub fn set(&mut self, slot: usize, view: Mat4, projection: Mat4) {
        if slot < 4 {
            self.view_matrices[slot] = Some(view);
            self.projection_matrices[slot] = Some(projection);
        }
    }

    /// Clears the matrices for a viewport slot.
    pub fn clear(&mut self, slot: usize) {
        if slot < 4 {
            self.view_matrices[slot] = None;
            self.projection_matrices[slot] = None;
        }
    }

    /// Gets the view matrix for a slot.
    pub fn get_view(&self, slot: usize) -> Option<&Mat4> {
        if slot < 4 {
            self.view_matrices[slot].as_ref()
        } else {
            None
        }
    }

    /// Gets the projection matrix for a slot.
    pub fn get_projection(&self, slot: usize) -> Option<&Mat4> {
        if slot < 4 {
            self.projection_matrices[slot].as_ref()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_camera_creation() {
        let camera = ViewportCamera::new(2);
        assert_eq!(camera.viewport_slot, 2);
        assert_eq!(camera.position, [0.0, 5.0, 10.0]);
        assert_eq!(camera.target, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_viewport_camera_default() {
        let camera = ViewportCamera::default();
        assert_eq!(camera.viewport_slot, 0);
        assert_eq!(camera.fov_degrees, 60.0);
        assert_eq!(camera.aspect_ratio, 16.0 / 9.0);
    }

    #[test]
    fn test_viewport_camera_builder() {
        let camera = ViewportCamera::new(1)
            .with_aspect_ratio(4.0 / 3.0)
            .with_fov(45.0);

        assert_eq!(camera.viewport_slot, 1);
        assert_eq!(camera.aspect_ratio, 4.0 / 3.0);
        assert_eq!(camera.fov_degrees, 45.0);
    }

    #[test]
    fn test_viewport_camera_position_and_target() {
        let camera =
            ViewportCamera::with_position_and_target(0, [10.0, 20.0, 30.0], [5.0, 5.0, 5.0]);

        assert_eq!(camera.viewport_slot, 0);
        assert_eq!(camera.position, [10.0, 20.0, 30.0]);
        assert_eq!(camera.target, [5.0, 5.0, 5.0]);
    }

    #[test]
    fn test_compute_view_matrix() {
        let camera = ViewportCamera::new(0);
        let view = camera.compute_view_matrix();

        // View matrix should be valid (not all zeros)
        // Check that it's not the identity matrix by checking a few elements
        let view_data = view.to_array();
        // The view matrix should have non-zero elements
        assert!(view_data.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_compute_projection_matrix() {
        let camera = ViewportCamera::new(0)
            .with_fov(60.0)
            .with_aspect_ratio(16.0 / 9.0);
        let proj = camera.compute_projection_matrix();

        // Projection matrix should be valid
        let proj_data = proj.to_array();
        assert!(proj_data.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_viewport_camera_matrices_creation() {
        let matrices = ViewportCameraMatrices::new();

        // All slots should be None initially
        for slot in 0..4 {
            assert!(matrices.get_view(slot).is_none());
            assert!(matrices.get_projection(slot).is_none());
        }
    }

    #[test]
    fn test_viewport_camera_matrices_set_and_get() {
        let mut matrices = ViewportCameraMatrices::new();
        let camera = ViewportCamera::new(2);

        let view = camera.compute_view_matrix();
        let proj = camera.compute_projection_matrix();

        matrices.set(2, view, proj);

        assert!(matrices.get_view(2).is_some());
        assert!(matrices.get_projection(2).is_some());
    }

    #[test]
    fn test_viewport_camera_matrices_clear() {
        let mut matrices = ViewportCameraMatrices::new();
        let camera = ViewportCamera::new(1);

        let view = camera.compute_view_matrix();
        let proj = camera.compute_projection_matrix();

        matrices.set(1, view, proj);
        assert!(matrices.get_view(1).is_some());

        matrices.clear(1);
        assert!(matrices.get_view(1).is_none());
    }

    #[test]
    fn test_viewport_camera_matrices_out_of_bounds() {
        let mut matrices = ViewportCameraMatrices::new();
        let camera = ViewportCamera::new(0);

        let view = camera.compute_view_matrix();
        let proj = camera.compute_projection_matrix();

        // Setting slot 4 should be ignored
        matrices.set(4, view, proj);
        assert!(matrices.get_view(4).is_none());

        // Getting slot 5 should return None
        assert!(matrices.get_view(5).is_none());
    }
}
