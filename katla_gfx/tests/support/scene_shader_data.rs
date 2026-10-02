//! Private CPU data layout used by scene-shader contract fixtures.

/// Frame-level uniforms that are shared across all draw calls.
///
/// Applications supply this shader ABI through explicit pass constants.
/// View/projection matrices come from the camera, lighting from the scene.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct ShaderFrameData {
    /// View matrix (world to camera transform) - column-major 4x4.
    pub view_matrix: [f32; 16],
    /// Projection matrix (camera to clip space) - column-major 4x4.
    pub proj_matrix: [f32; 16],
    /// Inverse view-projection matrix (clip to world space) - for sky rendering.
    pub inv_view_proj_matrix: [f32; 16],
    /// Camera position in world space.
    pub camera_position: [f32; 4],
    /// Light direction (normalized, points TO the light).
    pub light_direction: [f32; 4],
    /// Light color (RGB).
    pub light_color: [f32; 4],
    /// Light intensity and screen-space effect parameters.
    /// [x = intensity, y = depth_texture_bindless_idx, z = unused, w = unused]
    pub light_intensity: [f32; 4],
    /// Forward+ tile grid dimensions: [tiles_x, tiles_y, 0, 0].
    pub tiles: [u32; 4],
    /// Tonemap parameters: [exposure, gamma, mode, hdr_texture_index].
    /// Supplied by the application for postprocessing.
    pub tonemap: [f32; 4],
    /// Overlay parameters: [ldr_texture_index, stencil_indicator_index, 0, 0].
    /// Set by the frame graph before overlay pass execution.
    pub overlay: [f32; 4],
    /// Compositing parameters: [screen_width, screen_height, viewport_count, viewport_bindless_index].
    pub compositing: [f32; 4],
}

impl Default for ShaderFrameData {
    fn default() -> Self {
        Self {
            view_matrix: [0.0; 16],
            proj_matrix: [0.0; 16],
            inv_view_proj_matrix: [0.0; 16],
            camera_position: [0.0, 0.0, 0.0, 0.0],
            light_direction: [0.3, 1.0, 0.2, 0.0], // Upward toward sun
            light_color: [1.0, 0.98, 0.95, 0.0],   // Slightly warm white
            light_intensity: [3.0, 0.0, 0.0, 0.0], // HDR intensity for PBR
            tiles: [0, 0, 0, 0],
            tonemap: [1.0, 2.2, 0.0, 0.0],
            overlay: [0.0, 0.0, 0.0, 0.0],
            compositing: [0.0, 0.0, 0.0, 0.0],
        }
    }
}
