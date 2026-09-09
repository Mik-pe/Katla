//! Typed mesh descriptors for issue #90.
//!
//! Mesh creation takes an explicit vertex layout (from a trusted `Vertex`
//! implementation) and typed index elements. Nothing is guessed from byte
//! shapes: inconsistent declarations, out-of-range indices, and unsupported
//! topologies fail with typed errors before any GPU upload.
//!
//! Pure descriptor tests run everywhere; upload tests need a Vulkan device
//! (`#[ignore]`, like the other GPU contract tests).

use std::ffi::CString;

use katla_gfx::{
    AttributeType, IndexType, MeshDescriptor, MeshUsage, PrimitiveTopology, RendererError,
    ValidationMode, Vertex, VertexAttributeFormat, VertexLayout, VertexPBR, VertexPBRSkinned,
    VertexPosition, VertexPositionColor, VertexPositionNormal, VertexPositionNormalUV, VertexUI,
    VulkanRenderer,
};

// ---------------------------------------------------------------------------
// Pure tests (no device)
// ---------------------------------------------------------------------------

fn check_vertex_type<V: Vertex>(expected_stride: usize, expected_attrs: &[AttributeType]) {
    assert_eq!(std::mem::size_of::<V>(), expected_stride);
    assert_eq!(V::layout().stride(), expected_stride);
    assert_eq!(V::attribute_kinds(), expected_attrs);
}

#[test]
fn test_all_vertex_types_agree_on_stride_and_semantics() {
    use AttributeType::*;
    check_vertex_type::<VertexPBR>(48, &[Position, Normal, Tangent, TexCoord0]);
    check_vertex_type::<VertexPBRSkinned>(
        72,
        &[
            Position,
            Normal,
            Tangent,
            TexCoord0,
            JointIndices,
            JointWeights,
        ],
    );
    check_vertex_type::<VertexPosition>(12, &[Position]);
    check_vertex_type::<VertexPositionNormal>(24, &[Position, Normal]);
    check_vertex_type::<VertexPositionNormalUV>(32, &[Position, Normal, TexCoord0]);
    check_vertex_type::<VertexPositionColor>(28, &[Position, Color0]);
    check_vertex_type::<VertexUI>(24, &[Position, TexCoord0, Color0, TextureIndex]);
}

#[test]
fn test_layout_and_attributes_must_agree() {
    // A custom layout with semantics for every format validates.
    let descriptor = MeshDescriptor {
        layout: VertexLayout::new(vec![
            VertexAttributeFormat::Float3,
            VertexAttributeFormat::Float2,
        ]),
        attributes: vec![AttributeType::Position, AttributeType::TexCoord0],
        topology: PrimitiveTopology::TriangleList,
        usage: MeshUsage::Static,
        vertex_count: 3,
        index_count: 3,
        index_format: IndexType::Uint16,
    };
    assert!(descriptor.validate(20).is_ok());

    // Fewer semantics than formats: rejected, not guessed.
    let mismatched = MeshDescriptor {
        attributes: vec![AttributeType::Position],
        ..descriptor.clone()
    };
    assert!(matches!(
        mismatched.validate(20),
        Err(RendererError::InvalidDescriptor { .. })
    ));

    // Struct stride disagreeing with the layout: rejected.
    assert!(matches!(
        descriptor.validate(24),
        Err(RendererError::InvalidDescriptor { .. })
    ));

    // Empty vertex list: rejected.
    let empty = MeshDescriptor {
        vertex_count: 0,
        ..descriptor.clone()
    };
    assert!(matches!(
        empty.validate(20),
        Err(RendererError::InvalidDescriptor { .. })
    ));

    // No Position attribute: rejected (draw paths require one).
    let positionless = MeshDescriptor {
        layout: VertexLayout::new(vec![VertexAttributeFormat::Float3]),
        attributes: vec![AttributeType::Normal],
        ..descriptor.clone()
    };
    assert!(matches!(
        positionless.validate(12),
        Err(RendererError::InvalidDescriptor { .. })
    ));
}

#[test]
fn test_split_validation_checks_each_buffer() {
    use std::collections::HashMap;
    let descriptor = MeshDescriptor {
        layout: VertexLayout::position_normal(),
        attributes: vec![AttributeType::Position, AttributeType::Normal],
        topology: PrimitiveTopology::TriangleList,
        usage: MeshUsage::Static,
        vertex_count: 2,
        index_count: 3,
        index_format: IndexType::Uint32,
    };
    let mut attributes = HashMap::new();
    attributes.insert(AttributeType::Position, vec![0u8; 2 * 12]);
    attributes.insert(AttributeType::Normal, vec![0u8; 2 * 12]);
    assert!(descriptor.validate_split(&attributes).is_ok());

    // Short normal buffer: rejected with byte counts, not truncated.
    attributes.insert(AttributeType::Normal, vec![0u8; 12]);
    let error = descriptor.validate_split(&attributes).unwrap_err();
    match error {
        RendererError::InvalidDescriptor { reason, .. } => assert!(reason.contains("24")),
        other => panic!("expected InvalidDescriptor, got {other:?}"),
    }
}

