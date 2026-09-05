//! 3D Transform Gizmo for the editor viewport.
//!
//! Provides translate, rotate, and scale manipulation handles rendered at the
//! selected entity's position. The gizmo maintains constant screen-space size
//! regardless of camera distance.

mod types;

/// Logical size before applying each handle's relative dimensions.
pub(crate) const GIZMO_SCREEN_SIZE: f32 = 80.0;

pub use types::{
    GizmoAxis, GizmoColor, GizmoHandle, GizmoMode, GizmoPlane, GizmoResources, GizmoState,
    HitTestParams, compute_gizmo_scale, compute_rotate_delta, compute_scale_delta,
    compute_scale_plane_delta, compute_translate_delta, compute_translate_plane_delta,
    generate_rotate_draw_calls, generate_scale_draw_calls, generate_translate_draw_calls,
    hit_test_axes, ray_plane_intersection, screen_to_ray, world_to_screen,
};
