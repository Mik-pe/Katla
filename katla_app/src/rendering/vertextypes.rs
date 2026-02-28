use katla_vulkan::{VertexBinding, VertexFormat};

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexPosition {
    pub position: [f32; 3],
}

impl VertexPosition {
    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RGB32f],
        }
    }
}

// #[repr(C)]
// #[derive(Default, Debug, Clone)]
// pub struct VertexPos2Color {
//     pub position: [f32; 2],
//     pub color: [f32; 3],
// }

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexNormal {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl VertexNormal {
    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![VertexFormat::RGB32f, VertexFormat::RGB32f],
        }
    }
}

// #[repr(C)]
// #[derive(Default, Debug, Clone)]
// pub struct VertexNormalTangent {
//     pub position: [f32; 3],
//     pub normal: [f32; 3],
//     pub tangent: [f32; 4],
// }

#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexPBR {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coord0: [f32; 2],
}

impl VertexPBR {
    pub fn new(
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        tex_coord0: [f32; 2],
    ) -> Self {
        Self {
            position,
            normal,
            tangent,
            tex_coord0,
        }
    }

    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![
                VertexFormat::RGB32f,
                VertexFormat::RGB32f,
                VertexFormat::RGBA32f,
                VertexFormat::RG32f,
            ],
        }
    }
}

/// Skinned vertex with skeletal animation support.
///
/// Includes joint indices and weights for GPU skinning.
/// Each vertex can be influenced by up to 4 joints.
#[repr(C)]
#[derive(Default, Debug, Clone)]
pub struct VertexSkinned {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tangent: [f32; 4],
    pub tex_coord0: [f32; 2],
    pub joint_indices: [u16; 4], // Up to 4 joint influences (u16 - 65k joints is plenty)
    pub joint_weights: [f32; 4], // Weights must sum to 1.0
}

impl VertexSkinned {
    pub fn new(
        position: [f32; 3],
        normal: [f32; 3],
        tangent: [f32; 4],
        tex_coord0: [f32; 2],
        joint_indices: [u16; 4],
        joint_weights: [f32; 4],
    ) -> Self {
        Self {
            position,
            normal,
            tangent,
            tex_coord0,
            joint_indices,
            joint_weights,
        }
    }

    /// Create from a base VertexPBR with skinning data
    pub fn from_pbr(base: VertexPBR, joint_indices: [u16; 4], joint_weights: [f32; 4]) -> Self {
        Self {
            position: base.position,
            normal: base.normal,
            tangent: base.tangent,
            tex_coord0: base.tex_coord0,
            joint_indices,
            joint_weights,
        }
    }

    pub fn get_vertex_binding() -> VertexBinding {
        VertexBinding {
            formats: vec![
                VertexFormat::RGB32f,  // position (location 0)
                VertexFormat::RGB32f,  // normal (location 1)
                VertexFormat::RGBA32f, // tangent (location 2)
                VertexFormat::RG32f,   // uv (location 3)
                VertexFormat::RGBA16u, // joint_indices (location 4) - u16x4, 65k joints max
                VertexFormat::RGBA32f, // joint_weights (location 5)
            ],
        }
    }
}
