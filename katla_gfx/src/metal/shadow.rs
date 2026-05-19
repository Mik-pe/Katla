//! Metal shadow pass subsystem for CSM (Cascaded Shadow Maps).

use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLFunction, MTLPixelFormat};

use crate::backend::command::{
    DepthAttachmentInfo, GpuCommandBuffer, GpuRenderEncoder, IndexType, RenderPassInfo,
    ShaderStages,
};
use crate::error::RendererError;
use crate::handle::ResourceStorage;
use crate::pipeline::CompareOp;
use crate::render_pass::{ClearValue, LoadOp, StoreOp};
use crate::texture::{ImageFormat, TextureDescriptor, TextureUsage};

use super::buffer::MetalBuffer;
use super::context::MetalContext;
use super::pipeline::MetalGraphicsPipeline;
use super::texture::MetalTextureView;

const DEFAULT_SHADOW_RESOLUTION: u32 = 2048;
const DEFAULT_CASCADE_COUNT: u32 = 4;
const CASCADE_SPLIT_LAMBDA: f32 = 0.75;

/// Per-cascade view-projection matrix and split distance.
#[derive(Clone, Copy, Debug)]
pub struct ShadowCascade {
    pub view_proj: [f32; 16],
    pub split_depth: f32,
}

/// Metal shadow subsystem for CSM shadow mapping.
pub(crate) struct MetalShadowSubsystem {
    shadow_map_texture: Option<MetalTextureView>,
    shadow_pipeline: Option<MetalGraphicsPipeline>,
    cascades: Vec<ShadowCascade>,
    light_direction: [f32; 3],
    shadow_resolution: u32,
    cascade_count: u32,
}

impl MetalShadowSubsystem {
    pub(crate) fn new() -> Self {
        Self {
            shadow_map_texture: None,
            shadow_pipeline: None,
            cascades: (0..DEFAULT_CASCADE_COUNT)
                .map(|_| ShadowCascade {
                    view_proj: [0.0; 16],
                    split_depth: 0.0,
                })
                .collect(),
            light_direction: [0.0, -1.0, 0.0],
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
            cascade_count: DEFAULT_CASCADE_COUNT,
        }
    }

    pub(crate) fn shadow_map_view(&self) -> Option<&MetalTextureView> {
        self.shadow_map_texture.as_ref()
    }

    pub(crate) fn pipeline(&self) -> Option<&MetalGraphicsPipeline> {
        self.shadow_pipeline.as_ref()
    }

    pub(crate) fn cascade_count(&self) -> u32 {
        self.cascade_count
    }

    pub(crate) fn cascade_view_proj(&self, index: usize) -> [f32; 16] {
        self.cascades
            .get(index)
            .map(|c| c.view_proj)
            .unwrap_or([0.0; 16])
    }

    pub(crate) fn cascade_split_depth(&self, index: usize) -> f32 {
        self.cascades
            .get(index)
            .map(|c| c.split_depth)
            .unwrap_or(0.0)
    }

    pub(crate) fn shadow_resolution(&self) -> u32 {
        self.shadow_resolution
    }

    /// Create the shadow map depth texture.
    pub(crate) fn create_shadow_map(
        &mut self,
        context: &MetalContext,
    ) -> Result<(), RendererError> {
        let desc = TextureDescriptor::new(
            self.shadow_resolution,
            self.shadow_resolution,
            ImageFormat::D32Sfloat,
        )
        .with_usage(TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED);

        let (_texture, view) = context.create_texture(&desc)?;
        self.shadow_map_texture = Some(view);
        Ok(())
    }

    /// Create the shadow depth-only pipeline.
    pub(crate) fn create_pipeline(
        &mut self,
        context: &MetalContext,
        vertex_function: &ProtocolObject<dyn MTLFunction>,
    ) -> Result<(), RendererError> {
        let pipeline = context.create_graphics_pipeline(
            vertex_function,
            None,
            &[],
            Some(MTLPixelFormat::Depth32Float),
            true,
            CompareOp::Less,
            objc2_metal::MTLCullMode::Front,
            objc2_metal::MTLWinding::CounterClockwise,
        )?;

        self.shadow_pipeline = Some(pipeline);
        Ok(())
    }

