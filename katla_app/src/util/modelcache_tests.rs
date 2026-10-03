use super::*;
use katla_math::Quat;

fn decode(document: serde_json::Value, data: Vec<u8>) -> Result<GLTFModel, String> {
    let document =
        gltf::Gltf::from_slice_without_validation(&serde_json::to_vec(&document).unwrap())
            .unwrap()
            .document;
    GLTFModel::from_parts(document, vec![BufferData(data)], vec![])
}

fn triangle_document() -> serde_json::Value {
    serde_json::json!({
        "asset":{"version":"2.0"}, "scene":1,
        "scenes":[{"nodes":[0]}, {"nodes":[1]}],
        "nodes":[{"mesh":0}, {"mesh":1}],
        "buffers":[{"byteLength":36}], "bufferViews":[{"buffer":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]},
                  {"primitives":[{"attributes":{"POSITION":0},"material":1},{"attributes":{"POSITION":0}}]}],
        "materials":[{}, {"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.4,0.6,0.8],"metallicFactor":0.7,"roughnessFactor":0.3}}]
    })
}

fn triangle_bytes() -> Vec<u8> {
    bytemuck::cast_slice(&[[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]).to_vec()
}

#[test]
fn test_selected_scene_retains_per_primitive_and_implicit_materials() {
    let model = decode(triangle_document(), triangle_bytes()).unwrap();
    assert_eq!(model.primitives.len(), 2);
    assert!(
        model
            .primitives
            .iter()
            .all(|primitive| primitive.node_index == 1)
    );
    assert_eq!(
        model.primitives[0].material.base_color_factor,
        [0.2, 0.4, 0.6, 0.8]
    );
    assert_eq!(model.primitives[0].material.metallic_factor, 0.7);
    assert_eq!(model.primitives[1].material.metallic_factor, 1.0);
    assert_eq!(model.primitives[1].material.base_color_factor, [1.0; 4]);
    assert_eq!(model.primitives[1].indices, [0, 1, 2]);
}

#[test]
fn test_mixed_index_widths_and_nonindexed_primitives_stay_independent() {
    let mut document = triangle_document();
    let mut data = triangle_bytes();
    data.extend_from_slice(&[0, 1, 2, 0]);
    data.extend_from_slice(bytemuck::cast_slice(&[2u32, 1, 0]));
    document["buffers"][0]["byteLength"] = serde_json::json!(data.len());
    document["bufferViews"].as_array_mut().unwrap().extend([
        serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":3}),
        serde_json::json!({"buffer":0,"byteOffset":40,"byteLength":12}),
    ]);
    document["accessors"].as_array_mut().unwrap().extend([
        serde_json::json!({"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}),
        serde_json::json!({"bufferView":2,"componentType":5125,"count":3,"type":"SCALAR"}),
    ]);
    document["meshes"][1]["primitives"][0]["indices"] = serde_json::json!(1);
    document["meshes"][1]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"attributes":{"POSITION":0},"indices":2}));
    let model = decode(document, data).unwrap();
    assert_eq!(model.primitives[0].indices, [0, 1, 2]);
    assert_eq!(model.primitives[1].indices, [0, 1, 2]);
    assert_eq!(model.primitives[2].indices, [0, 1, 2]);
    assert_eq!(
        model.primitives[2].vertices.positions(),
        [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]]
    );
}

#[test]
fn test_static_node_matrix_preserves_shear_and_mirrored_winding() {
    let mut document = triangle_document();
    document["nodes"][1]["matrix"] =
        serde_json::json!([-2, 0, 0, 0, 0.5, 1, 0, 0, 0, 0, 1, 0, 4, 5, 6, 1]);
    let model = decode(document, triangle_bytes()).unwrap();
    assert_eq!(
        model.primitives[0].vertices.positions(),
        [[4.0, 5.0, 6.0], [2.0, 5.0, 6.0], [4.5, 6.0, 6.0]]
    );
    assert_eq!(model.primitives[0].indices, [0, 2, 1]);
}

