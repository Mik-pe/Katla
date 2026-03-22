use bytemuck::{Pod, Zeroable};

pub const MAX_CASCADES: usize = 4;

#[derive(Debug, Clone)]
pub struct CascadeParams {
    pub num_cascades: usize,
    pub lambda: f32,
    pub max_distance: f32,
    pub shadow_map_size: u32,
    pub depth_bias_constant: f32,
    pub depth_bias_slope: f32,
}

impl Default for CascadeParams {
    fn default() -> Self {
        Self {
            num_cascades: 4,
            lambda: 0.65,
            max_distance: 100.0,
            shadow_map_size: 2048,
            depth_bias_constant: 1.5,
            depth_bias_slope: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CascadeInfo {
    pub view_matrix: [f32; 16],
    pub proj_matrix: [f32; 16],
    pub split_distance: f32,
    pub texel_size: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ShadowCascadeGPU {
    pub view_proj: [f32; 16],
    pub split_distance: f32,
    pub texel_size: f32,
    pub _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ShadowFrameData {
    pub cascades: [ShadowCascadeGPU; MAX_CASCADES],
    pub light_direction: [f32; 4],
    pub shadow_bias: [f32; 4],
}

pub struct CascadeShadowMap {
    params: CascadeParams,
    cascades: Vec<CascadeInfo>,
    light_direction: [f32; 3],
}

impl CascadeShadowMap {
    pub fn new(params: CascadeParams) -> Self {
        let num_cascades = params.num_cascades.min(MAX_CASCADES);
        let cascades = vec![
            CascadeInfo {
                view_matrix: [0.0; 16],
                proj_matrix: [0.0; 16],
                split_distance: 0.0,
                texel_size: 1.0 / params.shadow_map_size as f32,
            };
            num_cascades
        ];

        Self {
            params,
            cascades,
            light_direction: [0.0, -1.0, 0.0],
        }
    }

    pub fn update(
        &mut self,
        light_direction: [f32; 3],
        camera_view: &[f32; 16],
        camera_proj: &[f32; 16],
    ) {
        self.light_direction = normalize3(&light_direction);

        let view_inv = mat4_inverse(camera_view);
        let proj_inv = mat4_inverse(camera_proj);
        let view_proj_inv = mat4_mul(&proj_inv, &view_inv);

        let near = extract_near_from_reverse_z_proj(camera_proj);
        let far = self.params.max_distance;

        let split_distances =
            compute_pssm_splits(near, far, self.params.num_cascades, self.params.lambda);

        let texel_size = 1.0 / self.params.shadow_map_size as f32;

        for (i, cascade) in self.cascades.iter_mut().enumerate() {
            let split_near = if i == 0 { near } else { split_distances[i - 1] };
            let split_far = split_distances[i];

            let corners = frustum_slice_corners(&view_proj_inv, split_near, split_far, camera_proj);

            let center = frustum_center(&corners);
            let light_view = compute_light_view(&self.light_direction, &center);

            let light_corners: Vec<[f32; 3]> = corners
                .iter()
                .map(|c| mat4_transform_point(&light_view, c))
                .collect();

            let (mins, maxs) = compute_aabb(&light_corners);

            let (snapped_proj, light_view_snapped) =
                snap_to_texel(&light_view, &mins, &maxs, self.params.shadow_map_size);

            let stabilized_proj = apply_pancake(&snapped_proj, &mins);

            cascade.view_matrix = light_view_snapped;
            cascade.proj_matrix = stabilized_proj;
            cascade.split_distance = split_far;
            cascade.texel_size = texel_size;

            log::debug!(
                "cascade {}: near={:.2}, far={:.2}, center=({:.1},{:.1},{:.1})",
                i,
                split_near,
                split_far,
                center[0],
                center[1],
                center[2],
            );
        }
    }

    pub fn cascades(&self) -> &[CascadeInfo] {
        &self.cascades
    }

    pub fn params(&self) -> &CascadeParams {
        &self.params
    }

    pub fn cascade_count(&self) -> usize {
        self.cascades.len()
    }

    pub fn gpu_data(&self) -> ShadowFrameData {
        let mut gpu_cascades = [ShadowCascadeGPU {
            view_proj: [0.0; 16],
            split_distance: 0.0,
            texel_size: 0.0,
            _pad: [0.0; 2],
        }; MAX_CASCADES];

        for (i, cascade) in self.cascades.iter().enumerate() {
            let view_proj = mat4_mul(&cascade.proj_matrix, &cascade.view_matrix);
            gpu_cascades[i] = ShadowCascadeGPU {
                view_proj,
                split_distance: cascade.split_distance,
                texel_size: cascade.texel_size,
                _pad: [0.0; 2],
            };
        }

        ShadowFrameData {
            cascades: gpu_cascades,
            light_direction: [
                self.light_direction[0],
                self.light_direction[1],
                self.light_direction[2],
                self.cascades.len() as f32,
            ],
            shadow_bias: [
                self.params.depth_bias_constant,
                self.params.depth_bias_slope,
                0.0,
                0.0,
            ],
        }
    }
}

fn compute_pssm_splits(near: f32, far: f32, num_cascades: usize, lambda: f32) -> Vec<f32> {
    let mut splits = Vec::with_capacity(num_cascades);
    for i in 1..=num_cascades {
        let t = i as f32 / num_cascades as f32;
        let log_split = near * (far / near).powf(t);
        let linear_split = near + (far - near) * t;
        let d = lambda * log_split + (1.0 - lambda) * linear_split;
        splits.push(d);
    }
    splits
}

fn frustum_slice_corners(
    view_proj_inv: &[f32; 16],
    near: f32,
    far: f32,
    proj: &[f32; 16],
) -> [[f32; 3]; 8] {
    // Convert view-space distances to NDC z for reverse-Z projection.
    // In reverse-Z, NDC z=1 is near plane, z=0 is far plane.
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

/// Convert a view-space distance to NDC z for reverse-Z projection.
///
/// For infinite reverse-Z: ndc_z = near / distance
/// For finite reverse-Z:  ndc_z = (far + near - 2*far*near/distance) / (far - near)
fn view_distance_to_ndc_z(distance: f32, proj: &[f32; 16]) -> f32 {
    if proj[10].abs() < 1e-8 {
        // Infinite projection: proj[14] = near
        let near = proj[14];
        near / distance
    } else {
        // Finite projection
        // proj[10] = -(far + near) / (far - near)
        // proj[14] = -2 * far * near / (far - near)
        let a = proj[10];
        let b = proj[14];
        // Solve: near = b / (a - 1), far = b / (a + 1)
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

fn snap_to_texel(
    light_view: &[f32; 16],
    mins: &[f32; 3],
    maxs: &[f32; 3],
    shadow_map_size: u32,
) -> ([f32; 16], [f32; 16]) {
    let world_units_per_texel_x = (maxs[0] - mins[0]) / shadow_map_size as f32;
    let world_units_per_texel_y = (maxs[1] - mins[1]) / shadow_map_size as f32;

    let center_x = (mins[0] + maxs[0]) * 0.5;
    let center_y = (mins[1] + maxs[1]) * 0.5;

    let snapped_center_x = (center_x / world_units_per_texel_x).round() * world_units_per_texel_x;
    let snapped_center_y = (center_y / world_units_per_texel_y).round() * world_units_per_texel_y;

    let offset_x = snapped_center_x - center_x;
    let offset_y = snapped_center_y - center_y;

    let mut snapped_view = *light_view;
    snapped_view[12] += offset_x * light_view[0] + offset_y * light_view[4];
    snapped_view[13] += offset_x * light_view[1] + offset_y * light_view[5];
    snapped_view[14] += offset_x * light_view[2] + offset_y * light_view[6];

    // Stabilize Z: snap the near plane to prevent depth range jittering.
    // The Z extent (maxs[2] - mins[2]) is quantized upward to the next texel-sized step.
    let z_range = maxs[2] - mins[2];
    let z_units_per_texel = z_range / shadow_map_size as f32;
    let snapped_z_range = (z_range / z_units_per_texel).ceil() * z_units_per_texel;
    let snapped_maxs_z = mins[2] + snapped_z_range;

    let proj = mat4_ortho(
        mins[0] + offset_x,
        maxs[0] + offset_x,
        mins[1] + offset_y,
        maxs[1] + offset_y,
        mins[2],
        snapped_maxs_z,
    );

    (proj, snapped_view)
}

fn apply_pancake(proj: &[f32; 16], mins: &[f32; 3]) -> [f32; 16] {
    let mut p = *proj;
    let pancake_offset = mins[2] - 1.0;
    p[10] = -2.0 / ((mins[2] - pancake_offset) - mins[2]);
    p[14] = -((mins[2] - pancake_offset) + mins[2]) / ((mins[2] - pancake_offset) - mins[2]);
    p
}

fn extract_near_from_reverse_z_proj(proj: &[f32; 16]) -> f32 {
    if proj[10].abs() < 1e-8 {
        proj[14]
    } else {
        proj[14] / proj[10]
    }
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
        + m[8] * (m[2] * m[7] - m[3] * m[6]);
    inv[11] = -(m[0] * (m[5] * m[11] - m[7] * m[9]) - m[4] * (m[1] * m[11] - m[3] * m[9])
        + m[8] * (m[1] * m[7] - m[3] * m[5]));
    inv[15] = m[0] * (m[5] * m[10] - m[6] * m[9]) - m[4] * (m[1] * m[10] - m[2] * m[9])
        + m[8] * (m[1] * m[6] - m[2] * m[5]);

    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det.abs() < 1e-10 {
        return [0.0; 16];
    }

    let inv_det = 1.0 / det;
    for v in inv.iter_mut() {
        *v *= inv_det;
    }
    inv
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
    fn test_pssm_splits() {
        let splits = compute_pssm_splits(0.1, 100.0, 4, 0.5);
        assert_eq!(splits.len(), 4);
        assert!(splits[0] > 0.1);
        assert!(splits[3] <= 100.0);
        for i in 1..splits.len() {
            assert!(splits[i] > splits[i - 1]);
        }
    }

    #[test]
    fn test_pssm_splits_lambda_extremes() {
        let log_splits = compute_pssm_splits(0.1, 100.0, 4, 1.0);
        let lin_splits = compute_pssm_splits(0.1, 100.0, 4, 0.0);
        for i in 0..4 {
            assert!(log_splits[i] <= lin_splits[i]);
        }
    }

    #[test]
    fn test_mat4_mul_identity() {
        let id = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let result = mat4_mul(&id, &id);
        for i in 0..16 {
            assert!((result[i] - id[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_mat4_inverse_identity() {
        let id = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let inv = mat4_inverse(&id);
        let result = mat4_mul(&id, &inv);
        for i in 0..16 {
            let expected = if i % 5 == 0 { 1.0 } else { 0.0 };
            assert!(
                (result[i] - expected).abs() < 1e-5,
                "m[{}] = {}, expected {}",
                i,
                result[i],
                expected
            );
        }
    }

    #[test]
    fn test_cascade_shadow_map_creation() {
        let csm = CascadeShadowMap::new(CascadeParams::default());
        assert_eq!(csm.cascade_count(), 4);
    }

    #[test]
    fn test_cascade_shadow_map_update() {
        let mut csm = CascadeShadowMap::new(CascadeParams::default());
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 5.0, 1.0,
        ];
        let proj = katla_math_proj_reverse_z(60.0, 16.0 / 9.0, 0.1);
        csm.update([0.5, -0.8, -0.3], &view, &proj);
        assert_eq!(csm.cascade_count(), 4);
        for cascade in csm.cascades() {
            assert!(cascade.split_distance > 0.0);
        }
    }

    #[test]
    fn test_gpu_data_layout() {
        let csm = CascadeShadowMap::new(CascadeParams::default());
        let gpu = csm.gpu_data();
        assert_eq!(gpu.light_direction[3], 4.0);
    }

    #[test]
    fn test_snap_to_texel() {
        let light_view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mins = [-10.0, -10.0, -20.0];
        let maxs = [10.0, 10.0, -5.0];
        let (proj, snapped_view) = snap_to_texel(&light_view, &mins, &maxs, 1024);
        let result = mat4_mul(&proj, &snapped_view);
        for i in 0..16 {
            assert!(!result[i].is_nan());
        }
    }

    fn katla_math_proj_reverse_z(fov: f32, aspect: f32, near: f32) -> [f32; 16] {
        let f = 1.0 / (fov.to_radians() * 0.5).tan();
        [
            f / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            -f,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            -1.0,
            0.0,
            0.0,
            near,
            0.0,
        ]
    }
}
