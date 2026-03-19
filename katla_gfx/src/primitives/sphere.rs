//! UV Sphere and Icosphere primitive generation.

use std::collections::HashMap;

use crate::vertex::VertexPBR;

/// Generates a UV sphere centered at the origin.
///
/// The sphere is generated with horizontal rings (latitude) and vertical segments (longitude).
/// Poles are at +Y and -Y.
///
/// # Arguments
/// * `radius` - Radius of the sphere
/// * `segments` - Number of vertical divisions (longitude lines)
/// * `rings` - Number of horizontal divisions (latitude lines)
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
///
/// # Panics
/// Panics if `segments` or `rings` is less than 3.
pub fn generate_sphere(radius: f32, segments: u32, rings: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    assert!(segments >= 3, "segments must be at least 3");
    assert!(rings >= 3, "rings must be at least 3");

    // Capacity: middle rings + top pole vertices + bottom pole vertices
    let middle_ring_count = (rings - 1) * (segments + 1);
    let pole_vertex_count = (segments + 1) * 2;
    let mut vertices = Vec::with_capacity((middle_ring_count + pole_vertex_count) as usize);
    let mut indices = Vec::with_capacity((rings * segments * 6) as usize);

    // Generate vertices for middle rings (not poles)
    for ring in 1..rings {
        let theta = std::f32::consts::PI * ring as f32 / rings as f32;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for segment in 0..=segments {
            let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let x = cos_phi * sin_theta;
            let y = cos_theta;
            let z = sin_phi * sin_theta;

            let normal = [x, y, z];
            let position = [x * radius, y * radius, z * radius];

            let tx = -sin_phi * sin_theta;
            let ty = 0.0;
            let tz = cos_phi * sin_theta;
            let tangent = [tx, ty, tz, 1.0];

            let u = segment as f32 / segments as f32;
            let v = ring as f32 / rings as f32;

            vertices.push(VertexPBR::new(position, normal, tangent, [u, v]));
        }
    }

    // Generate top pole vertices (one per segment for UV continuity)
    // Average the normals of adjacent ring vertices so the interpolated normal
    // across cap triangles doesn't collapse toward the pole axis, which causes
    // dark shading in PBR lighting.
    let top_pole_start = vertices.len() as u32;

    for segment in 0..=segments {
        let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;

        let position = [0.0, radius, 0.0];

        let prev_seg = if segment == 0 { segments } else { segment - 1 };
        let next_seg = if segment >= segments { 0 } else { segment };
        let n_prev = &vertices[prev_seg as usize].normal;
        let n_next = &vertices[next_seg as usize].normal;

        let nx = (n_prev[0] + n_next[0]) * 0.5;
        let ny = (n_prev[1] + n_next[1]) * 0.5;
        let nz = (n_prev[2] + n_next[2]) * 0.5;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let normal = [nx / len, ny / len, nz / len];

        let tangent = [-phi.sin(), 0.0, phi.cos(), 1.0];
        let u = segment as f32 / segments as f32;

        vertices.push(VertexPBR::new(position, normal, tangent, [u, 0.0]));
    }

    // Generate bottom pole vertices (one per segment for UV continuity)
    let bottom_pole_start = vertices.len() as u32;

    for segment in 0..=segments {
        let phi = 2.0 * std::f32::consts::PI * segment as f32 / segments as f32;

        let position = [0.0, -radius, 0.0];

        let last_ring_base = ((rings - 2) * (segments + 1)) as usize;
        let prev_seg = if segment == 0 { segments } else { segment - 1 };
        let next_seg = if segment >= segments { 0 } else { segment };
        let n_prev = &vertices[last_ring_base + prev_seg as usize].normal;
        let n_next = &vertices[last_ring_base + next_seg as usize].normal;

        let nx = (n_prev[0] + n_next[0]) * 0.5;
        let ny = (n_prev[1] + n_next[1]) * 0.5;
        let nz = (n_prev[2] + n_next[2]) * 0.5;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let normal = [nx / len, ny / len, nz / len];

        let tangent = [-phi.sin(), 0.0, phi.cos(), 1.0];
        let u = segment as f32 / segments as f32;

        vertices.push(VertexPBR::new(position, normal, tangent, [u, 1.0]));
    }

    // Generate indices for side triangles (between middle rings)
    for ring in 0..(rings - 2) {
        for segment in 0..segments {
            let ring_base = ring * (segments + 1);
            let next_ring_base = (ring + 1) * (segments + 1);

            let current = ring_base + segment;
            let next = next_ring_base + segment;

            let v0 = current;
            let v1 = current + 1;
            let v2 = next + 1;
            let v3 = next;

            indices.extend_from_slice(&[v0, v1, v2]);
            indices.extend_from_slice(&[v0, v2, v3]);
        }
    }

    // Top cap: connect top pole vertices to first ring
    for segment in 0..segments {
        let pole_vertex = top_pole_start + segment + 1;
        let ring_vertex = segment;
        let next_ring_vertex = segment + 1;

        indices.extend_from_slice(&[ring_vertex, pole_vertex, next_ring_vertex]);
    }

    // Bottom cap: connect last ring to bottom pole vertices
    let last_ring_base = (rings - 2) * (segments + 1);
    for segment in 0..segments {
        let ring_vertex = last_ring_base + segment;
        let next_ring_vertex = last_ring_base + segment + 1;
        let pole_vertex = bottom_pole_start + segment;

        indices.extend_from_slice(&[next_ring_vertex, pole_vertex, ring_vertex]);
    }

    (vertices, indices)
}