#[test]
fn test_interleaved_accessor_reads_all_vertices_with_offsets() {
    let mut document = triangle_document();
    let data = bytemuck::cast_slice(&[
        99.0f32, 0.0, 0.0, 0.0, 98.0, 1.0, 0.0, 0.0, 97.0, 0.0, 1.0, 0.0,
    ])
    .to_vec();
    document["buffers"][0]["byteLength"] = serde_json::json!(48);
    document["bufferViews"][0]["byteLength"] = serde_json::json!(48);
    document["bufferViews"][0]["byteStride"] = serde_json::json!(16);
    document["accessors"][0]["byteOffset"] = serde_json::json!(4);
    assert_eq!(
        decode(document, data).unwrap().primitives[0]
            .vertices
            .positions(),
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
}

#[test]
fn test_sparse_positions_without_a_base_buffer_view() {
    let mut document = triangle_document();
    let mut data = vec![1, 2, 0, 0];
    data.extend_from_slice(bytemuck::cast_slice(&[[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0]]));
    document["buffers"][0]["byteLength"] = serde_json::json!(28);
    document["bufferViews"] = serde_json::json!([{"buffer":0,"byteLength":2},{"buffer":0,"byteOffset":4,"byteLength":24}]);
    document["accessors"][0]
        .as_object_mut()
        .unwrap()
        .remove("bufferView");
    document["accessors"][0]["sparse"] = serde_json::json!({"count":2,"indices":{"bufferView":0,"componentType":5121},"values":{"bufferView":1}});
    assert_eq!(
        decode(document, data).unwrap().primitives[0]
            .vertices
            .positions(),
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
}

#[test]
fn test_invalid_topology_attribute_count_and_index_reject_import() {
    let mut document = triangle_document();
    document["meshes"][1]["primitives"][0]["mode"] = serde_json::json!(1);
    assert!(
        decode(document, triangle_bytes())
            .err()
            .unwrap()
            .contains("topology")
    );
    let mut document = triangle_document();
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"bufferView":0,"componentType":5126,"count":1,"type":"VEC3"}));
    document["meshes"][1]["primitives"][0]["attributes"]["NORMAL"] = serde_json::json!(1);
    assert!(
        decode(document, triangle_bytes())
            .err()
            .unwrap()
            .contains("NORMAL count")
    );
    let mut document = triangle_document();
    let mut data = triangle_bytes();
    data.extend_from_slice(&[0, 1, 3]);
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":3}));
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"bufferView":1,"componentType":5121,"count":3,"type":"SCALAR"}));
    document["meshes"][1]["primitives"][0]["indices"] = serde_json::json!(1);
    assert!(
        decode(document, data)
            .err()
            .unwrap()
            .contains("outside POSITION")
    );
}

#[test]
fn test_bundled_models_import_each_selected_primitive() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/models");
    for name in ["Fox.glb", "Box.glb", "Lantern.glb"] {
        let model = GLTFModel::new(root.join(name)).unwrap();
        assert!(!model.primitives.is_empty(), "{name}");
        for primitive in &model.primitives {
            let positions = primitive.vertices.positions();
            assert!(!positions.is_empty());
            assert!(!primitive.indices.is_empty());
            assert!(
                primitive
                    .indices
                    .iter()
                    .all(|index| (*index as usize) < positions.len())
            );
        }
    }
}

#[test]
fn test_generated_tangents_follow_uvs_and_flat_normals() {
    let mut document = triangle_document();
    let mut data = triangle_bytes();
    data.extend_from_slice(bytemuck::cast_slice(&[
        [0.0f32, 0.0],
        [0.0, 1.0],
        [1.0, 0.0],
    ]));
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":24}));
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}));
    document["meshes"][1]["primitives"][0]["attributes"]["TEXCOORD_0"] = serde_json::json!(1);
    let model = decode(document, data).unwrap();
    let GltfVertices::Static(vertices) = &model.primitives[0].vertices else {
        panic!("static")
    };
    for vertex in vertices {
        assert_eq!(vertex.normal, [0.0, 0.0, 1.0]);
        assert_eq!(vertex.tangent, [0.0, 1.0, 0.0, -1.0]);
    }
}

