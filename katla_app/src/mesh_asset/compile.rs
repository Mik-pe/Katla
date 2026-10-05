//! Deterministic static mesh baking, including normals and tangent handedness.

use super::{Geometry, MeshAsset};
use crate::geometry_cache::MeshGeometryData;
use katla_gfx::{primitives, vertex::VertexPBR};
use katla_math::{AABB, Quat, Vec3};

/// Backend-neutral mesh output. No GPU resources are allocated while compiling.
pub struct CompiledMesh {
    pub vertices: Vec<VertexPBR>,
    pub indices: Vec<u32>,
    pub bounds: AABB,
    pub geometry: MeshGeometryData,
}

type D3 = [f64; 3];
fn add(a: D3, b: D3) -> D3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: D3, b: D3) -> D3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: D3, s: f64) -> D3 {
    a.map(|v| v * s)
}
fn dot(a: D3, b: D3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: D3, b: D3) -> D3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(v: D3) -> Option<D3> {
    let length = dot(v, v).sqrt();
    (length.is_finite() && length > 0.0).then(|| mul(v, 1.0 / length))
}
pub(super) fn unit(v: [f32; 3]) -> Option<[f32; 3]> {
    normalize(v.map(f64::from)).map(|v| v.map(|value| value as f32))
}
fn orthogonal(n: D3) -> D3 {
    let axis = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    // A unit normal and the selected nonparallel unit axis cannot give zero.
    normalize(cross(n, axis)).unwrap_or([0.0, 0.0, 1.0])
}

pub(super) fn compile(asset: &MeshAsset) -> Result<CompiledMesh, String> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for part in &asset.parts {
        let (mut v, mut i) =
            generate(&part.geometry).map_err(|error| format!("part '{}': {error}", part.id))?;
        let transform = &part.transform;
        let q = transform.rotation;
        let q_length = q.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
        let q = q.map(|v| (v as f64 / q_length) as f32);
        let rotation = Quat::new(q[0], q[1], q[2], q[3]).make_mat4();
        let rotate = |value: D3| -> D3 {
            std::array::from_fn(|row| {
                (0..3)
                    .map(|col| rotation[col][row] as f64 * value[col])
                    .sum()
            })
        };
        let reflected = transform.scale.iter().filter(|v| **v < 0.0).count() % 2 == 1;
        for vertex in &mut v {
            let position = rotate(std::array::from_fn(|axis| {
                vertex.position[axis] as f64 * transform.scale[axis] as f64
            }));
            vertex.position = std::array::from_fn(|axis| {
                (position[axis] + transform.position[axis] as f64) as f32
            });
            let normal = normalize(rotate(std::array::from_fn(|axis| {
                vertex.normal[axis] as f64 / transform.scale[axis] as f64
            })))
            .ok_or_else(|| format!("part '{}': invalid generated normal", part.id))?;
            let tangent = rotate(std::array::from_fn(|axis| {
                vertex.tangent[axis] as f64 * transform.scale[axis] as f64
            }));
            let tangent = normalize(sub(tangent, mul(normal, dot(normal, tangent))))
                .unwrap_or_else(|| orthogonal(normal));
            vertex.normal = normal.map(|v| v as f32);
            let sign = vertex.tangent[3] * if reflected { -1.0 } else { 1.0 };
            vertex.tangent = [
                tangent[0] as f32,
                tangent[1] as f32,
                tangent[2] as f32,
                sign,
            ];
            if vertex
                .position
                .iter()
                .chain(&vertex.normal)
                .chain(&vertex.tangent)
                .any(|v| !v.is_finite())
            {
                return Err(format!(
                    "part '{}': transformed geometry is not finite",
                    part.id
                ));
            }
        }
        if reflected {
            for triangle in i.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
        let offset = vertices.len() as u32;
        indices.extend(i.into_iter().map(|index| index + offset));
        vertices.extend(v);
    }
    let points: Vec<_> = vertices
        .iter()
        .map(|v| Vec3::new(v.position[0], v.position[1], v.position[2]))
        .collect();
    let bounds = AABB::create_from_verts(&points);
    let geometry = MeshGeometryData {
        positions: vertices.iter().map(|v| v.position).collect(),
        triangles: indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|i| [i[0], i[1], i[2]])
            .collect(),
    };
    Ok(CompiledMesh {
        vertices,
        indices,
        bounds,
        geometry,
    })
}