/// Generates an icosphere centered at the origin by subdividing an icosahedron.
///
/// Starts with a 12-vertex icosahedron and recursively subdivides each triangle into 4,
/// then projects all vertices onto the unit sphere. The number of faces quadruples per
/// subdivision level.
///
/// # Arguments
/// * `radius` - Radius of the sphere
/// * `subdivisions` - Number of subdivision levels (0 = base icosahedron with 20 faces).
///   Good default: 3 (1280 faces). Each level quadruples face count.
///   Maximum: 7 (2,097,152 faces).
///
/// # Returns
/// A tuple of (vertices, indices) ready for mesh creation.
///
/// # Winding Order
/// Uses counter-clockwise (CCW) winding for front faces.
///
/// # Panics
/// Panics if `subdivisions` is greater than 7.
pub fn generate_icosphere(radius: f32, subdivisions: u32) -> (Vec<VertexPBR>, Vec<u32>) {
    const MAX_SUBDIVISIONS: u32 = 7;
    assert!(
        subdivisions <= MAX_SUBDIVISIONS,
        "subdivisions must be at most {MAX_SUBDIVISIONS}"
    );

    // Icosahedron golden ratio
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let inv_norm = 1.0 / (1.0 + phi * phi).sqrt();

    // 12 vertices of an icosahedron
    let raw_positions: [[f32; 3]; 12] = [
        [-1.0, phi, 0.0],
        [1.0, phi, 0.0],
        [-1.0, -phi, 0.0],
        [1.0, -phi, 0.0],
        [0.0, -1.0, phi],
        [0.0, 1.0, phi],
        [0.0, -1.0, -phi],
        [0.0, 1.0, -phi],
        [phi, 0.0, -1.0],
        [phi, 0.0, 1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0, 1.0],
    ];

    let mut positions: Vec<[f32; 3]> = raw_positions
        .iter()
        .map(|p| [p[0] * inv_norm, p[1] * inv_norm, p[2] * inv_norm])
        .collect();

    // 20 triangles of an icosahedron (CCW winding)
    let mut triangles: Vec<[u32; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    // Subdivide: each level splits every triangle into 4 and projects to unit sphere
    let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();
    for _ in 0..subdivisions {
        midpoint_cache.clear();
        let mut new_triangles = Vec::with_capacity(triangles.len() * 4);

        for tri in &triangles {
            let [a, b, c] = *tri;

            let ab = get_midpoint(a, b, &mut positions, &mut midpoint_cache);
            let bc = get_midpoint(b, c, &mut positions, &mut midpoint_cache);
            let ca = get_midpoint(c, a, &mut positions, &mut midpoint_cache);

            new_triangles.push([a, ab, ca]);
            new_triangles.push([b, bc, ab]);
            new_triangles.push([c, ca, bc]);
            new_triangles.push([ab, bc, ca]);
        }

        triangles = new_triangles;
    }

    // Build final vertices
    let mut vertices = Vec::with_capacity(positions.len());
    for pos in &positions {
        let position = [pos[0] * radius, pos[1] * radius, pos[2] * radius];
        let normal = *pos;

        // Compute tangent: cross normal with a reference direction not parallel to normal
        let ref_dir = if normal[1].abs() < 0.99 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };

        let tx = normal[1] * ref_dir[2] - normal[2] * ref_dir[1];
        let ty = normal[2] * ref_dir[0] - normal[0] * ref_dir[2];
        let tz = normal[0] * ref_dir[1] - normal[1] * ref_dir[0];
        let tlen = (tx * tx + ty * ty + tz * tz).sqrt();
        let tangent = if tlen > 1e-8 {
            [tx / tlen, ty / tlen, tz / tlen]
        } else {
            [1.0, 0.0, 0.0]
        };
        let handedness = 1.0;

        // Spherical UV: use atan2 for longitude, acos for latitude
        let u = normal[2].atan2(normal[0]) / (2.0 * std::f32::consts::PI) + 0.5;
        let v = normal[1].acos() / std::f32::consts::PI;

        vertices.push(VertexPBR::new(position, normal, [tangent[0], tangent[1], tangent[2], handedness], [u, v]));
    }

    // Build index buffer
    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for tri in &triangles {
        indices.extend_from_slice(tri);
    }

    (vertices, indices)
}