#[test]
fn test_secondary_uvs_preserve_normalized_values_and_reject_short_accessors() {
    let mut document = triangle_document();
    let mut data = triangle_bytes();
    data.extend_from_slice(bytemuck::cast_slice(&[
        [0u16, 65535],
        [65535, 32768],
        [32768, 0],
    ]));
    document["buffers"][0]["byteLength"] = serde_json::json!(data.len());
    document["bufferViews"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":12}));
    document["accessors"].as_array_mut().unwrap().push(serde_json::json!({"bufferView":1,"componentType":5123,"count":3,"type":"VEC2","normalized":true}));
    document["meshes"][1]["primitives"][0]["attributes"]["TEXCOORD_1"] = serde_json::json!(1);
    let model = decode(document.clone(), data.clone()).unwrap();
    let GltfVertices::Static(vertices) = &model.primitives[0].vertices else {
        panic!("static");
    };
    assert_eq!(vertices[0].tex_coord0, [0.; 2]);
    assert_eq!(vertices[0].tex_coord1, [0., 1.]);
    assert_eq!(vertices[1].tex_coord1, [1., 32768. / 65535.]);
    assert_eq!(vertices[2].tex_coord1, [32768. / 65535., 0.]);
    document["accessors"][1]["count"] = serde_json::json!(2);
    assert!(
        decode(document, data)
            .err()
            .unwrap()
            .contains("TEXCOORD_1 count")
    );
}

#[test]
fn test_skinned_primitive_keeps_tangents_and_its_nodes_skin() {
    let mut document = triangle_document();
    let mut data = triangle_bytes();
    data.extend_from_slice(bytemuck::cast_slice(&[[0u16, 0, 0, 0]; 3]));
    data.extend_from_slice(bytemuck::cast_slice(&[[2.0f32, 0.0, 0.0, 0.0]; 3]));
    data.extend_from_slice(bytemuck::cast_slice(&[[0.0f32, 1.0, 0.0, -1.0]; 3]));
    document["nodes"] =
        serde_json::json!([{"mesh":0},{"mesh":1,"skin":1,"translation":[4,5,6]}, {}, {}]);
    document["scenes"][1]["nodes"] = serde_json::json!([1, 2, 3]);
    document["skins"] = serde_json::json!([{"joints":[2]},{"joints":[3]}]);
    document["bufferViews"].as_array_mut().unwrap().extend([
        serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":24}),
        serde_json::json!({"buffer":0,"byteOffset":60,"byteLength":48}),
        serde_json::json!({"buffer":0,"byteOffset":108,"byteLength":48}),
    ]);
    document["accessors"].as_array_mut().unwrap().extend([
        serde_json::json!({"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"}),
        serde_json::json!({"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}),
        serde_json::json!({"bufferView":3,"componentType":5126,"count":3,"type":"VEC4"}),
    ]);
    for primitive in document["meshes"][1]["primitives"].as_array_mut().unwrap() {
        primitive["attributes"]["JOINTS_0"] = serde_json::json!(1);
        primitive["attributes"]["WEIGHTS_0"] = serde_json::json!(2);
        primitive["attributes"]["TANGENT"] = serde_json::json!(3);
    }
    let model = decode(document, data).unwrap();
    assert_eq!(model.primitives[0].skin_index, Some(1));
    let GltfVertices::Skinned(vertices) = &model.primitives[0].vertices else {
        panic!("skinned")
    };
    assert_eq!(
        vertices[0].position, [0.0; 3],
        "Skinned geometry must not bake the mesh node transform"
    );
    assert_eq!(vertices[0].tangent, [0.0, 1.0, 0.0, -1.0]);
    assert_eq!(vertices[0].joint_weights, [1.0, 0.0, 0.0, 0.0]);
    assert_ne!(
        model.primitives[0].material.base_color_factor,
        model.primitives[1].material.base_color_factor
    );
}

#[test]
fn test_repeated_scene_nodes_reject_cycles_and_multiple_parents() {
    let mut document = triangle_document();
    document["nodes"][1]["children"] = serde_json::json!([1]);
    assert!(
        decode(document, triangle_bytes())
            .err()
            .unwrap()
            .contains("repeats node")
    );
}
#[test]
fn test_static_material_frame_bakes_nonuniform_and_mirrored_transforms() {
    for mirror in [1.0, -1.0] {
        let mut vertex = VertexPBR {
            position: [1.0, 2.0, 3.0],
            normal: [
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0; 2],
            tex_coord1: [0.0; 2],
        };
        GLTFModel::transform_vertex_data(
            std::slice::from_mut(&mut vertex),
            &Mat4::from_trs(
                Vec3::new(4.0, 5.0, 6.0),
                Quat::identity(),
                Vec3::new(2.0 * mirror, 1.0, 0.5),
            ),
        );
        assert_eq!(vertex.position, [4.0 + 2.0 * mirror, 7.0, 7.5]);
        for (actual, expected) in
            vertex
                .normal
                .into_iter()
                .zip([0.0, 1.0 / 5.0f32.sqrt(), 2.0 / 5.0f32.sqrt()])
        {
            assert!(
                (actual - expected).abs() < 1e-6,
                "normal must use inverse transpose: {:?}",
                vertex.normal
            );
        }
        assert_eq!(vertex.tangent, [mirror, 0.0, 0.0, mirror]);
    }
}

#[test]
fn test_static_material_frame_stays_finite_for_singular_transform() {
    let mut vertex = VertexPBR {
        position: [0.0; 3],
        normal: [0.0; 3],
        tangent: [0.0, 0.0, 0.0, 1.0],
        tex_coord0: [0.0; 2],
        tex_coord1: [0.0; 2],
    };
    GLTFModel::transform_vertex_data(
        std::slice::from_mut(&mut vertex),
        &Mat4::from_trs(Vec3::ZERO, Quat::identity(), Vec3::ZERO),
    );
    assert_eq!(vertex.normal, [0.0, 1.0, 0.0]);
    assert_eq!(vertex.tangent, [0.0, 0.0, 1.0, 1.0]);
}
