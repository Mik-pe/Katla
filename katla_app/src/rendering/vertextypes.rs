use katla_vulkan::vertexbinding::{VertexBinding, VertexFormat};

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