    /// Update shadow cascade view-projection matrices from camera and light.
    pub(crate) fn update_cascades(
        &mut self,
        view_matrix: &[f32; 16],
        proj_matrix: &[f32; 16],
        light_direction: [f32; 3],
    ) {
        self.light_direction = normalize3(&light_direction);

        let view_inv = mat4_inverse(view_matrix);
        let proj_inv = mat4_inverse(proj_matrix);
        let view_proj_inv = mat4_mul(&proj_inv, &view_inv);

        let near = extract_near(proj_matrix);
        let far = extract_far(proj_matrix);

        for i in 0..self.cascade_count as usize {
            let split_ratio = (i as f32 + 1.0) / self.cascade_count as f32;
            let split_dist = CASCADE_SPLIT_LAMBDA * near * (far / near).powf(split_ratio)
                + (1.0 - CASCADE_SPLIT_LAMBDA) * (near + (far - near) * split_ratio);

            let cascade_view_proj = compute_cascade_view_proj(
                &view_proj_inv,
                proj_matrix,
                &self.light_direction,
                near,
                split_dist,
            );
            self.cascades[i].view_proj = cascade_view_proj;
            self.cascades[i].split_depth = (split_dist - near) / (far - near);
        }
    }
}

/// Render a single shadow cascade.
///
/// Creates a depth-only render pass targeting the cascade's slice of the shadow map,
/// then draws all opaque geometry from the light's perspective.
pub(crate) fn render_cascade(
    cmd_buffer: &mut super::command_buffer::MetalCommandBuffer,
    shadow_pipeline: &MetalGraphicsPipeline,
    shadow_map_view: &MetalTextureView,
    shadow_resolution: u32,
    frame_uniform_buffer: &MetalBuffer,
    object_storage_buffer: &MetalBuffer,
    cascade_view_proj: &[f32; 16],
    meshes: &ResourceStorage<super::metal_renderer::MetalMesh>,
    materials: &ResourceStorage<super::metal_renderer::MetalMaterial>,
    draw_list: &crate::renderer::types::DrawList,
) {
    let render_pass_info = RenderPassInfo {
        color_attachments: vec![],
        depth_attachment: Some(DepthAttachmentInfo {
            view: shadow_map_view.clone(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: ClearValue::DepthStencil {
                depth: 1.0,
                stencil: 0,
            },
            format: ImageFormat::D32Sfloat,
        }),
    };

    let mut encoder = cmd_buffer.begin_render_pass(render_pass_info);

    encoder.bind_graphics_pipeline(shadow_pipeline);
    encoder.set_viewport(
        0.0,
        0.0,
        shadow_resolution as f32,
        shadow_resolution as f32,
        0.0,
        1.0,
    );

    let stages = ShaderStages::VERTEX;
    encoder.bind_storage_buffer(frame_uniform_buffer, 0, 0, stages);
    encoder.bind_storage_buffer(object_storage_buffer, 0, 1, stages);

    encoder.set_push_constants(
        bytemuck::cast_slice(cascade_view_proj),
        2,
        ShaderStages::VERTEX,
    );

    for draw in &draw_list.draws {
        let Some(mesh) = meshes.get(draw.mesh.index()) else {
            continue;
        };
        let Some(material) = materials.get(draw.material.index()) else {
            continue;
        };
        let Some(ref _pipeline) = material.pipeline else {
            continue;
        };

        encoder.bind_vertex_buffer(&mesh.vertex_buffer, 0, 0);
        encoder.bind_index_buffer(&mesh.index_buffer, 0, IndexType::Uint32);
        encoder.draw_indexed(mesh.index_count, 1, 0, 0, draw.instance_index);
    }

    encoder.end_encoding();
}

/// Extract the near plane from a projection matrix.
fn extract_near(proj: &[f32; 16]) -> f32 {
    let m22 = proj[10];
    let m32 = proj[14];
    if m32 != 0.0 { m32 / (m22 - 1.0) } else { 0.1 }
}

/// Extract the far plane from a projection matrix.
fn extract_far(proj: &[f32; 16]) -> f32 {
    let m22 = proj[10];
    let m32 = proj[14];
    if m32 != 0.0 {
        m32 / (m22 + 1.0)
    } else {
        1000.0
    }
}

/// Compute a view-projection matrix for a shadow cascade using PSSM.
fn compute_cascade_view_proj(
    view_proj_inv: &[f32; 16],
    proj: &[f32; 16],
    light_dir: &[f32; 3],
    near: f32,
    far: f32,
) -> [f32; 16] {
    let corners = frustum_slice_corners(view_proj_inv, near, far, proj);
    let center = frustum_center(&corners);
    let light_view = compute_light_view(light_dir, &center);

    let light_corners: Vec<[f32; 3]> = corners
        .iter()
        .map(|c| mat4_transform_point(&light_view, c))
        .collect();

    let (mins, maxs) = compute_aabb(&light_corners);
    let light_proj = mat4_ortho(mins[0], maxs[0], mins[1], maxs[1], mins[2], maxs[2]);

    mat4_mul(&light_proj, &light_view)
}

fn frustum_slice_corners(
    view_proj_inv: &[f32; 16],
    near: f32,
    far: f32,
    proj: &[f32; 16],
) -> [[f32; 3]; 8] {
    let ndc_near = view_distance_to_ndc_z(near, proj);
    let ndc_far = view_distance_to_ndc_z(far, proj);

    let ndc_corners: [[f32; 3]; 8] = [
        [-1.0, -1.0, ndc_near],
        [1.0, -1.0, ndc_near],
        [1.0, 1.0, ndc_near],
        [-1.0, 1.0, ndc_near],
        [-1.0, -1.0, ndc_far],
        [1.0, -1.0, ndc_far],
        [1.0, 1.0, ndc_far],
        [-1.0, 1.0, ndc_far],
    ];

    let mut world_corners = [[0.0f32; 3]; 8];
    for (i, nc) in ndc_corners.iter().enumerate() {
        let p = [nc[0], nc[1], nc[2], 1.0];
        let tp = mat4_transform_vec4(view_proj_inv, &p);
        let w = 1.0 / tp[3];
        world_corners[i] = [tp[0] * w, tp[1] * w, tp[2] * w];
    }
    world_corners
}

fn view_distance_to_ndc_z(distance: f32, proj: &[f32; 16]) -> f32 {
    if proj[10].abs() < 1e-8 {
        let near = proj[14];
        near / distance
    } else {
        let a = proj[10];
        let b = proj[14];
        let near = b / (a - 1.0);
        let far = b / (a + 1.0);
        (far + near - 2.0 * far * near / distance) / (far - near)
    }
}

fn frustum_center(corners: &[[f32; 3]; 8]) -> [f32; 3] {
    let mut c = [0.0f32; 3];
    for corner in corners.iter() {
        c[0] += corner[0];
        c[1] += corner[1];
        c[2] += corner[2];
    }
    let inv = 1.0 / 8.0;
    [c[0] * inv, c[1] * inv, c[2] * inv]
}

fn compute_light_view(light_dir: &[f32; 3], center: &[f32; 3]) -> [f32; 16] {
    let light_dir = normalize3(light_dir);
    let up = if light_dir[1].abs() > 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };

    let right = normalize3(&cross3(&light_dir, &up));
    let true_up = cross3(&right, &light_dir);

    [
        right[0],
        true_up[0],
        -light_dir[0],
        0.0,
        right[1],
        true_up[1],
        -light_dir[1],
        0.0,
        right[2],
        true_up[2],
        -light_dir[2],
        0.0,
        -dot3(&right, center),
        -dot3(&true_up, center),
        dot3(&light_dir, center),
        1.0,
    ]
}