#[test]
fn test_for_attributes_derives_canonical_layout() {
    let layout = VertexLayout::for_attributes(&[
        AttributeType::TexCoord0,
        AttributeType::Position,
        AttributeType::Normal,
    ]);
    assert_eq!(layout, VertexLayout::position_normal_uv());
}

// ---------------------------------------------------------------------------
// Device tests (ignored without a GPU)
// ---------------------------------------------------------------------------

fn headless_renderer() -> VulkanRenderer {
    VulkanRenderer::init_headless(
        64,
        48,
        ValidationMode::Disabled,
        CString::new("Mesh descriptor test").unwrap(),
        CString::new("Katla").unwrap(),
    )
    .unwrap()
}

fn pbr_triangle() -> (Vec<VertexPBR>, Vec<u32>) {
    (
        vec![
            VertexPBR::from_position([-0.5, -0.5, 0.0]),
            VertexPBR::from_position([0.5, -0.5, 0.0]),
            VertexPBR::from_position([0.0, 0.5, 0.0]),
        ],
        vec![0, 1, 2],
    )
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_typed_mesh_creation_accepts_valid_descriptors() {
    let mut renderer = headless_renderer();
    let (vertices, indices) = pbr_triangle();
    let handle = renderer
        .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleList)
        .unwrap();
    assert_eq!(renderer.mesh_index_format(handle), Some(IndexType::Uint32));

    // u16 indices keep their width end to end.
    let indices_u16: Vec<u16> = vec![0, 1, 2];
    let handle_u16 = renderer
        .create_mesh(&vertices, &indices_u16, PrimitiveTopology::TriangleList)
        .unwrap();
    assert_eq!(
        renderer.mesh_index_format(handle_u16),
        Some(IndexType::Uint16)
    );

    // The recorded descriptor is explicit and complete.
    let registry_mesh = renderer
        .mesh_index_format(handle)
        .expect("mesh must be live");
    assert_eq!(registry_mesh, IndexType::Uint32);

    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_mesh_creation_rejects_out_of_range_indices() {
    let mut renderer = headless_renderer();
    let (vertices, _) = pbr_triangle();
    let error = renderer
        .create_mesh(&vertices, &[0u32, 1, 99], PrimitiveTopology::TriangleList)
        .unwrap_err();
    match error {
        RendererError::InvalidDescriptor { reason, .. } => assert!(reason.contains("99")),
        other => panic!("expected InvalidDescriptor, got {other:?}"),
    }
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_mesh_creation_rejects_empty_vertices() {
    let mut renderer = headless_renderer();
    let vertices: Vec<VertexPBR> = Vec::new();
    let error = renderer
        .create_mesh(&vertices, &[0u32, 1, 2], PrimitiveTopology::TriangleList)
        .unwrap_err();
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_mesh_creation_rejects_unsupported_topology() {
    let mut renderer = headless_renderer();
    let (vertices, indices) = pbr_triangle();
    let error = renderer
        .create_mesh(&vertices, &indices, PrimitiveTopology::TriangleStrip)
        .unwrap_err();
    assert!(
        matches!(error, RendererError::UnsupportedFeature(_)),
        "non-list topology must fail explicitly, got {error:?}"
    );
    renderer.destroy();
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_mesh_creation_rejects_inconsistent_custom_layout() {
    // A custom trusted Vertex impl whose stride lies about its layout.
    #[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
    #[repr(C)]
    struct LyingVertex {
        position: [f32; 3],
        padding: [f32; 2],
    }
    impl Vertex for LyingVertex {
        fn layout() -> VertexLayout {
            VertexLayout::position()
        }
        fn attribute_kinds() -> Vec<AttributeType> {
            vec![AttributeType::Position]
        }
    }
    let mut renderer = headless_renderer();
    let vertices = vec![
        LyingVertex {
            position: [0.0, 0.0, 0.0],
            padding: [0.0, 0.0],
        };
        3
    ];
    // 20-byte stride vs 12-byte layout stride: rejected, not guessed.
    let error = renderer
        .create_mesh(&vertices, &[0u32, 1, 2], PrimitiveTopology::TriangleList)
        .unwrap_err();
    assert!(
        matches!(error, RendererError::InvalidDescriptor { .. }),
        "got {error:?}"
    );
    renderer.destroy();
}