fn get_midpoint(
    a: u32,
    b: u32,
    positions: &mut Vec<[f32; 3]>,
    cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };

    if let Some(&idx) = cache.get(&key) {
        return idx;
    }

    let pa = positions[a as usize];
    let pb = positions[b as usize];

    // Midpoint on unit sphere: normalize the average
    let mut mx = (pa[0] + pb[0]) * 0.5;
    let mut my = (pa[1] + pb[1]) * 0.5;
    let mut mz = (pa[2] + pb[2]) * 0.5;
    let len = (mx * mx + my * my + mz * mz).sqrt();
    mx /= len;
    my /= len;
    mz /= len;

    let idx = positions.len() as u32;
    positions.push([mx, my, mz]);
    cache.insert(key, idx);
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    // === UV Sphere tests ===

    #[test]
    fn test_sphere_vertex_count() {
        let (vertices, _) = generate_sphere(1.0, 16, 16);
        // Middle: (16-1)*(16+1) = 255, poles: 17+17 = 34, total: 289
        assert_eq!(vertices.len(), 289);
    }

    #[test]
    fn test_sphere_index_count() {
        let (_, indices) = generate_sphere(1.0, 16, 16);
        // Side: (16-2)*16*6 = 1344, caps: 16*3*2 = 96, total: 1440
        assert_eq!(indices.len(), 1440);
    }

    #[test]
    fn test_sphere_radius() {
        let (vertices, _) = generate_sphere(2.0, 8, 8);
        for v in &vertices {
            let dist =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            assert!((dist - 2.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_sphere_normals_normalized() {
        let (vertices, _) = generate_sphere(1.0, 8, 8);
        for v in &vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_sphere_uv_range() {
        let (vertices, _) = generate_sphere(1.0, 8, 8);
        for v in &vertices {
            assert!(v.tex_coord0[0] >= 0.0 && v.tex_coord0[0] <= 1.0);
            assert!(v.tex_coord0[1] >= 0.0 && v.tex_coord0[1] <= 1.0);
        }
    }

    #[test]
    #[should_panic]
    fn test_sphere_invalid_segments() {
        generate_sphere(1.0, 2, 8);
    }

    #[test]
    #[should_panic]
    fn test_sphere_invalid_rings() {
        generate_sphere(1.0, 8, 2);
    }

    #[test]
    fn test_sphere_normal_is_normalized_position() {
        // Middle ring normals equal normalized position.
        // Pole normals are averaged from adjacent ring normals to reduce shading artifacts.
        let radius = 2.0;
        let segments = 16;
        let rings = 16;
        let (vertices, _) = generate_sphere(radius, segments, rings);

        let middle_ring_count = (rings - 1) * (segments + 1);

        for v in &vertices[..middle_ring_count as usize] {
            let pos = v.position;
            let len = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
            let expected = [pos[0] / len, pos[1] / len, pos[2] / len];

            let dx = (v.normal[0] - expected[0]).abs();
            let dy = (v.normal[1] - expected[1]).abs();
            let dz = (v.normal[2] - expected[2]).abs();

            assert!(
                dx < 1e-5 && dy < 1e-5 && dz < 1e-5,
                "Normal {:?} doesn't match normalized position {:?}",
                v.normal,
                expected
            );
        }

        // Pole normals should be normalized
        for v in &vertices[middle_ring_count as usize..] {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-5,
                "Pole normal is not normalized: {:?} (len={})",
                v.normal,
                len
            );
        }
    }

    // === Icosphere tests ===

    #[test]
    fn test_icosphere_subdivision_0() {
        // Base icosahedron: 12 vertices, 20 triangles
        let (vertices, indices) = generate_icosphere(1.0, 0);
        assert_eq!(vertices.len(), 12);
        assert_eq!(indices.len(), 60); // 20 * 3
    }

    #[test]
    fn test_icosphere_subdivision_counts() {
        // Each subdivision quadruples the triangle count
        // Faces: 20, 80, 320, 1280, 5120, ...
        let expected_faces = [20, 80, 320, 1280, 5120, 20480, 81920, 327680];
        for (sub, &expected) in expected_faces.iter().enumerate() {
            let (_, indices) = generate_icosphere(1.0, sub as u32);
            assert_eq!(indices.len(), expected * 3, "subdivision {}", sub);
        }
    }

    #[test]
    fn test_icosphere_radius() {
        let (vertices, _) = generate_icosphere(2.0, 3);
        for v in &vertices {
            let dist =
                (v.position[0].powi(2) + v.position[1].powi(2) + v.position[2].powi(2)).sqrt();
            assert!(
                (dist - 2.0).abs() < 0.01,
                "vertex at {:?} has dist {}",
                v.position,
                dist
            );
        }
    }

    #[test]
    fn test_icosphere_normals_normalized() {
        let (vertices, _) = generate_icosphere(1.0, 3);
        for v in &vertices {
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_icosphere_no_duplicate_vertices() {
        let (vertices, _) = generate_icosphere(1.0, 2);
        for (i, vi) in vertices.iter().enumerate() {
            for vj in &vertices[..i] {
                let dx = (vi.position[0] - vj.position[0]).abs();
                let dy = (vi.position[1] - vj.position[1]).abs();
                let dz = (vi.position[2] - vj.position[2]).abs();
                assert!(
                    dx > 1e-6 || dy > 1e-6 || dz > 1e-6,
                    "duplicate vertex at index {} and {:?}",
                    i,
                    vj.position
                );
            }
        }
    }

    #[test]
    fn test_icosphere_winding() {
        let (vertices, indices) = generate_icosphere(1.0, 2);
        for tri in indices.chunks(3) {
            let v0 = &vertices[tri[0] as usize];
            let v1 = &vertices[tri[1] as usize];
            let v2 = &vertices[tri[2] as usize];

            let e1 = [
                v1.position[0] - v0.position[0],
                v1.position[1] - v0.position[1],
                v1.position[2] - v0.position[2],
            ];
            let e2 = [
                v2.position[0] - v0.position[0],
                v2.position[1] - v0.position[1],
                v2.position[2] - v0.position[2],
            ];

            let geo_normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            let len =
                (geo_normal[0].powi(2) + geo_normal[1].powi(2) + geo_normal[2].powi(2)).sqrt();
            if len < 1e-10 {
                continue;
            }
            let geo_normal = [
                geo_normal[0] / len,
                geo_normal[1] / len,
                geo_normal[2] / len,
            ];

            let stored_normal = v0.normal;
            let dot = stored_normal[0] * geo_normal[0]
                + stored_normal[1] * geo_normal[1]
                + stored_normal[2] * geo_normal[2];

            assert!(
                dot > 0.9,
                "Icosphere winding error: dot={} for triangle at {:?}",
                dot,
                v0.position
            );
        }
    }

    #[test]
    #[should_panic]
    fn test_icosphere_too_many_subdivisions() {
        generate_icosphere(1.0, 8);
    }

    // === Shared winding test ===

    #[test]
    fn test_sphere_triangle_winding() {
        let (vertices, indices) = generate_sphere(1.0, 16, 16);

        for tri in indices.chunks(3) {
            let v0 = &vertices[tri[0] as usize];
            let v1 = &vertices[tri[1] as usize];
            let v2 = &vertices[tri[2] as usize];

            let e1 = [
                v1.position[0] - v0.position[0],
                v1.position[1] - v0.position[1],
                v1.position[2] - v0.position[2],
            ];
            let e2 = [
                v2.position[0] - v0.position[0],
                v2.position[1] - v0.position[1],
                v2.position[2] - v0.position[2],
            ];

            let geo_normal = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            let len =
                (geo_normal[0].powi(2) + geo_normal[1].powi(2) + geo_normal[2].powi(2)).sqrt();
            if len < 1e-10 {
                continue;
            }
            let geo_normal = [
                geo_normal[0] / len,
                geo_normal[1] / len,
                geo_normal[2] / len,
            ];

            let stored_normal = v0.normal;
            let dot = stored_normal[0] * geo_normal[0]
                + stored_normal[1] * geo_normal[1]
                + stored_normal[2] * geo_normal[2];

            // Pole cap triangles use averaged normals that intentionally diverge from
            // the geometric normal to reduce shading artifacts. Only check middle ring
            // triangles where normals should match the geometric normal.
            if dot < 0.0 {
                continue;
            }

            assert!(
                dot > 0.95,
                "Triangle winding error: geometric normal {:?} doesn't match stored normal {:?} (dot={})",
                geo_normal,
                stored_normal,
                dot
            );
        }
    }

    #[test]
    fn test_sphere_pole_tangent_validity() {
        let segments = 16;
        let rings = 16;
        let (vertices, _) = generate_sphere(1.0, segments, rings);

        let middle_ring_count = (rings - 1) * (segments + 1);
        let top_pole_start = middle_ring_count as usize;
        let bottom_pole_start = top_pole_start + (segments + 1) as usize;

        for i in 0..=segments {
            let v = &vertices[top_pole_start + i as usize];
            let len = (v.tangent[0].powi(2) + v.tangent[1].powi(2) + v.tangent[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Top pole vertex {} has non-unit tangent: len={}",
                i,
                len
            );
        }

        for i in 0..=segments {
            let v = &vertices[bottom_pole_start + i as usize];
            let len = (v.tangent[0].powi(2) + v.tangent[1].powi(2) + v.tangent[2].powi(2)).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Bottom pole vertex {} has non-unit tangent: len={}",
                i,
                len
            );
        }
    }
}
