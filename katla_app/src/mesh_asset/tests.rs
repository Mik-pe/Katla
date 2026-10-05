use super::*;

pub(crate) fn cube() -> MeshAsset {
    MeshAsset {
        version: MESH_VERSION,
        name: "Block".into(),
        parts: vec![MeshPart {
            id: "body".into(),
            transform: TransformDescriptor::default_transform(),
            geometry: Geometry::Cube { size: [1.0; 3] },
        }],
    }
}

#[test]
fn test_mesh_ron_and_json_preserve_named_parts_and_geometry() {
    let mesh = cube();
    let text = ron::ser::to_string_pretty(&mesh, crate::scene::ron_pretty_config()).unwrap();
    assert_eq!(MeshAsset::parse(&text).unwrap(), mesh);
    assert_eq!(
        serde_json::from_value::<MeshAsset>(serde_json::to_value(&mesh).unwrap()).unwrap(),
        mesh
    );
    let compiled = mesh.compile().unwrap();
    assert_eq!(compiled.vertices.len(), 24);
    assert_eq!(compiled.indices.len(), 36);
    assert_eq!(compiled.bounds.min().to_array(), [-0.5; 3]);
}

#[test]
fn test_mesh_combines_parts_into_one_stream_and_metadata_does_not_change_cache_key() {
    let mut mesh = cube();
    let first_key = mesh.geometry_key().unwrap();
    mesh.name = "Renamed".into();
    mesh.parts[0].id = "renamed part".into();
    assert_eq!(mesh.geometry_key().unwrap(), first_key);
    let mut second = mesh.parts[0].clone();
    second.id = "leg".into();
    second.transform.position[0] = 3.0;
    mesh.parts.push(second);
    let compiled = mesh.compile().unwrap();
    assert_eq!(compiled.vertices.len(), 48);
    assert_eq!(compiled.indices.len(), 72);
    assert!(
        compiled.indices[36..]
            .iter()
            .all(|index| *index >= 24 && *index < 48)
    );
    assert_eq!(compiled.bounds.max().to_array(), [3.5, 0.5, 0.5]);
}

fn triangle() -> MeshAsset {
    let mut mesh = cube();
    mesh.parts[0].geometry = Geometry::Triangles {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        indices: vec![0, 1, 2],
        normals: None,
        uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
    };
    mesh
}

#[test]
fn test_indexed_mesh_generates_normals_uv_tangents_and_reflected_winding() {
    let mut mesh = triangle();
    let compiled = mesh.compile().unwrap();
    assert_eq!(compiled.vertices[0].normal, [0.0, 0.0, 1.0]);
    assert_eq!(compiled.vertices[0].tangent, [1.0, 0.0, 0.0, 1.0]);
    mesh.parts[0].transform.scale = [-2.0, 3.0, 1.0];
    mesh.parts[0].transform.position = [4.0, 2.0, 1.0];
    let compiled = mesh.compile().unwrap();
    assert_eq!(compiled.indices, [0, 2, 1]);
    assert_eq!(compiled.vertices[1].position, [2.0, 2.0, 1.0]);
    assert_eq!(compiled.vertices[0].tangent, [-1.0, 0.0, 0.0, -1.0]);
    assert_eq!(compiled.vertices[0].normal, [0.0, 0.0, 1.0]);
}

#[test]
fn test_nonuniform_rotated_parts_use_inverse_transpose_normals() {
    let mut mesh = triangle();
    if let Geometry::Triangles { normals, .. } = &mut mesh.parts[0].geometry {
        *normals = Some(vec![[1.0, 1.0, 0.0]; 3]);
    }
    mesh.parts[0].transform.scale = [2.0, 1.0, 1.0];
    mesh.parts[0].transform.rotation = [
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ];
    let compiled = mesh.compile().unwrap();
    let n = compiled.vertices[0].normal;
    assert!((n[0] + 2.0 / 5.0f32.sqrt()).abs() < 1e-5);
    assert!((n[1] - 1.0 / 5.0f32.sqrt()).abs() < 1e-5);
    let t = compiled.vertices[0].tangent;
    assert!((n[0] * t[0] + n[1] * t[1] + n[2] * t[2]).abs() < 1e-5);
}

#[test]
fn test_mesh_rejects_invalid_topology_unused_vertices_and_degenerate_faces() {
    for indices in [vec![0, 1], vec![0, 1, 999], vec![0, 0, 2]] {
        let mut mesh = triangle();
        if let Geometry::Triangles { indices: value, .. } = &mut mesh.parts[0].geometry {
            *value = indices;
        }
        assert!(mesh.compile().is_err());
    }
    let mut mesh = triangle();
    if let Geometry::Triangles { positions, uvs, .. } = &mut mesh.parts[0].geometry {
        positions.push([1.0; 3]);
        *uvs = None;
    }
    assert!(mesh.compile().is_err());
    let mut mesh = cube();
    mesh.parts[0].geometry = Geometry::Sphere {
        radius: 1.0,
        segments: 8,
        rings: 2,
    };
    assert!(mesh.compile().is_err());
    mesh.parts[0].geometry = Geometry::Sphere {
        radius: 1.0,
        segments: u32::MAX,
        rings: u32::MAX,
    };
    assert!(mesh.compile().is_err());
    mesh.parts[0].geometry = Geometry::Cone {
        radius: 1.0,
        height: 1.0,
        segments: u32::MAX,
    };
    assert!(mesh.compile().is_err());
}

#[test]
fn test_combined_budget_duplicate_ids_bad_transforms_and_future_versions_fail() {
    let mut mesh = cube();
    mesh.parts.push(mesh.parts[0].clone());
    assert!(mesh.validate().is_err());
    mesh.parts[1].id = "second".into();
    for part in &mut mesh.parts {
        part.geometry = Geometry::Sphere {
            radius: 1.0,
            segments: 999,
            rings: 999,
        };
    }
    assert!(mesh.compile().is_err());
    let mut mesh = cube();
    mesh.parts[0].transform.scale[0] = 0.0;
    assert!(mesh.compile().is_err());
    mesh = cube();
    mesh.version += 1;
    assert!(mesh.validate().is_err());
    let text = ron::to_string(&cube()).unwrap().replace("name:", "typo:");
    assert!(MeshAsset::parse(&text).is_err());
}

#[test]
fn test_every_primitive_compiles_with_finite_orthonormal_vertex_frames() {
    for geometry in [
        Geometry::Cube { size: [1.0; 3] },
        Geometry::Sphere {
            radius: 1.0,
            segments: 12,
            rings: 8,
        },
        Geometry::Plane {
            width: 1.0,
            height: 1.0,
        },
        Geometry::Cylinder {
            radius: 0.5,
            height: 1.0,
            segments: 12,
        },
        Geometry::Cone {
            radius: 0.5,
            height: 1.0,
            segments: 12,
        },
        Geometry::Torus {
            radius: 1.0,
            tube_radius: 0.2,
            segments: 12,
            tube_segments: 8,
        },
    ] {
        let mut mesh = cube();
        mesh.parts[0].geometry = geometry;
        let compiled = mesh.compile().unwrap();
        for v in compiled.vertices {
            assert!(
                v.position
                    .iter()
                    .chain(&v.normal)
                    .chain(&v.tangent)
                    .all(|v| v.is_finite())
            );
            assert!((v.normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-5);
            assert!(
                (v.normal
                    .iter()
                    .zip(v.tangent)
                    .map(|(a, b)| a * b)
                    .sum::<f32>())
                .abs()
                    < 1e-5
            );
        }
    }
}