fn generate(geometry: &Geometry) -> Result<(Vec<VertexPBR>, Vec<u32>), String> {
    Ok(match geometry {
        Geometry::Cube { size } => primitives::generate_cube(*size),
        Geometry::Sphere {
            radius,
            segments,
            rings,
        } => primitives::generate_sphere(*radius, *segments, *rings),
        Geometry::Plane { width, height } => primitives::generate_plane(*width, *height),
        Geometry::Cylinder {
            radius,
            height,
            segments,
        } => primitives::generate_cylinder(*height, *radius, *segments),
        Geometry::Cone {
            radius,
            height,
            segments,
        } => primitives::generate_cone(*height, *radius, *segments),
        Geometry::Torus {
            radius,
            tube_radius,
            segments,
            tube_segments,
        } => primitives::generate_torus(*radius, *tube_radius, *segments, *tube_segments),
        Geometry::Triangles {
            positions,
            indices,
            normals,
            uvs,
        } => return triangles(positions, indices, normals.as_deref(), uvs.as_deref()),
    })
}

fn triangles(
    positions: &[[f32; 3]],
    indices: &[u32],
    normals: Option<&[[f32; 3]]>,
    uvs: Option<&[[f32; 2]]>,
) -> Result<(Vec<VertexPBR>, Vec<u32>), String> {
    let mut sums = vec![[0.0; 3]; positions.len()];
    let mut tangents = sums.clone();
    let mut bitangents = sums.clone();
    let mut used = vec![false; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let e1 = sub(positions[b].map(f64::from), positions[a].map(f64::from));
        let e2 = sub(positions[c].map(f64::from), positions[a].map(f64::from));
        let normal = cross(e1, e2);
        if normalize(normal).is_none() {
            return Err("Degenerate triangle with zero area".into());
        }
        for index in [a, b, c] {
            sums[index] = add(sums[index], normal);
            used[index] = true;
        }
        if let Some(uvs) = uvs {
            let du1 = uvs[b][0] as f64 - uvs[a][0] as f64;
            let dv1 = uvs[b][1] as f64 - uvs[a][1] as f64;
            let du2 = uvs[c][0] as f64 - uvs[a][0] as f64;
            let dv2 = uvs[c][1] as f64 - uvs[a][1] as f64;
            let det = du1 * dv2 - du2 * dv1;
            if det.abs() > 1e-12 {
                let tangent = mul(sub(mul(e1, dv2), mul(e2, dv1)), 1.0 / det);
                let bitangent = mul(sub(mul(e2, du1), mul(e1, du2)), 1.0 / det);
                for index in [a, b, c] {
                    tangents[index] = add(tangents[index], tangent);
                    bitangents[index] = add(bitangents[index], bitangent);
                }
            }
        }
    }
    let mut vertices = Vec::with_capacity(positions.len());
    for (index, &position) in positions.iter().enumerate() {
        if !used[index] {
            return Err(format!(
                "Vertex {index} is unused; remove it from the recipe"
            ));
        }
        let normal = normalize(
            normals
                .map(|n| n[index].map(f64::from))
                .unwrap_or(sums[index]),
        )
        .ok_or_else(|| format!("Vertex {index} needs an explicit normal; incident faces cancel"))?;
        let tangent = normalize(sub(
            tangents[index],
            mul(normal, dot(normal, tangents[index])),
        ))
        .unwrap_or_else(|| orthogonal(normal));
        let sign = if dot(cross(normal, tangent), bitangents[index]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertices.push(VertexPBR::new(
            position,
            normal.map(|v| v as f32),
            [
                tangent[0] as f32,
                tangent[1] as f32,
                tangent[2] as f32,
                sign,
            ],
            uvs.map(|v| v[index]).unwrap_or([0.0; 2]),
        ));
    }
    Ok((vertices, indices.to_vec()))
}