fn compute_aabb(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut mins = [f32::MAX; 3];
    let mut maxs = [f32::MIN; 3];
    for p in points.iter() {
        for j in 0..3 {
            mins[j] = mins[j].min(p[j]);
            maxs[j] = maxs[j].max(p[j]);
        }
    }
    (mins, maxs)
}

fn normalize3(v: &[f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-8 {
        [0.0, -1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn cross3(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = sum;
        }
    }
    r
}

fn mat4_inverse(m: &[f32; 16]) -> [f32; 16] {
    let m = *m;
    let mut inv = [0.0f32; 16];

    inv[0] = m[5] * (m[10] * m[15] - m[11] * m[14]) - m[9] * (m[6] * m[15] - m[7] * m[14])
        + m[13] * (m[6] * m[11] - m[7] * m[10]);
    inv[4] = -(m[4] * (m[10] * m[15] - m[11] * m[14]) - m[8] * (m[6] * m[15] - m[7] * m[14])
        + m[12] * (m[6] * m[11] - m[7] * m[10]));
    inv[8] = m[4] * (m[9] * m[15] - m[11] * m[13]) - m[8] * (m[5] * m[15] - m[7] * m[13])
        + m[12] * (m[5] * m[11] - m[7] * m[9]);
    inv[12] = -(m[4] * (m[9] * m[14] - m[10] * m[13]) - m[8] * (m[5] * m[14] - m[6] * m[13])
        + m[12] * (m[5] * m[10] - m[6] * m[9]));

    inv[1] = -(m[1] * (m[10] * m[15] - m[11] * m[14]) - m[9] * (m[2] * m[15] - m[3] * m[14])
        + m[13] * (m[2] * m[11] - m[3] * m[10]));
    inv[5] = m[0] * (m[10] * m[15] - m[11] * m[14]) - m[8] * (m[2] * m[15] - m[3] * m[14])
        + m[12] * (m[2] * m[11] - m[3] * m[10]);
    inv[9] = -(m[0] * (m[9] * m[15] - m[11] * m[13]) - m[8] * (m[1] * m[15] - m[3] * m[13])
        + m[12] * (m[1] * m[11] - m[3] * m[9]));
    inv[13] = m[0] * (m[9] * m[14] - m[10] * m[13]) - m[8] * (m[1] * m[14] - m[2] * m[13])
        + m[12] * (m[1] * m[10] - m[2] * m[9]);

    inv[2] = m[1] * (m[6] * m[15] - m[7] * m[14]) - m[5] * (m[2] * m[15] - m[3] * m[14])
        + m[13] * (m[2] * m[7] - m[3] * m[6]);
    inv[6] = -(m[0] * (m[6] * m[15] - m[7] * m[14]) - m[4] * (m[2] * m[15] - m[3] * m[14])
        + m[12] * (m[2] * m[7] - m[3] * m[6]));
    inv[10] = m[0] * (m[5] * m[15] - m[7] * m[13]) - m[4] * (m[1] * m[15] - m[3] * m[13])
        + m[12] * (m[1] * m[7] - m[3] * m[5]);
    inv[14] = -(m[0] * (m[5] * m[14] - m[6] * m[13]) - m[4] * (m[1] * m[14] - m[2] * m[13])
        + m[12] * (m[1] * m[6] - m[2] * m[5]));

    inv[3] = -(m[1] * (m[6] * m[11] - m[7] * m[10]) - m[5] * (m[2] * m[11] - m[3] * m[10])
        + m[9] * (m[2] * m[7] - m[3] * m[6]));
    inv[7] = m[0] * (m[6] * m[11] - m[7] * m[10]) - m[4] * (m[2] * m[11] - m[3] * m[10])
        + m[8] * (m[2] * m[7] - m[3] * m[5]);
    inv[11] = -(m[0] * (m[5] * m[11] - m[7] * m[9]) - m[4] * (m[1] * m[11] - m[3] * m[9])
        + m[8] * (m[1] * m[7] - m[3] * m[5]));
    inv[15] = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9])
        + m[8] * (m[1] * m[6] - m[2] * m[5]);

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-10 {
        let mut identity = [0.0f32; 16];
        identity[0] = 1.0;
        identity[5] = 1.0;
        identity[10] = 1.0;
        identity[15] = 1.0;
        return identity;
    }

    let inv_det = 1.0 / det;
    for v in inv.iter_mut() {
        *v *= inv_det;
    }
    inv
}

fn mat4_transform_point(m: &[f32; 16], p: &[f32; 3]) -> [f32; 3] {
    let x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12];
    let y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13];
    let z = m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14];
    [x, y, z]
}

fn mat4_transform_vec4(m: &[f32; 16], v: &[f32; 4]) -> [f32; 4] {
    let x = m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12] * v[3];
    let y = m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13] * v[3];
    let z = m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14] * v[3];
    let w = m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15] * v[3];
    [x, y, z, w]
}

fn mat4_ortho(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> [f32; 16] {
    [
        2.0 / (right - left),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / (top - bottom),
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / (far - near),
        0.0,
        -(right + left) / (right - left),
        -(top + bottom) / (top - bottom),
        -(far + near) / (far - near),
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_subsystem_creation() {
        let subsystem = MetalShadowSubsystem::new();
        assert!(subsystem.shadow_pipeline.is_none());
        assert!(subsystem.shadow_map_texture.is_none());
        assert_eq!(subsystem.cascades.len(), 4);
    }

    #[test]
    fn test_extract_near_far() {
        let mut proj = [0.0f32; 16];
        let near = 0.1f32;
        let far = 100.0f32;
        proj[0] = 1.0;
        proj[5] = 1.0;
        proj[10] = far / (near - far);
        proj[11] = -1.0;
        proj[14] = (near * far) / (near - far);

        let extracted_near = extract_near(&proj);
        let extracted_far = extract_far(&proj);
        assert!(extracted_near.is_finite());
        assert!(extracted_far.is_finite());
    }

    #[test]
    fn test_compute_cascade_view_proj() {
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 5.0, 1.0,
        ];
        let fov = 60.0_f32;
        let f = 1.0 / (fov.to_radians() * 0.5).tan();
        let near = 0.1_f32;
        // Infinite reverse-Z projection
        let proj = [
            f, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0,
        ];
        let proj_inv = mat4_inverse(&proj);
        let view_inv = mat4_inverse(&view);
        let view_proj_inv = mat4_mul(&proj_inv, &view_inv);
        let result = compute_cascade_view_proj(&view_proj_inv, &proj, &[0.0, -1.0, 0.0], 0.1, 10.0);
        for i in 0..16 {
            assert!(!result[i].is_nan(), "view_proj[{}] is NaN", i);
        }
    }
}
